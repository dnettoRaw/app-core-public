// =============================================================================
//        #######
//     ###       ###     F: executor.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0
// =============================================================================

//! Fixed bounded callback executor owned by the local scheduler.

use parking_lot::Mutex;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

pub(super) type ExecutorJob = Box<dyn FnOnce() + Send + 'static>;

pub(super) enum SubmitError {
    Full,
    Closed,
}

pub(super) struct FixedExecutor {
    sender: Mutex<Option<SyncSender<ExecutorJob>>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    queue_depth: Arc<AtomicUsize>,
    saturations: AtomicU64,
    worker_count: usize,
}

impl FixedExecutor {
    pub(super) fn new(worker_count: usize, queue_capacity: usize) -> Result<Self, ()> {
        let (sender, receiver) = mpsc::sync_channel(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let queue_depth = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::with_capacity(worker_count);
        for index in 0..worker_count {
            let worker_receiver = Arc::clone(&receiver);
            let worker_queue_depth = Arc::clone(&queue_depth);
            match thread::Builder::new()
                .name(format!("appcore-scheduler-worker-{index}"))
                .spawn(move || worker_loop(&worker_receiver, &worker_queue_depth))
            {
                Ok(worker) => workers.push(worker),
                Err(_) => {
                    drop(sender);
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(());
                }
            }
        }
        Ok(Self {
            sender: Mutex::new(Some(sender)),
            workers: Mutex::new(workers),
            queue_depth,
            saturations: AtomicU64::new(0),
            worker_count,
        })
    }

    pub(super) fn try_submit(&self, job: ExecutorJob) -> Result<(), SubmitError> {
        self.queue_depth.fetch_add(1, Ordering::AcqRel);
        let result = self
            .sender
            .lock()
            .as_ref()
            .ok_or(SubmitError::Closed)
            .and_then(|sender| match sender.try_send(job) {
                Ok(()) => Ok(()),
                Err(TrySendError::Full(_)) => Err(SubmitError::Full),
                Err(TrySendError::Disconnected(_)) => Err(SubmitError::Closed),
            });
        if result.is_err() {
            self.queue_depth.fetch_sub(1, Ordering::AcqRel);
            if matches!(result, Err(SubmitError::Full)) {
                self.record_saturation();
            }
        }
        result
    }

    pub(super) fn record_saturation(&self) {
        let _ = self
            .saturations
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_add(1))
            });
    }

    pub(super) fn queue_depth(&self) -> usize {
        self.queue_depth.load(Ordering::Acquire)
    }

    pub(super) fn saturation_count(&self) -> u64 {
        self.saturations.load(Ordering::Relaxed)
    }

    pub(super) fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub(super) fn shutdown(&self) {
        self.sender.lock().take();
        for worker in std::mem::take(&mut *self.workers.lock()) {
            let _ = worker.join();
        }
    }
}

impl Drop for FixedExecutor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_loop(receiver: &Mutex<Receiver<ExecutorJob>>, queue_depth: &AtomicUsize) {
    loop {
        let job = receiver.lock().recv();
        let Ok(job) = job else {
            break;
        };
        queue_depth.fetch_sub(1, Ordering::AcqRel);
        let _ = catch_unwind(AssertUnwindSafe(job));
    }
}
