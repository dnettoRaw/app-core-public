// =============================================================================
//        #######
//     ###       ###     F: async_sink.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Explicit bounded asynchronous delivery for callers that accept queueing.

use crate::{LogError, LogEvent, LogSink};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

const ASYNC_LOG_THREAD_STACK_BYTES: usize = 256 * 1024;
/// Maximum events accepted by one asynchronous sink configuration.
pub const MAX_ASYNC_LOG_EVENTS: usize = 65_536;
/// Maximum estimated retained bytes accepted by one asynchronous sink.
pub const MAX_ASYNC_LOG_BYTES: usize = 64 * 1024 * 1024;

/// Count and retained-byte ceilings for one asynchronous sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncSinkConfig {
    /// Maximum events retained across the queue and active delivery.
    pub max_events: usize,
    /// Maximum estimated event bytes retained across queue and active delivery.
    pub max_bytes: usize,
}

/// Lock-free observation of one asynchronous sink.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AsyncSinkStats {
    /// Events currently retained by the queue or worker.
    pub retained_events: usize,
    /// Estimated event bytes currently retained by the queue or worker.
    pub retained_bytes: usize,
    /// Events delivered successfully.
    pub delivered: u64,
    /// Inner sink delivery failures.
    pub failures: u64,
    /// Events rejected because either queue ceiling was reached.
    pub rejected: u64,
}

enum Message {
    Event(Box<LogEvent>, usize),
    Flush(SyncSender<()>),
    Shutdown,
}

struct Counters {
    retained_events: AtomicUsize,
    retained_bytes: AtomicUsize,
    delivered: AtomicU64,
    failures: AtomicU64,
    rejected: AtomicU64,
}

impl Counters {
    fn new() -> Self {
        Self {
            retained_events: AtomicUsize::new(0),
            retained_bytes: AtomicUsize::new(0),
            delivered: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
        }
    }
}

struct Lifecycle {
    sender: SyncSender<Message>,
    worker: Option<JoinHandle<()>>,
}

/// Opt-in non-blocking wrapper around one sink.
///
/// `emit` rejects immediately with [`LogError::Capacity`] when either bound is
/// exhausted. Call [`Self::flush`] or [`Self::shutdown`] at an explicit
/// lifecycle boundary; dropping the last owner also shuts down and joins the
/// worker.
pub struct AsyncSink {
    config: AsyncSinkConfig,
    lifecycle: Mutex<Lifecycle>,
    counters: Arc<Counters>,
    closed: AtomicBool,
    accepts_sensitive: bool,
}

impl AsyncSink {
    /// Starts one bounded worker around `sink`.
    pub fn new(config: AsyncSinkConfig, sink: Arc<dyn LogSink>) -> Result<Self, LogError> {
        if config.max_events == 0
            || config.max_events > MAX_ASYNC_LOG_EVENTS
            || config.max_bytes == 0
            || config.max_bytes > MAX_ASYNC_LOG_BYTES
        {
            return Err(LogError::Capacity);
        }
        let (sender, receiver) = mpsc::sync_channel(config.max_events);
        let counters = Arc::new(Counters::new());
        let worker_counters = Arc::clone(&counters);
        let accepts_sensitive = sink.accepts_sensitive();
        let worker = thread::Builder::new()
            .name("appcore-log-sink".to_string())
            .stack_size(ASYNC_LOG_THREAD_STACK_BYTES)
            .spawn(move || worker_loop(receiver, sink, &worker_counters))
            .map_err(|_| LogError::Io)?;
        Ok(Self {
            config,
            lifecycle: Mutex::new(Lifecycle {
                sender,
                worker: Some(worker),
            }),
            counters,
            closed: AtomicBool::new(false),
            accepts_sensitive,
        })
    }

    /// Waits until every event admitted before this call has been delivered.
    pub fn flush(&self) -> Result<(), LogError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(LogError::Io);
        }
        let lifecycle = self.lifecycle.lock();
        let (sender, receiver) = mpsc::sync_channel(0);
        lifecycle
            .sender
            .send(Message::Flush(sender))
            .map_err(|_| LogError::Io)?;
        receiver.recv().map_err(|_| LogError::Io)
    }

    /// Drains admitted events, terminates the worker and rejects future emits.
    ///
    /// This waits for the wrapped sink. Use only with a sink whose own I/O has
    /// a bounded completion contract; Rust cannot terminate arbitrary blocking
    /// sink code safely.
    pub fn shutdown(&self) -> Result<(), LogError> {
        if self.closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let (sent, worker) = {
            let mut lifecycle = self.lifecycle.lock();
            (
                lifecycle.sender.send(Message::Shutdown).is_ok(),
                lifecycle.worker.take(),
            )
        };
        let joined = worker.ok_or(LogError::Io)?.join().is_ok();
        if sent && joined {
            Ok(())
        } else {
            Err(LogError::Io)
        }
    }

    /// Returns bounded queue and delivery counters without waiting for I/O.
    pub fn stats(&self) -> AsyncSinkStats {
        AsyncSinkStats {
            retained_events: self.counters.retained_events.load(Ordering::Relaxed),
            retained_bytes: self.counters.retained_bytes.load(Ordering::Relaxed),
            delivered: self.counters.delivered.load(Ordering::Relaxed),
            failures: self.counters.failures.load(Ordering::Relaxed),
            rejected: self.counters.rejected.load(Ordering::Relaxed),
        }
    }

    fn reserve(&self, bytes: usize) -> Result<(), LogError> {
        if bytes > self.config.max_bytes
            || !reserve_bounded(&self.counters.retained_events, 1, self.config.max_events)
        {
            increment(&self.counters.rejected);
            return Err(LogError::Capacity);
        }
        if !reserve_bounded(&self.counters.retained_bytes, bytes, self.config.max_bytes) {
            self.counters
                .retained_events
                .fetch_sub(1, Ordering::Relaxed);
            increment(&self.counters.rejected);
            return Err(LogError::Capacity);
        }
        Ok(())
    }

    fn release(&self, bytes: usize) {
        self.counters
            .retained_events
            .fetch_sub(1, Ordering::Relaxed);
        self.counters
            .retained_bytes
            .fetch_sub(bytes, Ordering::Relaxed);
    }
}

