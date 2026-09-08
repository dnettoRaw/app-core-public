// =============================================================================
//        #######
//     ###       ###     F: artifact_tier.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.3
// =============================================================================

//! Composes bounded artifact tiers and promotion leases.

use crate::artifact_store::check_cancel_size;
use crate::{
    AiError, AiResult, ArtifactDigest, ArtifactIdentity, ArtifactLease, ArtifactStore,
    CancellationToken,
};
use std::collections::BTreeSet;
use std::fmt::{Debug, Formatter};
use std::sync::{Arc, Mutex};

/// Ordered store composition that promotes verified bytes toward faster tiers.
pub struct TieredArtifactStore {
    tiers: Vec<Arc<dyn ArtifactStore>>,
    max_prefetch_bytes: u64,
    promotions: Mutex<BTreeSet<ArtifactDigest>>,
}

impl Debug for TieredArtifactStore {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TieredArtifactStore")
            .field("tiers", &self.tiers.len())
            .field("max_prefetch_bytes", &self.max_prefetch_bytes)
            .finish_non_exhaustive()
    }
}

impl TieredArtifactStore {
    /// Creates a bounded fastest-to-slowest tier chain.
    pub fn new(tiers: Vec<Arc<dyn ArtifactStore>>, max_prefetch_bytes: u64) -> AiResult<Self> {
        if tiers.is_empty() || tiers.len() > 16 || max_prefetch_bytes == 0 {
            return Err(AiError::InvalidInput("tiered artifact store"));
        }
        Ok(Self {
            tiers,
            max_prefetch_bytes,
            promotions: Mutex::new(BTreeSet::new()),
        })
    }

    /// Loads and promotes into the established owned-byte result.
    pub fn load_and_promote(
        &self,
        identity: &ArtifactIdentity,
        max_bytes: u64,
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<u8>> {
        self.load_and_promote_lease(identity, max_bytes, cancellation)
            .map(ArtifactLease::into_vec)
    }

    /// Loads from the first available tier and returns a no-copy lease when supported.
    pub fn load_and_promote_lease(
        &self,
        identity: &ArtifactIdentity,
        max_bytes: u64,
        cancellation: &CancellationToken,
    ) -> AiResult<ArtifactLease> {
        check_cancel_size(identity, max_bytes, cancellation)?;
        for (index, tier) in self.tiers.iter().enumerate() {
            if !tier.contains(identity)? {
                continue;
            }
            let bytes = tier.load_lease(identity, max_bytes, cancellation)?;
            if index > 0 && identity.size_bytes <= self.max_prefetch_bytes {
                self.promote(identity, &bytes, index, cancellation)?;
            }
            return Ok(bytes);
        }
        Err(AiError::NotFound("artifact"))
    }

    fn promote(
        &self,
        identity: &ArtifactIdentity,
        bytes: &[u8],
        source_index: usize,
        cancellation: &CancellationToken,
    ) -> AiResult<()> {
        let mut promotions = self.promotions.lock().map_err(|_| AiError::InternalState)?;
        if !promotions.insert(identity.digest) {
            return Ok(());
        }
        drop(promotions);
        let result = self.tiers[..source_index]
            .iter()
            .rev()
            .try_for_each(|tier| tier.store(identity, bytes, cancellation));
        self.promotions
            .lock()
            .map_err(|_| AiError::InternalState)?
            .remove(&identity.digest);
        result
    }
}
