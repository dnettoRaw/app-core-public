// =============================================================================
//        #######
//     ###       ###     F: artifact_store.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiResult, ArtifactDigest, ArtifactIdentity, CancellationToken, LocalArtifactCache,
    PeerId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// Backend-neutral artifact-store class.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ArtifactStoreKind {
    /// In-process bounded cache.
    Memory,
    /// Local persistent cache.
    Local,
    /// Authenticated peer storage, not a mounted filesystem.
    Peer(PeerId),
}

/// Health and transfer hints kept separate from compute placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactStoreDescriptor {
    /// Store class and identity.
    pub kind: ArtifactStoreKind,
    /// Whether the store currently accepts work.
    pub healthy: bool,
    /// Whether an authenticated policy authorizes this store.
    pub trusted: bool,
    /// Recent transport latency.
    pub latency: Option<Duration>,
    /// Recent available bandwidth.
    pub bandwidth_bytes_per_second: Option<u64>,
    /// Reliability estimate in basis points.
    pub reliability_basis_points: u16,
}

/// Verified artifact storage boundary used by tiered cache and residency code.
pub trait ArtifactStore: Send + Sync {
    /// Returns safe placement metadata.
    fn descriptor(&self) -> ArtifactStoreDescriptor;

    /// Returns whether an identity may be loaded from this store.
    fn contains(&self, identity: &ArtifactIdentity) -> AiResult<bool>;

