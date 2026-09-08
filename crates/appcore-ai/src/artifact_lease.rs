// =============================================================================
//        #######
//     ###       ###     F: artifact_lease.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.3
// =============================================================================

//! Provides borrowed-use ownership for complete verified artifact bytes.

use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::sync::Arc;

enum ArtifactLeaseBytes {
    Owned(Vec<u8>),
    Shared(Arc<[u8]>),
}

/// Keeps verified artifact bytes alive without requiring every consumer to copy them.
pub struct ArtifactLease {
    bytes: ArtifactLeaseBytes,
}

impl ArtifactLease {
    pub(super) fn owned(bytes: Vec<u8>) -> Self {
        Self {
            bytes: ArtifactLeaseBytes::Owned(bytes),
        }
    }

    pub(super) fn shared(bytes: Arc<[u8]>) -> Self {
        Self {
            bytes: ArtifactLeaseBytes::Shared(bytes),
        }
    }

    /// Returns the verified artifact size.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_ref().len()
    }

    /// Reports whether the verified artifact is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }

    /// Converts the lease into compatibility-owned bytes.
    ///
    /// An already owned lease moves its allocation. A shared lease must copy
    /// because returning `Vec<u8>` transfers unique mutable ownership.
    #[must_use]
    pub fn into_vec(self) -> Vec<u8> {
        match self.bytes {
            ArtifactLeaseBytes::Owned(bytes) => bytes,
            ArtifactLeaseBytes::Shared(bytes) => bytes.as_ref().to_vec(),
        }
    }
}

impl AsRef<[u8]> for ArtifactLease {
    fn as_ref(&self) -> &[u8] {
        match &self.bytes {
            ArtifactLeaseBytes::Owned(bytes) => bytes,
            ArtifactLeaseBytes::Shared(bytes) => bytes,
        }
    }
}

impl Deref for ArtifactLease {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl Debug for ArtifactLease {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ArtifactLease")
            .field("bytes", &self.len())
            .finish_non_exhaustive()
    }
}
