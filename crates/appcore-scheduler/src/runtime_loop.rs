// =============================================================================
//        #######
//     ###       ###     F: runtime_loop.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/27 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/27 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Coordinator and callback completion loop for local and durable tasks.

use crate::executor::SubmitError;
use crate::runtime::SchedulerInner;
use crate::runtime_durable::{
    admit_durable_task, complete_durable_task, reconcile_pending_completions, renew_claims,
};
use crate::timing::{retry_delay, schedule_next};
use crate::{redact_text, TaskContext, TaskResult};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;

pub(super) fn coordinator_loop(inner: Arc<SchedulerInner>) {
    while !inner.shutdown.load(Ordering::Acquire) {
        reconcile_pending_completions(&inner);
        renew_claims(&inner);
        let available = inner
            .dispatch_limit
            .saturating_sub(inner.inflight.load(Ordering::Acquire));
        let due = collect_due(&inner, available);
        for task_id in due {
            dispatch_task(&inner, task_id);
        }

        let mut state = inner.state.lock();
        if !inner.shutdown.load(Ordering::Acquire) {
            inner
                .wakeup
                .wait_for(&mut state, inner.config.poll_interval);
        }
    }

    inner.executor.shutdown();
    inner.state.lock().tasks.clear();
}

fn collect_due(inner: &SchedulerInner, available: usize) -> Vec<String> {
    let now = SystemTime::now();
    let mut candidates = {
        let mut state = inner.state.lock();
        state
            .tasks
            .retain(|_, task| task.dispatched || !task.cancelled.load(Ordering::Acquire));
        state
            .tasks
            .iter()
            .filter(|(_, task)| !task.dispatched && task.next_run <= now)
            .map(|(id, task)| (id.clone(), task.priority, task.next_run, task.order))
            .collect::<Vec<_>>()
    };
    candidates.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.3.cmp(&right.3))
    });
    if candidates.len() > available {
        inner.executor.record_saturation();
    }
    let mut admitted = Vec::with_capacity(available);
    for (task_id, _, _, _) in candidates {
        if admitted.len() == available {
            break;
        }
        if admit_task(inner, &task_id, now) {
            admitted.push(task_id);
        }
    }
    admitted
}

fn admit_task(inner: &SchedulerInner, task_id: &str, now: SystemTime) -> bool {
    {
        let mut state = inner.state.lock();
        let Some(task) = state.tasks.get_mut(task_id) else {
            return false;
        };
        if task.dispatched || task.cancelled.load(Ordering::Acquire) {
            return false;
        }
        if task.durable.is_none() {
            task.dispatched = true;
            task.attempts = task.attempts.saturating_add(1);
            return true;
        }
    }
    admit_durable_task(inner, task_id, now)
}

fn dispatch_task(inner: &Arc<SchedulerInner>, task_id: String) {
    let (callback, context) = {
        let state = inner.state.lock();
        let Some(task) = state.tasks.get(&task_id) else {
            return;
        };
        let fencing_epoch = task
            .durable
            .as_ref()
            .and_then(|durable| durable.claim.as_ref())
            .map(|claim| claim.fencing_epoch);
        let lease_valid = task
            .durable
            .as_ref()
            .and_then(|durable| durable.lease_valid.clone());
        (
            Arc::clone(&task.callback),
            TaskContext::new(
                task_id.clone(),
                task.attempts,
                Arc::clone(&task.cancelled),
                Arc::clone(&inner.shutdown),
                task.trace.clone(),
                fencing_epoch,
                lease_valid,
            ),
        )
    };
    inner.inflight.fetch_add(1, Ordering::AcqRel);
    let worker_inner = Arc::clone(inner);
    let worker_task_id = task_id.clone();
    let job = Box::new(move || {
        mark_task_running(&worker_inner, &worker_task_id);
        worker_inner.active.fetch_add(1, Ordering::AcqRel);
        let result = catch_unwind(AssertUnwindSafe(|| callback(context)))
            .unwrap_or_else(|_| Err("task panicked".to_string()));
        worker_inner.active.fetch_sub(1, Ordering::AcqRel);
        complete_task(&worker_inner, &worker_task_id, result);
    });
    if let Err(error) = inner.executor.try_submit(job) {
        inner.inflight.fetch_sub(1, Ordering::AcqRel);
        defer_task(inner, &task_id, error);
    }
}

fn mark_task_running(inner: &SchedulerInner, task_id: &str) {
    if let Some(task) = inner.state.lock().tasks.get_mut(task_id) {
        task.running = true;
    }
}

fn defer_task(inner: &SchedulerInner, task_id: &str, error: SubmitError) {
    let mut state = inner.state.lock();
    let remove = inner.shutdown.load(Ordering::Acquire)
        || state
            .tasks
            .get(task_id)
            .is_some_and(|task| task.cancelled.load(Ordering::Acquire));
    if remove {
        state.tasks.remove(task_id);
    } else if let Some(task) = state.tasks.get_mut(task_id) {
        task.dispatched = false;
        if task.durable.is_none() {
            task.attempts = task.attempts.saturating_sub(1);
        }
        if matches!(error, SubmitError::Closed) {
            task.last_error = Some("scheduler executor unavailable".to_string());
        }
    }
    drop(state);
    inner.wakeup.notify_all();
}

fn complete_task(inner: &SchedulerInner, task_id: &str, result: TaskResult) {
    let durable = inner
        .state
        .lock()
        .tasks
        .get(task_id)
        .is_some_and(|task| task.durable.is_some());
    if durable {
        complete_durable_task(inner, task_id, result);
    } else {
        complete_ephemeral_task(inner, task_id, result);
    }
    inner.inflight.fetch_sub(1, Ordering::AcqRel);
    inner.wakeup.notify_all();
}

fn complete_ephemeral_task(inner: &SchedulerInner, task_id: &str, result: TaskResult) {
    let now = SystemTime::now();
    let mut state = inner.state.lock();
    let mut remove = false;
    if let Some(task) = state.tasks.get_mut(task_id) {
        task.dispatched = false;
        task.running = false;
        if inner.shutdown.load(Ordering::Acquire) || task.cancelled.load(Ordering::Acquire) {
            remove = true;
        } else if let Err(error) = result {
            task.last_error = Some(redact_text(&error));
            if task.attempts < task.retry.max_attempts {
                if let Some(next_run) =
                    now.checked_add(retry_delay(&task.retry, task.attempts, &inner.jitter_state))
                {
                    task.next_run = next_run;
                } else {
                    task.last_error = Some("retry schedule exceeds clock range".to_string());
                    remove = true;
                }
            } else {
                task.attempts = 0;
                if let Some(next_run) = schedule_next(&task.schedule, now) {
                    task.next_run = next_run;
                } else {
                    remove = true;
                }
            }
        } else {
            task.last_error = None;
            task.attempts = 0;
            if let Some(next_run) = schedule_next(&task.schedule, now) {
                task.next_run = next_run;
            } else {
                remove = true;
            }
        }
    }
    if remove {
        state.tasks.remove(task_id);
    }
    drop(state);
}
