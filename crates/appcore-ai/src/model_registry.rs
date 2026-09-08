// =============================================================================
//        #######
//     ###       ###     F: model_registry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.3
// =============================================================================

//! Owns bounded model metadata retention and lifecycle transitions.

use crate::{AiError, AiResult, AiTask, ArtifactLocation, ModelDescriptor, ModelId, ModelState};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

const MAX_REGISTERED_MODELS: usize = 4_096;
const MAX_LOCATIONS_PER_MODEL: usize = 128;
const MAX_REGISTRY_LOCATIONS: usize = 65_536;
const MAX_REGISTRY_LOCATION_BYTES: usize = 8 * 1_024 * 1_024;

/// Low-cardinality model registry lifecycle summary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModelRegistrySnapshot {
    /// All registered logical models.
    pub registered: usize,
    /// Models with at least one verified byte location.
    pub available: usize,
    /// Models currently loading.
    pub loading: usize,
    /// Models ready in a backend.
    pub ready: usize,
    /// Models in failed state.
    pub failed: usize,
}

/// Configurable model-registry retention bounds, capped by crate safety ceilings.
///
/// Accounted location bytes include each `ArtifactLocation` value and the
/// validated peer/device identifier bytes. Collection-node overhead is bounded
/// independently by the per-model and registry-wide item ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelRegistryLimits {
    /// Maximum logical models retained at once.
    pub max_models: usize,
    /// Maximum location items accepted in one model registration or record.
    pub max_locations_per_model: usize,
    /// Maximum artifact locations retained across the complete registry.
    pub max_total_locations: usize,
    /// Maximum accounted location bytes retained across the complete registry.
    pub max_total_location_bytes: usize,
}

impl Default for ModelRegistryLimits {
    fn default() -> Self {
        Self {
            max_models: MAX_REGISTERED_MODELS,
            max_locations_per_model: MAX_LOCATIONS_PER_MODEL,
            max_total_locations: MAX_REGISTRY_LOCATIONS,
            max_total_location_bytes: MAX_REGISTRY_LOCATION_BYTES,
        }
    }
}

/// Payload-free pressure metrics for model-registry location retention.
///
/// Byte gauges use the same accounting defined by [`ModelRegistryLimits`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModelRegistryPressure {
    /// Artifact locations currently retained.
    pub current_locations: usize,
    /// Highest retained location count since registry creation.
    pub peak_locations: usize,
    /// Configured registry-wide location ceiling.
    pub max_total_locations: usize,
    /// Configured per-model location ceiling.
    pub max_locations_per_model: usize,
    /// Accounted location bytes currently retained.
    pub current_location_bytes: usize,
    /// Highest accounted location bytes retained since registry creation.
    pub peak_location_bytes: usize,
    /// Configured registry-wide accounted-byte ceiling.
    pub max_total_location_bytes: usize,
    /// Location admissions rejected by either configured ceiling.
    pub rejected_locations: u64,
}

/// Mutable registry view of one logical model.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelRecord {
    /// Immutable model descriptor.
    pub descriptor: ModelDescriptor,
    /// Current lifecycle state.
    pub state: ModelState,
    /// Deduplicated artifact locations.
    pub locations: BTreeSet<ArtifactLocation>,
}

/// Immutable shared snapshot of one model record.
///
/// A lease never holds the registry lock. A later registry mutation uses
/// copy-on-write, so an existing lease continues to expose its point-in-time
/// state without copying the descriptor on the read path.
#[derive(Clone, Debug)]
pub struct ModelRecordLease(pub(crate) Arc<ModelRecord>);

impl ModelRecordLease {
    /// Returns the immutable point-in-time record.
    #[must_use]
    pub fn as_record(&self) -> &ModelRecord {
        &self.0
    }

    pub(crate) fn shared(&self) -> Arc<ModelRecord> {
        Arc::clone(&self.0)
    }
}

impl Deref for ModelRecordLease {
    type Target = ModelRecord;

