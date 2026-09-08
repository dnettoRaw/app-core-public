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
use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;
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

    for task in inner.state.lock().tasks.values() {
        task.cancelled.store(true, Ordering::Release);
    }
    inner.executor.shutdown();
    inner.state.lock().tasks.clear();
}

#[derive(Eq, PartialEq)]
struct DueCandidate<'a> {
    task_id: &'a str,
    priority: u8,
    next_run: SystemTime,
    order: u64,
}

#[derive(Clone, Copy)]
struct DueOrderKey<'a> {
    task_id: &'a str,
    priority: u8,
    next_run: SystemTime,
    order: u64,
}

impl Ord for DueCandidate<'_> {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        due_order(self.order_key(), other.order_key())
    }
}

impl PartialOrd for DueCandidate<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl DueCandidate<'_> {
    fn order_key(&self) -> DueOrderKey<'_> {
        DueOrderKey {
            task_id: self.task_id,
            priority: self.priority,
            next_run: self.next_run,
            order: self.order,
        }
    }
}

struct DueSelection<'a> {
    candidates: Vec<DueCandidate<'a>>,
    saturated: bool,
    #[cfg(test)]
    peak_candidates: usize,
}

fn collect_due(inner: &SchedulerInner, available: usize) -> Vec<String> {
    let now = SystemTime::now();
    let (saturated, mut task_ids) = {
        let mut state = inner.state.lock();
        state
            .tasks
            .retain(|_, task| task.dispatched || !task.cancelled.load(Ordering::Acquire));
        let selection = select_due_candidates(
            state
                .tasks
                .iter()
                .filter(|(_, task)| !task.dispatched && task.next_run <= now)
                .map(|(id, task)| (id.as_str(), task.priority, task.next_run, task.order)),
            available,
        );
        let task_ids: Vec<String> = selection
            .candidates
            .into_iter()
            .map(|candidate| candidate.task_id.to_string())
            .collect();
        (selection.saturated, task_ids)
    };
    if saturated {
        inner.executor.record_saturation();
    }
    task_ids.retain(|task_id| admit_task(inner, task_id, now));
    task_ids
}

fn select_due_candidates<'a>(
    candidates: impl Iterator<Item = (&'a str, u8, SystemTime, u64)>,
    available: usize,
) -> DueSelection<'a> {
    let mut selected: BinaryHeap<DueCandidate<'a>> = BinaryHeap::with_capacity(available);
    let mut saturated = false;
    #[cfg(test)]
    let mut peak_candidates = 0usize;
    for (task_id, priority, next_run, order) in candidates {
        let candidate_key = DueOrderKey {
            task_id,
            priority,
            next_run,
            order,
        };
        let should_insert = if selected.len() < available {
            true
        } else {
            saturated = true;
            selected
                .peek()
                .is_some_and(|worst| due_order(candidate_key, worst.order_key()).is_lt())
        };
        if should_insert {
            if selected.len() == available {
                let _ = selected.pop();
            }
            selected.push(DueCandidate {
                task_id,
                priority,
                next_run,
                order,
            });
            #[cfg(test)]
            {
                peak_candidates = peak_candidates.max(selected.len());
            }
        }
    }
    let mut candidates = selected.into_vec();
    candidates.sort();
    DueSelection {
        candidates,
        saturated,
        #[cfg(test)]
        peak_candidates,
    }
}

fn due_order(left: DueOrderKey<'_>, right: DueOrderKey<'_>) -> CmpOrdering {
    right
        .priority
        .cmp(&left.priority)
        .then_with(|| left.next_run.cmp(&right.next_run))
        .then_with(|| left.order.cmp(&right.order))
        .then_with(|| left.task_id.cmp(right.task_id))
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

#[cfg(test)]
mod tests {
    use super::select_due_candidates;
    use crate::MAX_SCHEDULER_TASKS;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn bounded_selection_preserves_dispatch_order() {
        let base = UNIX_EPOCH + Duration::from_secs(10);
        let tasks = [
            ("low", 1, base, 0),
            ("late-high", 10, base + Duration::from_secs(1), 1),
            ("early-high-second", 10, base, 3),
            ("early-high-first", 10, base, 2),
            ("medium", 5, base, 4),
        ];

        let selection = select_due_candidates(tasks.into_iter(), 4);
        let selected = selection
            .candidates
            .iter()
            .map(|candidate| candidate.task_id)
            .collect::<Vec<_>>();

        assert_eq!(
            selected,
            [
                "early-high-first",
                "early-high-second",
                "late-high",
                "medium"
            ]
        );
        assert!(selection.saturated);
        assert_eq!(selection.peak_candidates, 4);
    }

    #[test]
    fn maximum_due_set_retains_only_dispatch_capacity() {
        let selection = select_due_candidates(
            (0..MAX_SCHEDULER_TASKS).map(|order| {
                (
                    "bounded-task",
                    (order % 16) as u8,
                    UNIX_EPOCH + Duration::from_secs((order % 32) as u64),
                    order as u64,
                )
            }),
            128,
        );

        assert_eq!(selection.candidates.len(), 128);
        assert_eq!(selection.peak_candidates, 128);
        assert!(selection.saturated);
    }

    #[test]
    fn zero_capacity_retains_no_due_candidates() {
        let selection = select_due_candidates(std::iter::once(("waiting", 1, UNIX_EPOCH, 0)), 0);

        assert!(selection.candidates.is_empty());
        assert_eq!(selection.peak_candidates, 0);
        assert!(selection.saturated);
    }
}
