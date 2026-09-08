// =============================================================================
//        #######
//     ###       ###     F: observation_flush.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Bounds observation-drain queue admission and acknowledgement.

use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) fn deadline(timeout: Duration) -> std::io::Result<Instant> {
    if timeout.is_zero() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "observation flush timeout must be positive",
        ));
    }
    Instant::now().checked_add(timeout).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "observation flush timeout is invalid",
        )
    })
}

pub(crate) fn enqueue<T>(
    sender: &SyncSender<T>,
    mut command: T,
    deadline: Instant,
) -> std::io::Result<()> {
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err(timeout_error());
        };
        if remaining.is_zero() {
            return Err(timeout_error());
        }
        match sender.try_send(command) {
            Ok(()) => return Ok(()),
            Err(TrySendError::Disconnected(_)) => return Err(stopped_error()),
            Err(TrySendError::Full(pending)) => {
                command = pending;
                thread::park_timeout(remaining.min(Duration::from_millis(1)));
            }
        }
    }
}

pub(crate) fn wait(receiver: Receiver<()>, deadline: Instant) -> std::io::Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    receiver
        .recv_timeout(remaining)
        .map_err(|error| match error {
            RecvTimeoutError::Timeout => timeout_error(),
            RecvTimeoutError::Disconnected => stopped_error(),
        })
}

fn timeout_error() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "observation drain flush timed out",
    )
}

fn stopped_error() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::BrokenPipe, "observation drain stopped")
}