    fn deref(&self) -> &Self::Target {
        self.as_record()
    }
}

/// Thread-safe model metadata and lifecycle registry.
#[derive(Debug)]
pub struct ModelRegistry {
    limits: ModelRegistryLimits,
    models: RwLock<BTreeMap<ModelId, Arc<ModelRecord>>>,
    current_locations: AtomicUsize,
    peak_locations: AtomicUsize,
    current_location_bytes: AtomicUsize,
    peak_location_bytes: AtomicUsize,
    rejected_locations: AtomicU64,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self {
            limits: ModelRegistryLimits::default(),
            models: RwLock::new(BTreeMap::new()),
            current_locations: AtomicUsize::new(0),
            peak_locations: AtomicUsize::new(0),
            current_location_bytes: AtomicUsize::new(0),
            peak_location_bytes: AtomicUsize::new(0),
            rejected_locations: AtomicU64::new(0),
        }
    }
}

impl ModelRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty registry with tighter caller-selected retention bounds.
    pub fn with_limits(limits: ModelRegistryLimits) -> AiResult<Self> {
        validate_limits(limits)?;
        Ok(Self {
            limits,
            models: RwLock::new(BTreeMap::new()),
            current_locations: AtomicUsize::new(0),
            peak_locations: AtomicUsize::new(0),
            current_location_bytes: AtomicUsize::new(0),
            peak_location_bytes: AtomicUsize::new(0),
            rejected_locations: AtomicU64::new(0),
        })
    }

    /// Registers one new model and rejects duplicate IDs.
    pub fn register(
        &self,
        descriptor: ModelDescriptor,
        locations: impl IntoIterator<Item = ArtifactLocation>,
    ) -> AiResult<()> {
        descriptor.validate()?;
        let retained_locations = self.collect_locations(locations)?;
        let state = if retained_locations.is_empty() {
            ModelState::Discovered
        } else {
            ModelState::Available
        };
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        if models.contains_key(&descriptor.id) {
            return Err(AiError::Conflict("model id"));
        }
        if models.len() >= self.limits.max_models {
            return Err(AiError::Capacity("model registry"));
        }
        let current = self.current_locations.load(Ordering::Relaxed);
        let Some(next) = current.checked_add(retained_locations.len()) else {
            return self.reject_location_capacity();
        };
        let added_bytes = locations_retained_bytes(&retained_locations)?;
        let current_bytes = self.current_location_bytes.load(Ordering::Relaxed);
        let Some(next_bytes) = current_bytes.checked_add(added_bytes) else {
            return self.reject_location_capacity();
        };
        if next > self.limits.max_total_locations
            || next_bytes > self.limits.max_total_location_bytes
        {
            return self.reject_location_capacity();
        }
        models.insert(
            descriptor.id.clone(),
            Arc::new(ModelRecord {
                descriptor,
                state,
                locations: retained_locations,
            }),
        );
        self.record_location_usage(next, next_bytes);
        Ok(())
    }

    /// Returns a cloned record without holding a registry lock.
    pub fn get(&self, id: &ModelId) -> AiResult<ModelRecord> {
        Ok(self.get_lease(id)?.as_record().clone())
    }

    /// Returns a shared immutable record without holding a registry lock.
    pub fn get_lease(&self, id: &ModelId) -> AiResult<ModelRecordLease> {
        self.models
            .read()
            .map_err(|_| AiError::InternalState)?
            .get(id)
            .map(|record| ModelRecordLease(Arc::clone(record)))
            .ok_or(AiError::NotFound("model"))
    }

    /// Returns all compatible records in stable ID order.
    pub fn candidates(&self, task: &AiTask) -> AiResult<Vec<ModelRecord>> {
        Ok(self
            .candidate_leases(task)?
            .into_iter()
            .map(|record| record.as_record().clone())
            .collect())
    }

    /// Returns compatible shared snapshots in stable ID order.
    pub fn candidate_leases(&self, task: &AiTask) -> AiResult<Vec<ModelRecordLease>> {
        Ok(self
            .models
            .read()
            .map_err(|_| AiError::InternalState)?
            .values()
            .filter(|record| record.descriptor.supports_task(task))
            .map(|record| ModelRecordLease(Arc::clone(record)))
            .collect())
    }

    /// Applies one explicit lifecycle transition.
    pub fn transition(&self, id: &ModelId, next: ModelState) -> AiResult<()> {
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        let record = models.get_mut(id).ok_or(AiError::NotFound("model"))?;
        if record.state == next {
            return Ok(());
        }
        if !valid_transition(record.state, next) {
            return Err(AiError::Conflict("model state transition"));
        }
        Arc::make_mut(record).state = next;
        Ok(())
    }

    pub(crate) fn note_load_started(&self, id: &ModelId) -> AiResult<()> {
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        let record = models.get_mut(id).ok_or(AiError::NotFound("model"))?;
        if matches!(record.state, ModelState::Available | ModelState::Failed) {
            Arc::make_mut(record).state = ModelState::Loading;
        }
        Ok(())
    }

    pub(crate) fn note_load_finished(&self, id: &ModelId, success: bool) -> AiResult<()> {
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        let record = models.get_mut(id).ok_or(AiError::NotFound("model"))?;
        let next = if success {
            ModelState::Ready
        } else if record.state != ModelState::Ready {
            ModelState::Failed
        } else {
            return Ok(());
        };
        if record.state != next {
            Arc::make_mut(record).state = next;
        }
        Ok(())
    }

    /// Adds one location without changing artifact identity.
    pub fn add_location(&self, id: &ModelId, location: ArtifactLocation) -> AiResult<()> {
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        let record = models.get_mut(id).ok_or(AiError::NotFound("model"))?;
        if record.locations.contains(&location) {
            return Ok(());
        }
        let current = self.current_locations.load(Ordering::Relaxed);
        let current_bytes = self.current_location_bytes.load(Ordering::Relaxed);
        let location_bytes = location_retained_bytes(&location);
        if record.locations.len() >= self.limits.max_locations_per_model
            || current >= self.limits.max_total_locations
        {
            return self.reject_location_capacity();
        }
        let Some(next_bytes) = current_bytes.checked_add(location_bytes) else {
            return self.reject_location_capacity();
        };
        if next_bytes > self.limits.max_total_location_bytes {
            return self.reject_location_capacity();
        }
        let record = Arc::make_mut(record);
        record.locations.insert(location);
        if record.state == ModelState::Discovered {
            record.state = ModelState::Available;
        }
        self.record_location_usage(current.saturating_add(1), next_bytes);
        Ok(())
    }

    /// Removes one stale location while preserving logical identity.
    pub fn remove_location(&self, id: &ModelId, location: &ArtifactLocation) -> AiResult<bool> {
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        let record = models.get_mut(id).ok_or(AiError::NotFound("model"))?;
        if !record.locations.contains(location) {
            return Ok(false);
        }
        let record = Arc::make_mut(record);
        let removed = record.locations.remove(location);
        if removed {
            self.current_locations.fetch_sub(1, Ordering::Relaxed);
            self.current_location_bytes
                .fetch_sub(location_retained_bytes(location), Ordering::Relaxed);
        }
        Ok(removed)
    }

    /// Returns payload-free location-retention pressure without locking the registry.
    #[must_use]
    pub fn pressure(&self) -> ModelRegistryPressure {
        ModelRegistryPressure {
            current_locations: self.current_locations.load(Ordering::Relaxed),
            peak_locations: self.peak_locations.load(Ordering::Relaxed),
            max_total_locations: self.limits.max_total_locations,
            max_locations_per_model: self.limits.max_locations_per_model,
            current_location_bytes: self.current_location_bytes.load(Ordering::Relaxed),
            peak_location_bytes: self.peak_location_bytes.load(Ordering::Relaxed),
            max_total_location_bytes: self.limits.max_total_location_bytes,
            rejected_locations: self.rejected_locations.load(Ordering::Relaxed),
        }
    }

    /// Returns aggregate lifecycle state without high-cardinality model labels.
    pub fn snapshot(&self) -> AiResult<ModelRegistrySnapshot> {
        let models = self.models.read().map_err(|_| AiError::InternalState)?;
        let mut snapshot = ModelRegistrySnapshot {
            registered: models.len(),
            ..ModelRegistrySnapshot::default()
        };
        for record in models.values() {
            match record.state {
                ModelState::Available => snapshot.available = snapshot.available.saturating_add(1),
                ModelState::Loading => snapshot.loading = snapshot.loading.saturating_add(1),
                ModelState::Ready => snapshot.ready = snapshot.ready.saturating_add(1),
                ModelState::Failed => snapshot.failed = snapshot.failed.saturating_add(1),
                ModelState::Discovered | ModelState::Evicting => {}
            }
        }
        Ok(snapshot)
    }

    fn collect_locations(
        &self,
        locations: impl IntoIterator<Item = ArtifactLocation>,
    ) -> AiResult<BTreeSet<ArtifactLocation>> {
        let mut retained = BTreeSet::new();
        for (index, location) in locations.into_iter().enumerate() {
            if index >= self.limits.max_locations_per_model {
                return self.reject_location_capacity();
            }
            retained.insert(location);
        }
        Ok(retained)
    }

    fn record_location_usage(&self, current: usize, current_bytes: usize) {
        self.current_locations.store(current, Ordering::Relaxed);
        self.peak_locations.fetch_max(current, Ordering::Relaxed);
        self.current_location_bytes
            .store(current_bytes, Ordering::Relaxed);
        self.peak_location_bytes
            .fetch_max(current_bytes, Ordering::Relaxed);
    }

    fn reject_location_capacity<T>(&self) -> AiResult<T> {
        self.rejected_locations.fetch_add(1, Ordering::Relaxed);
        Err(AiError::Capacity("model artifact locations"))
    }
}

