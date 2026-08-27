// =============================================================================
//        #######
//     ###       ###     F: timing.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/27 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/27 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Checked schedule parsing, advancement and retry timing.

use crate::{RetryPolicy, SchedulerError, TaskSchedule};
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) enum ParsedSchedule {
    Once,
    Interval(Duration),
    Cron(Box<Schedule>),
}

pub(super) fn schedule_next(schedule: &ParsedSchedule, now: SystemTime) -> Option<SystemTime> {
    match schedule {
        ParsedSchedule::Once => None,
        ParsedSchedule::Interval(every) => now.checked_add(*every),
        ParsedSchedule::Cron(schedule) => {
            let now: DateTime<Utc> = now.into();
            schedule.after(&now).next().map(Into::into)
        }
    }
}

pub(super) fn parse_schedule(
    schedule: TaskSchedule,
) -> Result<(ParsedSchedule, SystemTime), SchedulerError> {
    let now = SystemTime::now();
    match schedule {
        TaskSchedule::Once { run_at } => Ok((ParsedSchedule::Once, run_at)),
        TaskSchedule::Interval { every, start_at } => {
            if every.is_zero() {
                return Err(SchedulerError::InvalidSchedule("zero interval"));
            }
            let next_run = match start_at {
                Some(start_at) => start_at,
                None => now
                    .checked_add(every)
                    .ok_or(SchedulerError::InvalidSchedule(
                        "interval exceeds clock range",
                    ))?,
            };
            Ok((ParsedSchedule::Interval(every), next_run))
        }
        TaskSchedule::Cron { expression } => {
            let schedule = Schedule::from_str(&expression)
                .map_err(|error| SchedulerError::InvalidCron(error.to_string()))?;
            let now_utc: DateTime<Utc> = now.into();
            let next = schedule
                .after(&now_utc)
                .next()
                .ok_or(SchedulerError::InvalidSchedule(
                    "cron has no next occurrence",
                ))?;
            Ok((ParsedSchedule::Cron(Box::new(schedule)), next.into()))
        }
    }
}

pub(super) fn retry_delay(
    policy: &RetryPolicy,
    attempt: u32,
    jitter_state: &AtomicU64,
) -> Duration {
    let exponent = attempt.saturating_sub(1).min(31);
    let factor = u128::from(policy.multiplier).saturating_pow(exponent);
    let base_ms = policy.initial_backoff.as_millis().saturating_mul(factor);
    let capped_ms = base_ms.min(policy.max_backoff.as_millis());
    let jitter_max = policy.jitter.as_millis().min(u128::from(u64::MAX)) as u64;
    let jitter = if jitter_max == 0 {
        0
    } else {
        next_random(jitter_state) % jitter_max.saturating_add(1)
    };
    Duration::from_millis(
        capped_ms
            .min(u128::from(u64::MAX))
            .saturating_add(u128::from(jitter))
            .min(u128::from(u64::MAX)) as u64,
    )
}

fn next_random(state: &AtomicU64) -> u64 {
    let mut current = state.load(Ordering::Relaxed);
    loop {
        let mut next = current;
        next ^= next << 13;
        next ^= next >> 7;
        next ^= next << 17;
        match state.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(actual) => current = actual,
        }
    }
}

pub(super) fn now_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(1)
        .max(1)
}
