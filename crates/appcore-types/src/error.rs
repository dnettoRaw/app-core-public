// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Runtime error contract used across core public traits.

use crate::identity::{CompatibilityStatus, CoreCompatibilityStatus};
use crate::ids::CommandName;

/// Standard runtime error categories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// A requested lifecycle transition is not allowed from the current state.
    InvalidStateTransition,
    /// The same state transition was registered more than once.
    DuplicateStateTransition,
    /// Authentication was absent or invalid.
    Unauthorized,
    /// Authentication succeeded but the operation is not permitted.
    Forbidden,
    /// Application identity fields are internally inconsistent.
    InvalidAppIdentity,
    /// Runtime contract versions are incompatible.
    IncompatibleRuntimeContract,
    /// The configured storage boundary is unavailable.
    StorageUnavailable,
    /// Secret storage is locked.
    VaultLocked,
    /// A synchronization operation was rejected.
    SyncRejected,
    /// A command was rejected before successful execution.
    CommandRejected,
    /// A command handler already exists for the supplied command name.
    HandlerAlreadyRegistered(CommandName),
    /// No command handler exists for the supplied command name.
    HandlerNotFound(CommandName),
    /// The same plugin instance was registered more than once.
    PluginAlreadyRegistered,
    /// A required Runtime manifest is absent.
    MissingManifest,
    /// A command envelope contains an empty command identity.
    EmptyCommandId,
    /// An event envelope contains an empty event identity.
    EmptyEventId,
    /// A validated identifier was rejected.
    InvalidIdentifier {
        /// Identifier type.
        kind: &'static str,
        /// Rejected non-sensitive value.
        value: String,
        /// Stable validation reason.
        reason: &'static str,
    },
    /// A request failed structural validation.
    InvalidRequest {
        /// Request category.
        kind: &'static str,
        /// Stable validation reason.
        reason: &'static str,
    },
    /// A shared Runtime resource could not be locked.
    LockPoisoned {
        /// Logical resource name.
        resource: &'static str,
    },
    /// A named registry item already exists.
    RegistryItemAlreadyRegistered {
        /// Registry item category.
        kind: &'static str,
        /// Repeated item name.
        name: String,
    },
    /// A named registry item was not found.
    RegistryItemNotFound {
        /// Registry item category.
        kind: &'static str,
        /// Requested item name.
        name: String,
    },
    /// Required Runtime configuration is absent.
    MissingConfiguration {
        /// Missing configuration field.
        name: &'static str,
    },
    /// Two Runtime identities are incompatible.
    IncompatibleIdentity(CompatibilityStatus),
    /// Two distributed Core identities are incompatible.
    IncompatibleCoreIdentity(CoreCompatibilityStatus),
    /// A required generic capability is absent.
    MissingCapabilityNamed {
        /// Missing capability name.
        capability: String,
    },
    /// A registry returned a controlled error.
    RegistryError(String),
    /// A registry contains a duplicate item category.
    DuplicateRegistryItem {
        /// Duplicate item category.
        kind: String,
    },
    /// An idempotency key failed validation.
    InvalidIdempotencyKey {
        /// Stable validation reason.
        reason: &'static str,
    },
    /// Durable idempotency storage failed.
    IdempotencyStoreIo {
        /// Storage operation being performed.
        operation: &'static str,
        /// Non-sensitive I/O detail.
        message: String,
    },
    /// An idempotency key was reused with a different request.
    IdempotencyConflict {
        /// Conflicting key.
        key: String,
    },
    /// An equivalent request is still being processed.
    IdempotencyPending {
        /// Pending key.
        key: String,
    },
    /// Durable operational audit or event journal failed.
    OperationalJournalIo {
        /// Stable journal operation.
        operation: &'static str,
        /// Non-sensitive failure detail.
        message: String,
    },
}

/// Result alias for runtime contract methods.
pub type RuntimeResult<T> = Result<T, RuntimeError>;
