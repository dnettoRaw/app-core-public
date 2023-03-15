// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Dependency-aware supervision for in-process managed services.
//!
//! This crate supervises managed services, workers, queues, and threads. It
//! deliberately does not supervise the operating-system process that hosts the
//! Runtime.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod adapters;
mod constants;
mod error;
mod events;
mod graph;
mod health;
mod policy;
mod restart_executor;
mod service;
mod snapshot;
mod supervisor;
mod watchdog;

pub use adapters::{CallbackManagedService, ManagedThreadService, PassiveManagedService};
pub use constants::DEFAULT_EVENT_CAPACITY;
pub use error::{SupervisorError, SupervisorResult};
pub use events::{SupervisorEvent, SupervisorEventKind};
pub use health::{
    DependencyRequirement, ServiceActivationState, ServiceHealth, ServiceRuntimeState,
};
pub use policy::{RestartMode, RestartPolicy};
pub use restart_executor::RestartState;
pub use service::{ManagedResource, ManagedService, ServiceDependency, ServiceDescriptor};
pub use snapshot::{
    RestartExecutorSnapshot, ServiceSnapshot, SupervisorDiagnosis, WatchdogSnapshot,
};
pub use supervisor::Supervisor;
pub use watchdog::{
    SupervisorWatchdog, WatchdogConfig, WatchdogState, DEFAULT_WATCHDOG_CHECK_INTERVAL_MS,
    DEFAULT_WATCHDOG_STALL_TIMEOUT_MS,
};
