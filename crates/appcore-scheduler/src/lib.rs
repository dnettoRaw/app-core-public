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

mod executor;
mod placement;
mod runtime;
mod task;

pub use placement::{
    PlacementCandidate, PlacementDecision, PlacementEngine, PlacementEvaluation,
    PlacementRejection, PlacementRequest, ResourceRequest,
};
pub use runtime::{Scheduler, TaskHandle};
pub use task::{
    RetryPolicy, ScheduledTask, SchedulerConfig, SchedulerError, SchedulerSnapshot, TaskCallback,
    TaskContext, TaskResult, TaskSchedule, TaskSnapshot,
};

use appcore_core::{redact_text, TraceContext};
use chrono::{DateTime, Utc};
use cron::Schedule;
use parking_lot::{Condvar, Mutex};
use std::collections::HashMap;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_TASK_ID_BYTES: usize = 128;

#[cfg(test)]
mod tests;
