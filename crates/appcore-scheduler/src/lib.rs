// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 10:48:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded local task scheduling with cooperative cancellation and clean shutdown.

#![deny(missing_docs)]

mod durable;
mod executor;
mod placement;
mod runtime;
mod runtime_durable;
mod runtime_loop;
mod state;
mod state_file;
mod state_memory;
mod task;
mod timing;

pub use durable::DurableSchedulerConfigV1;
pub use placement::{
    PlacementCandidate, PlacementDecision, PlacementEngine, PlacementEvaluation,
    PlacementRejection, PlacementRequest, ResourceRequest,
};
pub use runtime::{Scheduler, TaskHandle};
pub use state::{
    DurableTaskMisfirePolicyV1, SchedulerStateClaimRequestV1, SchedulerStateClaimV1,
    SchedulerStateCompletionV1, SchedulerStateError, SchedulerStateProvider,
    SchedulerStateRecordV1, SchedulerStateRegistrationV1, SchedulerStateStatsV1,
    MAX_SCHEDULER_CLAIM_TTL_MS, MAX_SCHEDULER_CLOCK_SKEW_MS, MAX_SCHEDULER_OWNER_ID_BYTES,
    MAX_SCHEDULER_STATE_RECORDS,
};
pub use state_file::{FileSchedulerStateProvider, SCHEDULER_STATE_FORMAT_V1};
pub use state_memory::InMemorySchedulerStateProvider;
pub use task::{
    RetryPolicy, ScheduledTask, SchedulerConfig, SchedulerError, SchedulerSnapshot, TaskCallback,
    TaskContext, TaskResult, TaskSchedule, TaskSnapshot,
};

use appcore_core::{redact_text, TraceContext};
use cron::Schedule;
use parking_lot::{Condvar, Mutex};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

const MAX_TASK_ID_BYTES: usize = 128;

#[cfg(test)]
mod runtime_durable_tests;
#[cfg(test)]
mod state_file_tests;
#[cfg(test)]
mod tests;
