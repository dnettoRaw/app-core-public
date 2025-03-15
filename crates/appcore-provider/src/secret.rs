// =============================================================================
//        #######
//     ###       ###     F: secret.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{ProviderError, ProviderResult};
use appcore_contracts::SecretRef;
use std::fmt::{Debug, Formatter};
use zeroize::Zeroizing;

/// Secret material resolved at deployment time.
pub struct ResolvedSecret(Zeroizing<String>);

impl ResolvedSecret {
    /// Wraps secret material so it is redacted from debug output and zeroized on drop.
    pub fn new(value: impl Into<String>) -> ProviderResult<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ProviderError::SecretUnavailable(
                "resolved value is empty".to_string(),
            ));
        }
        Ok(Self(Zeroizing::new(value)))
    }

    /// Borrows the secret only for immediate provider construction.
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }

    /// Transfers ownership without creating an intermediate plain `String`.
    pub fn into_zeroizing(self) -> Zeroizing<String> {
        self.0
    }
}

impl Debug for ResolvedSecret {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ResolvedSecret(REDACTED)")
    }
}

/// Provider contract for resolving deployment secrets without embedding values in manifests.
pub trait SecretProvider: Send + Sync {
    /// Resolves one external secret reference.
    fn resolve(&self, reference: &SecretRef) -> ProviderResult<ResolvedSecret>;
}
