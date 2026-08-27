// =============================================================================
//        #######
//     ###       ###     F: runtime_durable.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/27 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/27 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Claim, renewal, recovery and completion for opt-in durable tasks.

use crate::durable::{ms_to_system_time, system_time_to_ms};
use crate::runtime::{SchedulerInner, TaskEntry};
use crate::timing::{retry_delay, schedule_next};
use crate::{
    redact_text, SchedulerStateClaimV1, SchedulerStateCompletionV1, SchedulerStateError,
    SchedulerStateRecordV1, TaskResult,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

const PROVIDER_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(100);
const OWNER_RECONCILE_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

pub(super) fn admit_durable_task(inner: &SchedulerInner, task_id: &str, now: SystemTime) -> bool {
    let existing_claim = {
        let state = inner.state.lock();
        let Some(task) = state.tasks.get(task_id) else {
            return false;
        };
        task.durable
            .as_ref()
            .and_then(|durable| durable.claim.clone())
    };
    let now_ms = match system_time_to_ms(now) {
        Ok(now_ms) => now_ms,
        Err(_) => {
            record_state_error(inner, task_id, "scheduler clock is unavailable");
            return false;
        }
    };
    if let Some(claim) = existing_claim {
        if claim.lease_until_ms >= now_ms {
            let mut state = inner.state.lock();
            if let Some(task) = state.tasks.get_mut(task_id) {
                task.dispatched = true;
                return true;
            }
        }
        clear_local_claim(inner, task_id);
        return false;
    }
    let Some(runtime) = inner.durable.as_ref() else {
        record_state_error(inner, task_id, "scheduler state provider is unavailable");
        return false;
    };
    let claim = match runtime.try_claim(task_id, now_ms) {
        Ok(Some(claim)) => claim,
        Ok(None) => {
            reconcile_record(inner, task_id);
            return false;
        }
        Err(_) => {
            record_state_error(inner, task_id, "scheduler state claim failed");
            return false;
        }
    };
    let skip_misfire = {
        let mut state = inner.state.lock();
        let Some(task) = state.tasks.get_mut(task_id) else {
            return false;
        };
        if task.dispatched || task.cancelled.load(Ordering::Acquire) {
            return false;
        }
        let Some(durable) = task.durable.as_mut() else {
            return false;
        };
        durable.claim = Some(claim.clone());
        durable.lease_valid = Some(Arc::new(AtomicBool::new(true)));
        task.attempts = claim.attempt;
        task.dispatched = true;
        durable.misfire_pending
    };
    if skip_misfire {
        settle_misfire(inner, task_id, claim, now);
        false
    } else {
        true
    }
}

pub(super) fn complete_durable_task(inner: &SchedulerInner, task_id: &str, result: TaskResult) {
    let now = SystemTime::now();
    let now_ms = match system_time_to_ms(now) {
        Ok(value) => value,
        Err(_) => {
            record_state_error(inner, task_id, "scheduler clock is unavailable");
            return;
        }
    };
    let completion = {
        let mut state = inner.state.lock();
        let Some(task) = state.tasks.get_mut(task_id) else {
            return;
        };
        task.running = false;
        if inner.shutdown.load(Ordering::Acquire) || task.cancelled.load(Ordering::Acquire) {
            invalidate_lease(task);
            state.tasks.remove(task_id);
            return;
        }
        let Some(claim) = task
            .durable
            .as_ref()
            .and_then(|durable| durable.claim.clone())
        else {
            task.dispatched = false;
            task.last_error = Some("durable task lost its claim".to_string());
            increment_state_errors(inner);
            return;
        };
        let (mut settled, next_run) = completion_schedule(task, result, now, inner);
        let next_run_ms = match next_run {
            Some(time) => match system_time_to_ms(time) {
                Ok(value) => Some(value),
                Err(_) => {
                    task.last_error = Some("durable next run exceeds clock range".to_string());
                    settled = true;
                    None
                }
            },
            None => None,
        };
        let completion = SchedulerStateCompletionV1 {
            claim,
            completed_at_ms: now_ms,
            next_run_ms,
            settled,
        };
        if let Some(durable) = task.durable.as_mut() {
            durable.pending_completion = Some(completion.clone());
            durable.provider_retry_at = now;
        }
        completion
    };
    commit_completion(inner, task_id, &completion);
}

fn completion_schedule(
    task: &mut TaskEntry,
    result: TaskResult,
    now: SystemTime,
    inner: &SchedulerInner,
) -> (bool, Option<SystemTime>) {
    match result {
        Ok(()) => {
            task.last_error = None;
            (true, schedule_next(&task.schedule, now))
        }
        Err(error) if task.attempts < task.retry.max_attempts => {
            task.last_error = Some(redact_text(&error));
            match now.checked_add(retry_delay(&task.retry, task.attempts, &inner.jitter_state)) {
                Some(next_run) => (false, Some(next_run)),
                None => {
                    task.last_error = Some("retry schedule exceeds clock range".to_string());
                    (true, None)
                }
            }
        }
        Err(error) => {
            task.last_error = Some(redact_text(&error));
            (true, schedule_next(&task.schedule, now))
        }
    }
}

fn settle_misfire(
    inner: &SchedulerInner,
    task_id: &str,
    claim: SchedulerStateClaimV1,
    now: SystemTime,
) {
    let next_run = {
        let state = inner.state.lock();
        state
            .tasks
            .get(task_id)
            .and_then(|task| schedule_next(&task.schedule, now))
    };
    let next_run_ms = next_run.and_then(|next| match system_time_to_ms(next) {
        Ok(value) => Some(value),
        Err(_) => {
            record_state_error(inner, task_id, "durable next run exceeds clock range");
            None
        }
    });
    let Ok(completed_at_ms) = system_time_to_ms(now) else {
        record_state_error(inner, task_id, "scheduler clock is unavailable");
        return;
    };
    let completion = SchedulerStateCompletionV1 {
        claim,
        completed_at_ms,
        next_run_ms,
        settled: true,
    };
    if let Some(task) = inner.state.lock().tasks.get_mut(task_id) {
        if let Some(durable) = task.durable.as_mut() {
            durable.pending_completion = Some(completion.clone());
            durable.provider_retry_at = now;
        }
    }
    commit_completion(inner, task_id, &completion);
}

pub(super) fn reconcile_pending_completions(inner: &SchedulerInner) {
    let now = SystemTime::now();
    let pending = inner
        .state
        .lock()
        .tasks
        .iter()
        .filter_map(|(task_id, task)| {
            task.durable
                .as_ref()
                .and_then(|durable| {
                    (durable.provider_retry_at <= now)
                        .then(|| durable.pending_completion.clone())
                        .flatten()
                })
                .map(|completion| (task_id.clone(), completion))
        })
        .collect::<Vec<_>>();
    for (task_id, completion) in pending {
        commit_completion(inner, &task_id, &completion);
    }
}

fn commit_completion(
    inner: &SchedulerInner,
    task_id: &str,
    completion: &SchedulerStateCompletionV1,
) {
    let Some(runtime) = inner.durable.as_ref() else {
        record_state_error(inner, task_id, "scheduler state provider is unavailable");
        return;
    };
    match runtime.complete(completion) {
        Ok(record) => apply_record(inner, task_id, record),
        Err(SchedulerStateError::Fenced) => {
            record_state_error(inner, task_id, "durable task completion was fenced");
            clear_local_claim(inner, task_id);
            reconcile_record(inner, task_id);
        }
        Err(_) => record_state_error(inner, task_id, "scheduler state completion failed"),
    }
}

fn apply_record(inner: &SchedulerInner, task_id: &str, record: SchedulerStateRecordV1) {
    let mut state = inner.state.lock();
    if record.completed {
        state.tasks.remove(task_id);
        return;
    }
    let now_ms = system_time_to_ms(SystemTime::now()).unwrap_or(record.next_run_ms);
    let effective_next_run_ms = record.claim.as_ref().map_or(record.next_run_ms, |claim| {
        if record.next_run_ms > now_ms {
            record.next_run_ms
        } else if claim.lease_until_ms > now_ms {
            claim
                .lease_until_ms
                .min(now_ms.saturating_add(OWNER_RECONCILE_DELAY.as_millis() as u64))
        } else {
            now_ms.saturating_add(PROVIDER_RETRY_DELAY.as_millis() as u64)
        }
    });
    let Ok(next_run) = ms_to_system_time(effective_next_run_ms) else {
        drop(state);
        record_state_error(inner, task_id, "scheduler state time is invalid");
        return;
    };
    if let Some(task) = state.tasks.get_mut(task_id) {
        task.next_run = next_run;
        task.attempts = record.attempts;
        task.dispatched = false;
        task.running = false;
        if let Some(durable) = task.durable.as_mut() {
            if let Some(valid) = durable.lease_valid.take() {
                valid.store(false, Ordering::Release);
            }
            durable.claim = None;
            durable.pending_completion = None;
            durable.misfire_pending &= record.next_run_ms <= now_ms;
            durable.provider_retry_at = SystemTime::now();
        }
    }
}

fn reconcile_record(inner: &SchedulerInner, task_id: &str) {
    let Some(runtime) = inner.durable.as_ref() else {
        return;
    };
    match runtime.record(task_id) {
        Ok(Some(record)) => apply_record(inner, task_id, record),
        Ok(None) => record_state_error(inner, task_id, "durable task state is missing"),
        Err(_) => record_state_error(inner, task_id, "scheduler state read failed"),
    }
}

pub(super) fn renew_claims(inner: &SchedulerInner) {
    let Some(runtime) = inner.durable.as_ref() else {
        return;
    };
    let Ok(now_ms) = system_time_to_ms(SystemTime::now()) else {
        return;
    };
    let renew_at_window = runtime.claim_ttl_ms / 2;
    let claims = inner
        .state
        .lock()
        .tasks
        .iter()
        .filter_map(|(task_id, task)| {
            let durable = task.durable.as_ref()?;
            let claim = durable.claim.as_ref()?;
            if durable.pending_completion.is_none()
                && now_ms >= claim.lease_until_ms.saturating_sub(renew_at_window)
            {
                Some((task_id.clone(), claim.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for (task_id, claim) in claims {
        match runtime.renew(&claim, now_ms) {
            Ok(lease_until_ms) => {
                update_lease(inner, &task_id, claim.fencing_epoch, lease_until_ms)
            }
            Err(_) => {
                record_state_error(inner, &task_id, "scheduler state renewal failed");
                if now_ms >= claim.lease_until_ms {
                    invalidate_task_lease(inner, &task_id);
                }
            }
        }
    }
}

fn update_lease(inner: &SchedulerInner, task_id: &str, epoch: u64, lease_until_ms: u64) {
    let mut state = inner.state.lock();
    let Some(durable) = state
        .tasks
        .get_mut(task_id)
        .and_then(|task| task.durable.as_mut())
    else {
        return;
    };
    if let Some(claim) = durable
        .claim
        .as_mut()
        .filter(|claim| claim.fencing_epoch == epoch)
    {
        claim.lease_until_ms = lease_until_ms;
    }
}

fn clear_local_claim(inner: &SchedulerInner, task_id: &str) {
    let mut state = inner.state.lock();
    let Some(task) = state.tasks.get_mut(task_id) else {
        return;
    };
    invalidate_lease(task);
    task.dispatched = false;
    task.running = false;
    if let Some(durable) = task.durable.as_mut() {
        durable.claim = None;
        durable.pending_completion = None;
    }
}

fn invalidate_task_lease(inner: &SchedulerInner, task_id: &str) {
    if let Some(task) = inner.state.lock().tasks.get_mut(task_id) {
        invalidate_lease(task);
    }
}

fn invalidate_lease(task: &mut TaskEntry) {
    if let Some(valid) = task
        .durable
        .as_ref()
        .and_then(|durable| durable.lease_valid.as_ref())
    {
        valid.store(false, Ordering::Release);
    }
}

fn record_state_error(inner: &SchedulerInner, task_id: &str, message: &'static str) {
    increment_state_errors(inner);
    if let Some(task) = inner.state.lock().tasks.get_mut(task_id) {
        task.last_error = Some(message.to_string());
        let retry_at = SystemTime::now()
            .checked_add(PROVIDER_RETRY_DELAY)
            .unwrap_or(SystemTime::now());
        if let Some(durable) = task.durable.as_mut() {
            if durable.pending_completion.is_some() {
                durable.provider_retry_at = retry_at;
            } else if !task.dispatched && task.next_run < retry_at {
                task.next_run = retry_at;
            }
        }
    }
}

fn increment_state_errors(inner: &SchedulerInner) {
    let _ = inner
        .state_errors
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_add(1))
        });
}
