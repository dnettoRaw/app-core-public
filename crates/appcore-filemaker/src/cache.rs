// =============================================================================
//        #######
//     ###       ###     F: cache.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded cache contracts and behavior for this crate.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use crate::{
    DocumentFingerprint, ErrorCode, FileMakerError, ResolvedScene, ResourceLimits, Result,
};

/// Bounded deterministic compile-once/render-many scene cache.
pub struct SceneCache {
    capacity: usize,
    max_bytes: usize,
    used_bytes: usize,
    entries: BTreeMap<DocumentFingerprint, CacheEntry>,
    insertion_order: VecDeque<DocumentFingerprint>,
}

struct CacheEntry {
    scene: Arc<ResolvedScene>,
    bytes: usize,
}

impl SceneCache {
    /// Default aggregate serialized scene budget.
    pub const DEFAULT_MAX_BYTES: usize = 256 * 1024 * 1024;

    /// Creates a cache with an explicit non-zero entry bound.
    pub fn new(capacity: usize) -> Result<Self> {
        Self::with_byte_capacity(capacity, Self::DEFAULT_MAX_BYTES)
    }

    /// Creates a cache bounded by entries and aggregate serialized bytes.
    pub fn with_byte_capacity(capacity: usize, max_bytes: usize) -> Result<Self> {
        if capacity == 0 || max_bytes == 0 {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "scene cache entry and byte bounds must be non-zero",
            ));
        }
        Ok(Self {
            capacity,
            max_bytes,
            used_bytes: 0,
            entries: BTreeMap::new(),
            insertion_order: VecDeque::new(),
        })
    }

    /// Returns an immutable cached scene without changing eviction order.
    #[must_use]
    pub fn get(&self, key: &DocumentFingerprint) -> Option<Arc<ResolvedScene>> {
        self.entries.get(key).map(|entry| entry.scene.clone())
    }

    /// Returns a cached scene or resolves and inserts it exactly once on a miss.
    pub fn get_or_try_insert_with<F>(
        &mut self,
        key: DocumentFingerprint,
        limits: &ResourceLimits,
        resolve: F,
    ) -> Result<Arc<ResolvedScene>>
    where
        F: FnOnce() -> Result<ResolvedScene>,
    {
        if let Some(scene) = self.get(&key) {
            return Ok(scene);
        }
        self.insert(key, resolve()?, limits)
    }

    /// Inserts a fully resolved scene and evicts the oldest inserted key.
    pub fn insert(
        &mut self,
        key: DocumentFingerprint,
        scene: ResolvedScene,
        limits: &ResourceLimits,
    ) -> Result<Arc<ResolvedScene>> {
        if scene.engine_version != crate::ENGINE_VERSION {
            return Err(cache_error(
                "cached scene engine version does not match the active engine",
            ));
        }
        let elements = scene
            .pages
            .iter()
            .try_fold(0_usize, |total, page| {
                total.checked_add(page.elements.len())
            })
            .ok_or_else(|| cache_error("cached scene element count overflow"))?;
        if scene.pages.len() > limits.max_pages || elements > limits.max_elements {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "cached scene exceeds configured page or element budget",
            ));
        }
        if let Some(existing) = self.entries.get(&key) {
            return Ok(existing.scene.clone());
        }
        let bytes = crate::memory::serialized_size(&scene)?;
        if bytes > self.max_bytes {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "resolved scene exceeds the cache byte budget",
            ));
        }
        while self.entries.len() >= self.capacity
            || self.used_bytes.saturating_add(bytes) > self.max_bytes
        {
            if let Some(oldest) = self.insertion_order.pop_front() {
                if let Some(entry) = self.entries.remove(&oldest) {
                    self.used_bytes = self.used_bytes.saturating_sub(entry.bytes);
                }
            }
        }
        let scene = Arc::new(scene);
        self.entries.insert(
            key,
            CacheEntry {
                scene: scene.clone(),
                bytes,
            },
        );
        self.used_bytes += bytes;
        self.insertion_order.push_back(key);
        Ok(scene)
    }

    /// Number of retained scenes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no scenes are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Aggregate serialized bytes retained by cached scenes.
    #[must_use]
    pub const fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    /// Configured aggregate serialized scene byte budget.
    #[must_use]
    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }
}

fn cache_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::Validation, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(byte: u8) -> DocumentFingerprint {
        let mut builder = crate::FingerprintBuilder::new();
        builder.field("test", &[byte]).unwrap();
        builder.finish()
    }

    #[test]
    fn evicts_oldest_without_unbounded_growth() {
        let mut cache = SceneCache::new(1).unwrap();
        let scene = ResolvedScene {
            template_id: "cache".to_owned(),
            pages: Vec::new(),
            engine_version: crate::ENGINE_VERSION.to_owned(),
        };
        cache
            .insert(fingerprint(1), scene.clone(), &ResourceLimits::default())
            .unwrap();
        cache
            .insert(fingerprint(2), scene, &ResourceLimits::default())
            .unwrap();
        assert!(cache.get(&fingerprint(1)).is_none());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn resolves_only_once_for_a_repeated_fingerprint() {
        let mut cache = SceneCache::new(1).unwrap();
        let key = fingerprint(1);
        let mut calls = 0;
        for _ in 0..2 {
            cache
                .get_or_try_insert_with(key, &ResourceLimits::default(), || {
                    calls += 1;
                    Ok(ResolvedScene {
                        template_id: "cache".to_owned(),
                        pages: Vec::new(),
                        engine_version: crate::ENGINE_VERSION.to_owned(),
                    })
                })
                .unwrap();
        }
        assert_eq!(calls, 1);
    }

    #[test]
    fn rejects_a_scene_from_another_engine_version() {
        let mut cache = SceneCache::new(1).unwrap();
        let error = cache
            .insert(
                fingerprint(1),
                ResolvedScene {
                    template_id: "stale".to_owned(),
                    pages: Vec::new(),
                    engine_version: "other".to_owned(),
                },
                &ResourceLimits::default(),
            )
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Validation);
        assert!(cache.is_empty());
    }

    #[test]
    fn rejects_one_scene_larger_than_the_byte_budget() {
        let mut cache = SceneCache::with_byte_capacity(2, 1).unwrap();
        let error = cache
            .insert(
                fingerprint(1),
                ResolvedScene {
                    template_id: "oversized".to_owned(),
                    pages: Vec::new(),
                    engine_version: crate::ENGINE_VERSION.to_owned(),
                },
                &ResourceLimits::default(),
            )
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::LimitExceeded);
        assert_eq!(cache.used_bytes(), 0);
    }

    #[test]
    fn byte_budget_evicts_even_below_the_entry_capacity() {
        let scene = ResolvedScene {
            template_id: "bounded".to_owned(),
            pages: Vec::new(),
            engine_version: crate::ENGINE_VERSION.to_owned(),
        };
        let bytes = crate::memory::serialized_size(&scene).unwrap();
        let mut cache = SceneCache::with_byte_capacity(4, bytes).unwrap();
        cache
            .insert(fingerprint(1), scene.clone(), &ResourceLimits::default())
            .unwrap();
        cache
            .insert(fingerprint(2), scene, &ResourceLimits::default())
            .unwrap();
        assert!(cache.get(&fingerprint(1)).is_none());
        assert_eq!(cache.len(), 1);
        assert!(cache.used_bytes() <= cache.max_bytes());
    }
}
