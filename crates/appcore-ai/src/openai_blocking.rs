// =============================================================================
//        #######
//     ###       ###     F: openai_blocking.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/25 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/25 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use crate::{AiError, AiResult, CancellationToken, OpenAiTransportFuture};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Debug)]
pub(crate) struct BlockingGate {
    maximum: usize,
    active: AtomicUsize,
}

impl Default for BlockingGate {
    fn default() -> Self {
        Self {
            maximum: 4,
            active: AtomicUsize::new(0),
        }
    }
}

impl BlockingGate {
    pub(crate) fn new(maximum: usize) -> AiResult<Self> {
        if maximum == 0 || maximum > 64 {
            return Err(AiError::InvalidInput("OpenAI transport concurrency"));
        }
        Ok(Self {
            maximum,
            active: AtomicUsize::new(0),
        })
    }

    fn acquire(self: &Arc<Self>) -> Option<BlockingPermit> {
        let mut active = self.active.load(Ordering::Acquire);
        loop {
            if active >= self.maximum {
                return None;
            }
            match self.active.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(BlockingPermit {
                        gate: Arc::clone(self),
                    });
                }
                Err(actual) => active = actual,
            }
        }
    }
}

struct BlockingPermit {
    gate: Arc<BlockingGate>,
}

impl Drop for BlockingPermit {
    fn drop(&mut self) {
        self.gate.active.fetch_sub(1, Ordering::AcqRel);
    }
}

struct Completion<T> {
    result: Option<AiResult<T>>,
    waker: Option<Waker>,
}

struct BlockingFuture<T> {
    completion: Arc<Mutex<Completion<T>>>,
    cancellation: CancellationToken,
    transport_cancellation: appcore_transport::CancellationToken,
}

impl<T> Future for BlockingFuture<T> {
    type Output = AiResult<T>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.cancellation.is_cancelled() {
            self.transport_cancellation.cancel();
            return Poll::Ready(Err(AiError::Cancelled));
        }
        let mut completion = self
            .completion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(result) = completion.result.take() {
            Poll::Ready(result)
        } else {
            completion.waker = Some(context.waker().clone());
            Poll::Pending
        }
    }
}

impl<T> Drop for BlockingFuture<T> {
    fn drop(&mut self) {
        self.transport_cancellation.cancel();
    }
}

pub(crate) fn run<T, F>(
    gate: Arc<BlockingGate>,
    cancellation: CancellationToken,
    operation: F,
) -> OpenAiTransportFuture<'static>
where
    T: Into<crate::OpenAiTransportResponse> + Send + 'static,
    F: FnOnce(appcore_transport::CancellationToken) -> AiResult<T> + Send + 'static,
{
    let Some(permit) = gate.acquire() else {
        return Box::pin(async { Err(AiError::QueueFull) });
    };
    let completion = Arc::new(Mutex::new(Completion {
        result: None,
        waker: None,
    }));
    let transport_cancellation = appcore_transport::CancellationToken::new();
    let worker_completion = Arc::clone(&completion);
    let worker_cancellation = transport_cancellation.clone();
    let spawned = std::thread::Builder::new()
        .name("appcore-ai-http".to_string())
        .spawn(move || {
            let result = operation(worker_cancellation).map(Into::into);
            let mut completion = worker_completion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            completion.result = Some(result);
            if let Some(waker) = completion.waker.take() {
                waker.wake();
            }
            drop(permit);
        });
    if spawned.is_err() {
        let mut value = completion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        value.result = Some(Err(AiError::Capacity("OpenAI transport thread")));
    }
    Box::pin(BlockingFuture {
        completion,
        cancellation,
        transport_cancellation,
    })
}
