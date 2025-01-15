// =============================================================================
//        #######
//     ###       ###     F: worker.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

struct WorkerFuture<T> {
    state: Arc<Mutex<WorkerFutureState<T>>>,
}

struct WorkerFutureState<T> {
    result: Option<ControlPlaneResult<T>>,
    waker: Option<Waker>,
}

impl<T> Future for WorkerFuture<T> {
    type Output = ControlPlaneResult<T>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match state.result.take() {
            Some(result) => Poll::Ready(result),
            None => {
                state.waker = Some(context.waker().clone());
                Poll::Pending
            }
        }
    }
}

type WorkerJob = Box<dyn FnOnce() + Send + 'static>;

#[derive(Clone)]
pub(crate) struct ControlPlaneWorker {
    inner: Arc<WorkerInner>,
}

struct WorkerInner {
    sender: Mutex<Option<mpsc::SyncSender<WorkerJob>>>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
}

impl std::fmt::Debug for ControlPlaneWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ControlPlaneWorker")
            .field("max_pending_items", &MAX_CONTROL_PLANE_WORK_ITEMS)
            .finish()
    }
}

impl ControlPlaneWorker {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel::<WorkerJob>(MAX_CONTROL_PLANE_WORK_ITEMS);
        let worker_thread = thread::Builder::new()
            .name("appcore-control-plane-worker".to_string())
            .spawn(move || {
                while let Ok(operation) = receiver.recv() {
                    operation();
                }
            })
            .ok();
        Self {
            inner: Arc::new(WorkerInner {
                sender: Mutex::new(Some(sender)),
                thread: Mutex::new(worker_thread),
            }),
        }
    }

    pub(crate) fn enqueue<T, F>(&self, operation: F) -> ControlPlaneFuture<'static, T>
    where
        T: Send + 'static,
        F: FnOnce() -> ControlPlaneResult<T> + Send + 'static,
    {
        let state = Arc::new(Mutex::new(WorkerFutureState {
            result: None,
            waker: None,
        }));
        let worker_state = Arc::clone(&state);
        let job: WorkerJob = Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
                .unwrap_or_else(|_| {
                    Err(ControlPlaneError::Transport(
                        "control-plane transport worker panicked".to_string(),
                    ))
                });
            let waker = {
                let mut state = worker_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.result = Some(result);
                state.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        });
        let sender = self
            .inner
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let error = match sender.as_ref().map(|sender| sender.try_send(job)) {
            None => Some("control-plane worker is unavailable"),
            Some(Ok(())) => None,
            Some(Err(mpsc::TrySendError::Full(_))) => Some("control-plane worker queue is full"),
            Some(Err(mpsc::TrySendError::Disconnected(_))) => {
                Some("control-plane worker is unavailable")
            }
        };
        drop(sender);
        if let Some(message) = error {
            state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .result = Some(Err(ControlPlaneError::Transport(message.to_string())));
        }
        Box::pin(WorkerFuture { state })
    }
}

impl Drop for WorkerInner {
    fn drop(&mut self) {
        self.sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(worker_thread) = self
            .thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            if worker_thread.thread().id() != thread::current().id() {
                let _ = worker_thread.join();
            }
        }
    }
}
