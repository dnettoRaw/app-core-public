// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::ProviderRole;

/// Provider composition failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// A factory is already registered for the role and provider ID.
    #[error("provider factory already registered for {role:?}: {provider_id}")]
    DuplicateFactory {
        /// Duplicated role.
        role: ProviderRole,
        /// Duplicated provider identity.
        provider_id: String,
    },
    /// No factory is registered for a selected provider.
    #[error("provider is unavailable for {role:?}: {provider_id}")]
    Unavailable {
        /// Requested role.
        role: ProviderRole,
        /// Requested provider identity.
        provider_id: String,
    },
    /// Provider configuration is invalid.
    #[error("invalid provider configuration: {0}")]
    InvalidConfiguration(String),
    /// A required external secret cannot be resolved.
    #[error("provider secret is unavailable: {0}")]
    SecretUnavailable(String),
    /// Provider initialization failed.
    #[error("provider initialization failed: {0}")]
    Initialization(String),
}

/// Result returned by provider composition.
pub type ProviderResult<T> = Result<T, ProviderError>;
