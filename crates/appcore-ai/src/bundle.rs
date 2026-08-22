// =============================================================================
//        #######
//     ###       ###     F: bundle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiResult, ArtifactDigest, ArtifactIdentity, CancellationToken, CapabilityId,
    LocalArtifactCache,
};
use std::collections::BTreeSet;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

/// Semantic class of one independently verified model-bundle segment.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ArtifactSegmentKind {
    /// Tokenizer or vocabulary bytes.
    Tokenizer,
    /// Chat template or generation metadata.
    Metadata,
    /// Shared dense model weights.
    Weights,
    /// One mixture-of-experts shard.
    Expert(u32),
    /// One optional adapter such as LoRA weights.
    Adapter,
    /// Provider-defined data-only segment class.
    Other(CapabilityId),
}

/// Exact identity and location of one range inside a complete artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactSegment {
    /// Stable segment name unique inside the bundle.
    pub id: CapabilityId,
    /// Segment purpose used by a backend residency policy.
    pub kind: ArtifactSegmentKind,
    /// Byte offset from the start of the complete artifact.
    pub offset: u64,
    /// Exact byte length.
    pub length: u64,
    /// SHA-256 digest of exactly this range.
    pub digest: ArtifactDigest,
}

/// Bounded range manifest tied to one complete artifact identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactBundleManifest {
    /// Identity of the complete previously verified artifact.
    pub artifact: ArtifactIdentity,
    /// Sorted non-overlapping independently hashed ranges.
    pub segments: Vec<ArtifactSegment>,
}

impl ArtifactBundleManifest {
    /// Validates IDs, range arithmetic, ordering and per-bundle limits.
    pub fn validate(&self, max_segments: usize, max_segment_bytes: u64) -> AiResult<()> {
        if max_segments == 0
            || max_segment_bytes == 0
            || self.segments.is_empty()
            || self.segments.len() > max_segments
        {
            return Err(AiError::InvalidInput("artifact bundle bounds"));
        }
        let mut ids = BTreeSet::new();
        let mut previous_end = 0u64;
        for segment in &self.segments {
            let end = segment
                .offset
                .checked_add(segment.length)
                .ok_or(AiError::InvalidInput("artifact segment overflow"))?;
            if segment.length == 0
                || segment.length > max_segment_bytes
                || end > self.artifact.size_bytes
                || segment.offset < previous_end
                || !ids.insert(&segment.id)
            {
                return Err(AiError::InvalidInput("artifact segment"));
            }
            previous_end = end;
        }
        Ok(())
    }

    fn segment(&self, id: &CapabilityId) -> AiResult<&ArtifactSegment> {
        self.segments
            .iter()
            .find(|segment| &segment.id == id)
            .ok_or(AiError::NotFound("artifact segment"))
    }
}

/// Verified bytes for one requested bundle range.
#[derive(Clone, Eq, PartialEq)]
pub struct LoadedArtifactSegment {
    /// Stable segment identity.
    pub id: CapabilityId,
    /// Semantic class.
    pub kind: ArtifactSegmentKind,
    /// Verified exact range bytes.
    pub bytes: Vec<u8>,
}

impl Debug for LoadedArtifactSegment {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoadedArtifactSegment")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

/// Bounded reader used by backends that support segmented model residency.
#[derive(Debug)]
pub struct SegmentedModelReader {
    cache: Arc<LocalArtifactCache>,
    max_segments: usize,
    max_segment_bytes: u64,
    max_request_segments: usize,
    max_request_bytes: u64,
}

impl SegmentedModelReader {
    /// Creates a reader with explicit per-manifest and per-request bounds.
    pub fn new(
        cache: Arc<LocalArtifactCache>,
        max_segments: usize,
        max_segment_bytes: u64,
        max_request_segments: usize,
        max_request_bytes: u64,
    ) -> AiResult<Self> {
        if max_segments == 0
            || max_segment_bytes == 0
            || max_request_segments == 0
            || max_request_segments > max_segments
            || max_request_bytes == 0
        {
            return Err(AiError::InvalidInput("segmented reader bounds"));
        }
        Ok(Self {
            cache,
            max_segments,
            max_segment_bytes,
            max_request_segments,
            max_request_bytes,
        })
    }

    /// Loads selected ranges in caller order and verifies every segment digest.
    pub fn load(
        &self,
        manifest: &ArtifactBundleManifest,
        segment_ids: &[CapabilityId],
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<LoadedArtifactSegment>> {
        manifest.validate(self.max_segments, self.max_segment_bytes)?;
        if segment_ids.is_empty() || segment_ids.len() > self.max_request_segments {
            return Err(AiError::LimitExceeded {
                kind: crate::LimitKind::InputParts,
                actual: u64::try_from(segment_ids.len()).unwrap_or(u64::MAX),
                limit: u64::try_from(self.max_request_segments).unwrap_or(u64::MAX),
            });
        }
        let mut total = 0u64;
        let mut unique = BTreeSet::new();
        let mut loaded = Vec::with_capacity(segment_ids.len());
        for id in segment_ids {
            if cancellation.is_cancelled() {
                return Err(AiError::Cancelled);
            }
            if !unique.insert(id) {
                return Err(AiError::InvalidInput("duplicate requested segment"));
            }
            let segment = manifest.segment(id)?;
            total = total
                .checked_add(segment.length)
                .ok_or(AiError::InvalidInput("segment request overflow"))?;
            if total > self.max_request_bytes {
                return Err(AiError::LimitExceeded {
                    kind: crate::LimitKind::InputBytes,
                    actual: total,
                    limit: self.max_request_bytes,
                });
            }
            let bytes = self.cache.load_range(
                &manifest.artifact,
                segment.offset,
                segment.length,
                self.max_segment_bytes.min(self.max_request_bytes),
            )?;
            if ArtifactDigest::from_bytes(&bytes) != segment.digest {
                return Err(AiError::Integrity("artifact segment digest"));
            }
            loaded.push(LoadedArtifactSegment {
                id: segment.id.clone(),
                kind: segment.kind.clone(),
                bytes,
            });
        }
        Ok(loaded)
    }
}