    /// Loads complete bytes with digest, size, cancellation and caller limit checks.
    fn load(
        &self,
        identity: &ArtifactIdentity,
        max_bytes: u64,
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<u8>>;

    /// Stores complete verified bytes without trusting an external filename.
    fn store(
        &self,
        identity: &ArtifactIdentity,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> AiResult<()>;

    /// Removes recoverable cached bytes when the implementation supports eviction.
    fn remove(&self, identity: &ArtifactIdentity) -> AiResult<bool>;

    /// Reports whether mandatory publisher provenance was verified for activation.
    fn provenance_verified(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        Ok(!identity.signature_required)
    }
}

impl ArtifactStore for LocalArtifactCache {
    fn descriptor(&self) -> ArtifactStoreDescriptor {
        ArtifactStoreDescriptor {
            kind: ArtifactStoreKind::Local,
            healthy: true,
            trusted: true,
            latency: None,
            bandwidth_bytes_per_second: None,
            reliability_basis_points: 10_000,
        }
    }

    fn contains(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        LocalArtifactCache::contains(self, identity)
    }

    fn load(
        &self,
        identity: &ArtifactIdentity,
        max_bytes: u64,
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<u8>> {
        check_cancel_size(identity, max_bytes, cancellation)?;
        let bytes = LocalArtifactCache::load(self, identity)?;
        check_cancel_size(identity, max_bytes, cancellation)?;
        Ok(bytes)
    }

    fn store(
        &self,
        identity: &ArtifactIdentity,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> AiResult<()> {
        check_cancel_size(identity, identity.size_bytes, cancellation)?;
        LocalArtifactCache::store(self, identity, bytes).map(|_| ())
    }

    fn remove(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        LocalArtifactCache::remove(self, identity)
    }
}

#[derive(Debug, Default)]
struct MemoryState {
    artifacts: BTreeMap<ArtifactDigest, Arc<[u8]>>,
    used_bytes: u64,
}

/// Digest-keyed memory artifact store with a fixed aggregate byte bound.
#[derive(Debug)]
pub struct MemoryArtifactStore {
    max_bytes: u64,
    state: RwLock<MemoryState>,
}

impl MemoryArtifactStore {
    /// Creates an empty memory cache.
    pub fn new(max_bytes: u64) -> AiResult<Self> {
        if max_bytes == 0 {
            return Err(AiError::InvalidInput("memory artifact store size"));
        }
        Ok(Self {
            max_bytes,
            state: RwLock::new(MemoryState::default()),
        })
    }

    /// Returns currently occupied bytes.
    pub fn used_bytes(&self) -> AiResult<u64> {
        Ok(self
            .state
            .read()
            .map_err(|_| AiError::InternalState)?
            .used_bytes)
    }
}

impl ArtifactStore for MemoryArtifactStore {
    fn descriptor(&self) -> ArtifactStoreDescriptor {
        ArtifactStoreDescriptor {
            kind: ArtifactStoreKind::Memory,
            healthy: true,
            trusted: true,
            latency: None,
            bandwidth_bytes_per_second: None,
            reliability_basis_points: 10_000,
        }
    }

    fn contains(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        Ok(self
            .state
            .read()
            .map_err(|_| AiError::InternalState)?
            .artifacts
            .contains_key(&identity.digest))
    }

    fn load(
        &self,
        identity: &ArtifactIdentity,
        max_bytes: u64,
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<u8>> {
        check_cancel_size(identity, max_bytes, cancellation)?;
        let bytes = {
            self.state
                .read()
                .map_err(|_| AiError::InternalState)?
                .artifacts
                .get(&identity.digest)
                .cloned()
                .ok_or(AiError::NotFound("artifact"))?
        };
        verify(identity, &bytes)?;
        Ok(bytes.as_ref().to_vec())
    }

    fn store(
        &self,
        identity: &ArtifactIdentity,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> AiResult<()> {
        check_cancel_size(identity, self.max_bytes, cancellation)?;
        verify(identity, bytes)?;
        let mut state = self.state.write().map_err(|_| AiError::InternalState)?;
        if let Some(existing) = state.artifacts.get(&identity.digest) {
            return if existing.as_ref() == bytes {
                Ok(())
            } else {
                Err(AiError::Integrity("artifact memory collision"))
            };
        }
        let size = u64::try_from(bytes.len()).map_err(|_| AiError::Capacity("artifact"))?;
        if state.used_bytes.saturating_add(size) > self.max_bytes {
            return Err(AiError::Capacity("memory artifact store"));
        }
        state.artifacts.insert(identity.digest, Arc::from(bytes));
        state.used_bytes = state.used_bytes.saturating_add(size);
        Ok(())
    }

    fn remove(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        let mut state = self.state.write().map_err(|_| AiError::InternalState)?;
        let Some(bytes) = state.artifacts.remove(&identity.digest) else {
            return Ok(false);
        };
        state.used_bytes = state
            .used_bytes
            .saturating_sub(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        Ok(true)
    }
}

/// Authenticated transport boundary for a peer artifact donation service.
pub trait PeerArtifactTransport: Send + Sync {
    /// Reports whether the peer currently advertises the exact digest.
    fn contains(&self, peer: &PeerId, identity: &ArtifactIdentity) -> AiResult<bool>;

    /// Fetches exact artifact bytes without exposing a remote filesystem path.
    fn fetch(
        &self,
        peer: &PeerId,
        identity: &ArtifactIdentity,
        max_bytes: u64,
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<u8>>;

    /// Stores exact verified bytes when peer contribution policy permits it.
    fn put(
        &self,
        peer: &PeerId,
        identity: &ArtifactIdentity,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> AiResult<()>;

    /// Removes a donated replica when peer policy permits it.
    fn remove(&self, peer: &PeerId, identity: &ArtifactIdentity) -> AiResult<bool>;
}

/// Peer artifact store that fails closed unless authenticated trust is explicit.
pub struct PeerArtifactStore {
    descriptor: ArtifactStoreDescriptor,
    transport: Arc<dyn PeerArtifactTransport>,
    fetches: AtomicU64,
    transferred_bytes: AtomicU64,
    failures: AtomicU64,
}

impl Debug for PeerArtifactStore {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerArtifactStore")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl PeerArtifactStore {
    /// Creates a peer store only from explicit peer placement metadata.
    pub fn new(
        descriptor: ArtifactStoreDescriptor,
        transport: Arc<dyn PeerArtifactTransport>,
    ) -> AiResult<Self> {
        if !matches!(descriptor.kind, ArtifactStoreKind::Peer(_))
            || descriptor.reliability_basis_points > 10_000
        {
            return Err(AiError::InvalidInput("peer artifact store descriptor"));
        }
        Ok(Self {
            descriptor,
            transport,
            fetches: AtomicU64::new(0),
            transferred_bytes: AtomicU64::new(0),
            failures: AtomicU64::new(0),
        })
    }

    /// Returns bounded transfer counters without peer-identity labels.
    #[must_use]
    pub fn metrics(&self) -> crate::PeerArtifactMetrics {
        crate::PeerArtifactMetrics {
            fetches: self.fetches.load(Ordering::Relaxed),
            transferred_bytes: self.transferred_bytes.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
        }
    }

    fn trusted_peer(&self) -> AiResult<&PeerId> {
        if !self.descriptor.healthy || !self.descriptor.trusted {
            return Err(AiError::Unauthorized);
        }
        match &self.descriptor.kind {
            ArtifactStoreKind::Peer(peer) => Ok(peer),
            _ => Err(AiError::InternalState),
        }
    }
}

impl ArtifactStore for PeerArtifactStore {
    fn descriptor(&self) -> ArtifactStoreDescriptor {
        self.descriptor.clone()
    }

    fn contains(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        self.transport.contains(self.trusted_peer()?, identity)
    }

    fn load(
        &self,
        identity: &ArtifactIdentity,
        max_bytes: u64,
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<u8>> {
        let result = (|| {
            check_cancel_size(identity, max_bytes, cancellation)?;
            let bytes =
                self.transport
                    .fetch(self.trusted_peer()?, identity, max_bytes, cancellation)?;
            verify(identity, &bytes)?;
            Ok(bytes)
        })();
        match &result {
            Ok(bytes) => {
                self.fetches.fetch_add(1, Ordering::Relaxed);
                self.transferred_bytes.fetch_add(
                    u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                    Ordering::Relaxed,
                );
            }
            Err(_) => {
                self.failures.fetch_add(1, Ordering::Relaxed);
            }
        }
        result
    }

    fn store(
        &self,
        identity: &ArtifactIdentity,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> AiResult<()> {
        check_cancel_size(identity, identity.size_bytes, cancellation)?;
        verify(identity, bytes)?;
        self.transport
            .put(self.trusted_peer()?, identity, bytes, cancellation)
    }

    fn remove(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        self.transport.remove(self.trusted_peer()?, identity)
    }
}

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

    /// Loads from the first available tier and promotes once without double-load races.
    pub fn load_and_promote(
        &self,
        identity: &ArtifactIdentity,
        max_bytes: u64,
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<u8>> {
        check_cancel_size(identity, max_bytes, cancellation)?;
        for (index, tier) in self.tiers.iter().enumerate() {
            if !tier.contains(identity)? {
                continue;
            }
            let bytes = tier.load(identity, max_bytes, cancellation)?;
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

fn check_cancel_size(
    identity: &ArtifactIdentity,
    max_bytes: u64,
    cancellation: &CancellationToken,
) -> AiResult<()> {
    if cancellation.is_cancelled() {
        return Err(AiError::Cancelled);
    }
    if identity.size_bytes > max_bytes {
        return Err(AiError::LimitExceeded {
            kind: crate::LimitKind::InputBytes,
            actual: identity.size_bytes,
            limit: max_bytes,
        });
    }
    Ok(())
}

fn verify(identity: &ArtifactIdentity, bytes: &[u8]) -> AiResult<()> {
    if u64::try_from(bytes.len()).ok() != Some(identity.size_bytes)
        || ArtifactDigest::from_bytes(bytes) != identity.digest
    {
        return Err(AiError::Integrity("artifact digest"));
    }
    Ok(())
}