fn validate_limits(limits: ModelRegistryLimits) -> AiResult<()> {
    if limits.max_models == 0
        || limits.max_models > MAX_REGISTERED_MODELS
        || limits.max_locations_per_model == 0
        || limits.max_locations_per_model > MAX_LOCATIONS_PER_MODEL
        || limits.max_total_locations == 0
        || limits.max_total_locations > MAX_REGISTRY_LOCATIONS
        || limits.max_total_location_bytes == 0
        || limits.max_total_location_bytes > MAX_REGISTRY_LOCATION_BYTES
    {
        Err(AiError::InvalidInput("model registry limits"))
    } else {
        Ok(())
    }
}

fn locations_retained_bytes(locations: &BTreeSet<ArtifactLocation>) -> AiResult<usize> {
    locations.iter().try_fold(0usize, |bytes, location| {
        bytes
            .checked_add(location_retained_bytes(location))
            .ok_or(AiError::Capacity("model artifact locations"))
    })
}

fn location_retained_bytes(location: &ArtifactLocation) -> usize {
    let identifier_bytes = match location {
        ArtifactLocation::Vram(device) => device.as_str().len(),
        ArtifactLocation::Peer(peer) => peer.as_str().len(),
        ArtifactLocation::Memory | ArtifactLocation::LocalStorage => 0,
    };
    std::mem::size_of::<ArtifactLocation>().saturating_add(identifier_bytes)
}

fn valid_transition(current: ModelState, next: ModelState) -> bool {
    matches!(
        (current, next),
        (
            ModelState::Discovered,
            ModelState::Available | ModelState::Failed
        ) | (
            ModelState::Available,
            ModelState::Loading | ModelState::Failed
        ) | (ModelState::Loading, ModelState::Ready | ModelState::Failed)
            | (ModelState::Ready, ModelState::Evicting | ModelState::Failed)
            | (
                ModelState::Evicting,
                ModelState::Available | ModelState::Failed
            )
            | (ModelState::Failed, ModelState::Available)
    )
}