impl LogSink for AsyncSink {
    fn emit(&self, event: &LogEvent) -> Result<(), LogError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(LogError::Io);
        }
        let bytes = event.retained_bytes();
        self.reserve(bytes)?;
        let lifecycle = self.lifecycle.lock();
        if self.closed.load(Ordering::Acquire) {
            self.release(bytes);
            return Err(LogError::Io);
        }
        match lifecycle
            .sender
            .try_send(Message::Event(Box::new(event.clone()), bytes))
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                self.release(bytes);
                increment(&self.counters.rejected);
                Err(LogError::Capacity)
            }
            Err(TrySendError::Disconnected(_)) => {
                self.release(bytes);
                Err(LogError::Io)
            }
        }
    }

    fn accepts_sensitive(&self) -> bool {
        self.accepts_sensitive
    }

    fn name(&self) -> &'static str {
        "async"
    }
}

impl Drop for AsyncSink {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        let _ = self.lifecycle.get_mut().sender.try_send(Message::Shutdown);
    }
}

fn reserve_bounded(counter: &AtomicUsize, amount: usize, maximum: usize) -> bool {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(amount).filter(|next| *next <= maximum)
        })
        .is_ok()
}

fn increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(1))
    });
}

fn worker_loop(receiver: Receiver<Message>, sink: Arc<dyn LogSink>, counters: &Counters) {
    while let Ok(message) = receiver.recv() {
        match message {
            Message::Event(event, bytes) => {
                let delivered =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink.emit(&event)));
                if matches!(delivered, Ok(Ok(()))) {
                    increment(&counters.delivered);
                } else {
                    increment(&counters.failures);
                }
                counters.retained_events.fetch_sub(1, Ordering::Relaxed);
                counters.retained_bytes.fetch_sub(bytes, Ordering::Relaxed);
            }
            Message::Flush(completed) => {
                let _ = completed.send(());
            }
            Message::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Severity, Verbosity};
    use std::sync::{Condvar, Mutex as StdMutex};

    struct GatedSink {
        state: StdMutex<(bool, bool)>,
        changed: Condvar,
    }

    impl GatedSink {
        fn new() -> Self {
            Self {
                state: StdMutex::new((false, false)),
                changed: Condvar::new(),
            }
        }

        fn wait_until_entered(&self) {
            let mut state = self.state.lock().unwrap();
            while !state.0 {
                state = self.changed.wait(state).unwrap();
            }
        }

        fn release(&self) {
            let mut state = self.state.lock().unwrap();
            state.1 = true;
            self.changed.notify_all();
        }
    }

    impl LogSink for GatedSink {
        fn emit(&self, _event: &LogEvent) -> Result<(), LogError> {
            let mut state = self.state.lock().unwrap();
            state.0 = true;
            self.changed.notify_all();
            while !state.1 {
                state = self.changed.wait(state).unwrap();
            }
            Ok(())
        }
    }

    #[test]
    fn active_delivery_remains_inside_both_bounds() {
        let inner = Arc::new(GatedSink::new());
        let sink = AsyncSink::new(
            AsyncSinkConfig {
                max_events: 1,
                max_bytes: 4096,
            },
            inner.clone(),
        )
        .unwrap();
        let event = LogEvent::new(1, Severity::Info, Verbosity::V4, "test", "message");

        sink.emit(&event).unwrap();
        inner.wait_until_entered();
        assert_eq!(sink.emit(&event), Err(LogError::Capacity));
        assert_eq!(sink.stats().retained_events, 1);

        inner.release();
        sink.flush().unwrap();
        assert_eq!(
            sink.stats(),
            AsyncSinkStats {
                delivered: 1,
                rejected: 1,
                ..AsyncSinkStats::default()
            }
        );
        sink.shutdown().unwrap();
        assert_eq!(sink.emit(&event), Err(LogError::Io));
    }
}
