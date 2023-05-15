// =============================================================================
//        #######
//     ###       ###     F: storage.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Required persistence durability.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageDurability {
    /// Data may be discarded when the process exits.
    Ephemeral,
    /// Data must survive process restarts on the same node.
    #[default]
    Local,
    /// Data must use a durable provider selected by deployment.
    Durable,
}

/// Application storage requirements without provider or path decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageRequirements {
    durability: StorageDurability,
    minimum_bytes: u64,
    shared: bool,
}

impl StorageRequirements {
    /// Creates provider-independent storage requirements.
    pub fn new(durability: StorageDurability, minimum_bytes: u64, shared: bool) -> Self {
        Self {
            durability,
            minimum_bytes,
            shared,
        }
    }

    /// Returns the required durability.
    pub fn durability(&self) -> StorageDurability {
        self.durability
    }

    /// Returns the minimum requested capacity.
    pub fn minimum_bytes(&self) -> u64 {
        self.minimum_bytes
    }

    /// Reports whether storage must be shared across nodes.
    pub fn is_shared(&self) -> bool {
        self.shared
    }
}
