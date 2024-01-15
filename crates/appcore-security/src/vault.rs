// =============================================================================
//        #######
//     ###       ###     F: vault.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Vault contracts for local secret lock/unlock boundaries.

use crate::token::SecurityResult;

/// Vault state contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultState {
    /// Secret material is unavailable.
    Locked,
    /// Secret material may be accessed through the vault implementation.
    Unlocked,
}

/// Minimal vault contract. This is not a remote vault protocol.
pub trait Vault {
    /// Returns the current vault state.
    fn state(&self) -> VaultState;
    /// Removes access to secret material.
    fn lock(&mut self) -> SecurityResult<()>;
    /// Unlocks the vault using deployment-supplied key material.
    fn unlock(&mut self, key_material: &[u8]) -> SecurityResult<()>;
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;
