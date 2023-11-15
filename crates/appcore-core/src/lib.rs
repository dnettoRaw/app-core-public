// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Public contracts for the AppCore runtime foundation.
//! This crate defines generic types and traits only.

#![deny(missing_docs)]

/// Versioned, implementation-independent manifest contracts.
pub use appcore_contracts as contracts;

pub mod audit;
pub mod builder;
pub mod bus;
pub mod clock;
pub mod command;
pub mod context;
pub mod controller;
pub mod decision;
pub mod envelope;
pub mod error;
pub mod event;
pub mod event_bus;
pub mod handler;
pub mod idempotency;
pub mod identity;
pub mod ids;
pub mod lifecycle;
pub mod manifest;
pub mod operational;
mod operational_journal;
pub mod plugin;
pub mod redaction;
pub mod registry;
pub mod runtime;
pub mod state;
pub mod trace;

pub use audit::{AuditCategory, AuditEntry, AuditLog, AuditOutcome, AuditRecord};
pub use builder::RuntimeBuilder;
pub use bus::CommandBus;
pub use clock::{Clock, SystemClock};
pub use command::{CommandRegistry, RuntimeCommand};
pub use context::RuntimeContext;
pub use controller::RuntimeController;
pub use decision::{DecisionEngine, DecisionNode, DecisionOutcome, DecisionRegistry};
pub use envelope::{CommandEnvelope, EventEnvelope};
pub use error::{RuntimeError, RuntimeResult};
pub use event::{EventRegistry, RuntimeEvent};
pub use event_bus::EventBus;
pub use handler::{CommandHandler, CommandResult};
pub use idempotency::{
    FileIdempotencyStore, IdempotencyStore, InMemoryIdempotencyStore, IDEMPOTENCY_FORMAT_V1,
};
pub use identity::{
    CompatibilityStatus, CoreCompatibilityPolicy, CoreCompatibilityStatus, CoreIdentity, CoreKind,
    RuntimeIdentity,
};
pub use ids::{
    validate_distributed_identifier, validate_identifier, AppFamily, AppId, CapabilityName,
    ClusterId, CommandName, CoreId, EventName, InstanceId, NodeId, ProtocolVersion, QueryName,
    RuntimeContractVersion, StateName, SyncGroup, TenantId,
};
pub use lifecycle::{RuntimeLifecycle, RuntimeLifecycleEvent, RuntimeLifecycleState};
pub use manifest::{
    CapabilityDescriptor, CapabilityMode, CapabilityRequirements, CapabilityVisibility,
    DistributedCoreManifest, PeerEndpoint,
};
pub use operational::RuntimeOperationalMode;
pub use operational_journal::{
    FileOperationalJournal, OperationalJournalRecord, OPERATIONAL_JOURNAL_FORMAT_V1,
};
pub use plugin::AppPlugin;
pub use redaction::{redact_text, redact_text_with_limit, MAX_OPERATIONAL_TEXT_BYTES};
pub use runtime::RuntimeInstance;
pub use state::{RuntimeState, StateMachine, StateRegistry, StateTransition};
pub use trace::TraceContext;
