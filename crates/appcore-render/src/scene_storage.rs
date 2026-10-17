// =============================================================================
//        #######
//     ###       ###     F: scene_storage.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{RenderBinding, RenderError, Transform, Velocity};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
/// Scene-scoped generational identity, stable when a row moves between archetypes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityId {
    pub(crate) owner: u64,
    pub(crate) index: u32,
    pub(crate) generation: u64,
}
#[derive(Clone, Copy)]
pub(crate) struct Location {
    pub(crate) archetype: usize,
    pub(crate) row: usize,
}
pub(crate) struct Slot {
    pub(crate) generation: u64,
    pub(crate) location: Option<Location>,
}
pub(crate) struct Archetype {
    pub(crate) entities: Vec<EntityId>,
    pub(crate) transforms: Vec<Transform>,
    pub(crate) bindings: Vec<RenderBinding>,
    pub(crate) velocities: Option<Vec<Velocity>>,
}
impl Archetype {
    pub(crate) fn new(capacity: usize, moving: bool) -> Result<Self, RenderError> {
        Ok(Self {
            entities: reserved(capacity)?,
            transforms: reserved(capacity)?,
            bindings: reserved(capacity)?,
            velocities: if moving {
                Some(reserved(capacity)?)
            } else {
                None
            },
        })
    }
    pub(crate) fn push(
        &mut self,
        id: EntityId,
        transform: Transform,
        binding: RenderBinding,
        velocity: Option<Velocity>,
    ) -> usize {
        let row = self.entities.len();
        self.entities.push(id);
        self.transforms.push(transform);
        self.bindings.push(binding);
        if let (Some(column), Some(value)) = (&mut self.velocities, velocity) {
            column.push(value);
        }
        row
    }
    pub(crate) fn remove(&mut self, row: usize) -> (Transform, RenderBinding, Option<Velocity>) {
        self.entities.swap_remove(row);
        (
            self.transforms.swap_remove(row),
            self.bindings.swap_remove(row),
            self.velocities.as_mut().map(|v| v.swap_remove(row)),
        )
    }
}
/// Disjoint mutable columns borrowed from the moving archetype; structural changes
/// cannot occur while this value is borrowed. Component types preserve numeric validity.
pub struct MovingColumns<'a> {
    /// Row-aligned stable identities.
    pub entities: &'a [EntityId],
    /// Aligned contiguous transforms.
    pub transforms: &'a mut [Transform],
    /// Aligned contiguous velocities.
    pub velocities: &'a mut [Velocity],
}
/// Immutable extracted instance; does not borrow the world or resolve its assets.
#[derive(Debug, Clone, Copy)]
pub struct SceneInstance {
    /// Identity at extraction time, not a mutable entity reference.
    pub entity: EntityId,
    /// Validated local transform.
    pub transform: Transform,
    /// Declared asset/appearance binding.
    pub binding: RenderBinding,
}
/// Owned bounded frame snapshot; world mutations cannot invalidate its storage.
pub(crate) struct SnapshotPermit(pub(crate) Arc<AtomicU32>);
impl Drop for SnapshotPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

pub struct SceneSnapshot {
    pub(crate) _permit: SnapshotPermit,
    pub(crate) instances: Vec<SceneInstance>,
}
impl SceneSnapshot {
    /// Contiguous immutable instances, in archetype order rather than spawn order.
    pub fn instances(&self) -> &[SceneInstance] {
        &self.instances
    }
}

pub(crate) fn reserved<T>(capacity: usize) -> Result<Vec<T>, RenderError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| RenderError::CpuAllocationFailed)?;
    Ok(values)
}
