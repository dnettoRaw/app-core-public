// =============================================================================
//        #######
//     ###       ###     F: scene.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    scene_storage::{reserved, Archetype, Location, Slot, SnapshotPermit},
    EntityId, MovingColumns, RenderBinding, RenderError, SceneInstance, SceneSnapshot, Transform,
    Velocity,
};
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};
static NEXT_WORLD: AtomicU64 = AtomicU64::new(1);
/// CPU ownership budgets. The byte ceiling accounts columns, slots, free indices, snapshots and step scratch,
/// excluding allocator overhead and independently owned mesh/material assets.
#[derive(Debug, Clone, Copy)]
pub struct SceneLimits {
    /// Maximum live/retired identity slots and capacity of each maintained archetype.
    pub max_entities: u32,
    /// Maximum instances copied into one frame snapshot.
    pub max_snapshot_instances: u32,
    /// Maximum simultaneously owned snapshots, in 1..8.
    pub max_snapshots: u32,
    /// Accounted CPU reservation ceiling, at most 512 MiB.
    pub managed_cpu_bytes: u64,
}
impl Default for SceneLimits {
    fn default() -> Self {
        Self {
            max_entities: 8192,
            max_snapshot_instances: 8192,
            max_snapshots: 2,
            managed_cpu_bytes: 32 * 1024 * 1024,
        }
    }
}
impl SceneLimits {
    /// Validate capacity and compute the conservative reservation before allocating columns.
    pub fn reservation_bytes(self) -> Result<u64, RenderError> {
        if self.max_entities == 0
            || self.max_entities > 65536
            || self.max_snapshot_instances == 0
            || self.max_snapshot_instances > self.max_entities
            || !(1..=8).contains(&self.max_snapshots)
            || self.managed_cpu_bytes > 512 * 1024 * 1024
        {
            return Err(RenderError::InvalidLimits);
        }
        let row = std::mem::size_of::<EntityId>()
            + std::mem::size_of::<Transform>()
            + std::mem::size_of::<RenderBinding>();
        let per_entity = 2 * row
            + std::mem::size_of::<Velocity>()
            + std::mem::size_of::<Slot>()
            + std::mem::size_of::<u32>()
            + std::mem::size_of::<Transform>();
        let bytes = u64::from(self.max_entities) * per_entity as u64
            + u64::from(self.max_snapshots)
                * u64::from(self.max_snapshot_instances)
                * std::mem::size_of::<SceneInstance>() as u64;
        if bytes > self.managed_cpu_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        Ok(bytes)
    }
}
/// Native two-archetype ECS: render-only and render+velocity. No external ECS or locks.
/// IDs index a location table; row compaction always repairs the moved entity's location.
pub struct SceneWorld {
    owner: u64,
    limits: SceneLimits,
    slots: Vec<Slot>,
    free_slots: Vec<u32>,
    archetypes: [Archetype; 2],
    live: usize,
    snapshots: Arc<AtomicU32>,
}
impl SceneWorld {
    /// Reserve bounded columns after validating the CPU budget.
    pub fn new(limits: SceneLimits) -> Result<Self, RenderError> {
        limits.reservation_bytes()?;
        let owner = NEXT_WORLD
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| RenderError::IdentityExhausted)?;
        let capacity = limits.max_entities as usize;
        Ok(Self {
            owner,
            limits,
            slots: reserved(capacity)?,
            free_slots: reserved(capacity)?,
            archetypes: [
                Archetype::new(capacity, false)?,
                Archetype::new(capacity, true)?,
            ],
            live: 0,
            snapshots: Arc::new(AtomicU32::new(0)),
        })
    }
    /// Live entities, bounded by the identity slot capacity.
    pub fn len(&self) -> usize {
        self.live
    }
    /// Whether no entities remain.
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }
    /// Spawn a validated instance in the matching archetype. Failure makes no structural change.
    /// Slot admission is constant-time through a bounded free list; reuse order is unspecified.
    pub fn spawn(
        &mut self,
        transform: Transform,
        binding: RenderBinding,
        velocity: Option<Velocity>,
    ) -> Result<EntityId, RenderError> {
        let binding = binding.validate()?;
        let index = if let Some(index) = self.free_slots.pop() {
            index as usize
        } else {
            if self.slots.len() >= self.limits.max_entities as usize {
                return Err(RenderError::BudgetExceeded);
            }
            self.slots.push(Slot {
                generation: 0,
                location: None,
            });
            self.slots.len() - 1
        };
        let slot = &mut self.slots[index];
        slot.generation += 1;
        let id = EntityId {
            owner: self.owner,
            index: index as u32,
            generation: slot.generation,
        };
        let archetype = usize::from(velocity.is_some());
        let row = self.archetypes[archetype].push(id, transform, binding, velocity);
        self.slots[index].location = Some(Location { archetype, row });
        self.live += 1;
        Ok(id)
    }
    fn locate(&self, id: EntityId) -> Result<Location, RenderError> {
        if id.owner != self.owner {
            return Err(RenderError::InvalidResource);
        }
        self.slots
            .get(id.index as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.location)
            .ok_or(RenderError::InvalidResource)
    }
    fn remove_row(&mut self, location: Location) -> (Transform, RenderBinding, Option<Velocity>) {
        let table = &mut self.archetypes[location.archetype];
        let values = table.remove(location.row);
        if let Some(moved) = table.entities.get(location.row) {
            self.slots[moved.index as usize].location = Some(location);
        }
        values
    }
    /// Remove without invalidating other identities; stale handles never alias a later spawn.
    pub fn despawn(&mut self, id: EntityId) -> Result<(), RenderError> {
        let location = self.locate(id)?;
        self.remove_row(location);
        let slot = &mut self.slots[id.index as usize];
        slot.location = None;
        if slot.generation < u64::MAX {
            self.free_slots.push(id.index);
        }
        self.live -= 1;
        Ok(())
    }
    /// Read the current validated transform by stable identity.
    pub fn transform(&self, id: EntityId) -> Result<Transform, RenderError> {
        let l = self.locate(id)?;
        Ok(self.archetypes[l.archetype].transforms[l.row])
    }
    /// Replace a validated transform without moving its row or binding.
    pub fn set_transform(&mut self, id: EntityId, value: Transform) -> Result<(), RenderError> {
        let l = self.locate(id)?;
        self.archetypes[l.archetype].transforms[l.row] = value;
        Ok(())
    }
    /// Add/remove velocity through explicit archetype migration, preserving entity identity.
    pub fn set_velocity(
        &mut self,
        id: EntityId,
        velocity: Option<Velocity>,
    ) -> Result<(), RenderError> {
        let l = self.locate(id)?;
        let target = usize::from(velocity.is_some());
        if l.archetype == target {
            if let (Some(column), Some(value)) = (&mut self.archetypes[target].velocities, velocity)
            {
                column[l.row] = value;
            }
            return Ok(());
        }
        let (transform, binding, _) = self.remove_row(l);
        let row = self.archetypes[target].push(id, transform, binding, velocity);
        self.slots[id.index as usize].location = Some(Location {
            archetype: target,
            row,
        });
        Ok(())
    }
    /// Borrow disjoint aligned transform/velocity columns. Structural mutation requires releasing them.
    pub fn moving_columns(&mut self) -> MovingColumns<'_> {
        let table = &mut self.archetypes[1];
        MovingColumns {
            entities: &table.entities,
            transforms: &mut table.transforms,
            velocities: table
                .velocities
                .as_mut()
                .map_or(&mut [], |v| v.as_mut_slice()),
        }
    }
    /// Integrate local velocity with a bounded timestep; validate every candidate before committing any.
    pub fn step_velocity(&mut self, dt: f32) -> Result<(), RenderError> {
        if !dt.is_finite() || !(0.0..=0.05).contains(&dt) {
            return Err(RenderError::InvalidLimits);
        }
        let table = &mut self.archetypes[1];
        let velocities = table
            .velocities
            .as_ref()
            .ok_or(RenderError::InvalidLimits)?;
        let mut next = reserved(table.transforms.len())?;
        for (transform, velocity) in table.transforms.iter().zip(velocities) {
            next.push(transform.with_position(std::array::from_fn(|i| {
                transform.position()[i] + velocity.value()[i] * dt
            }))?);
        }
        table.transforms.copy_from_slice(&next);
        Ok(())
    }
    /// Extract owned instances; rejects excess before allocating the snapshot.
    /// A renderer must resolve asset identities before uploading or submitting a draw.
    pub fn snapshot(&self) -> Result<SceneSnapshot, RenderError> {
        if self.live > self.limits.max_snapshot_instances as usize {
            return Err(RenderError::BudgetExceeded);
        }
        self.snapshots
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.limits.max_snapshots).then_some(n + 1)
            })
            .map_err(|_| RenderError::SnapshotsPending)?;
        let permit = SnapshotPermit(self.snapshots.clone());
        let mut instances = reserved(self.live)?;
        for table in &self.archetypes {
            for i in 0..table.entities.len() {
                instances.push(SceneInstance {
                    entity: table.entities[i],
                    transform: table.transforms[i],
                    binding: table.bindings[i],
                });
            }
        }
        Ok(SceneSnapshot {
            _permit: permit,
            instances,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_generation_is_retired_without_reentering_the_free_list() -> Result<(), RenderError>
    {
        let limits = SceneLimits {
            max_entities: 1,
            max_snapshot_instances: 1,
            ..Default::default()
        };
        let mut world = SceneWorld::new(limits)?;
        let mut resources = crate::ResourceArena::new(crate::RenderLimits::default())?;
        let binding = RenderBinding {
            mesh: resources.insert((), 1)?,
            material: resources.insert((), 1)?,
            tint: [1.; 4],
            animation: 0,
            morph: 0,
        };
        let initial = world.spawn(Transform::default(), binding, None)?;
        let exhausted = EntityId {
            generation: u64::MAX,
            ..initial
        };
        world.slots[0].generation = u64::MAX;
        world.archetypes[0].entities[0] = exhausted;
        world.despawn(exhausted)?;
        assert!(world.free_slots.is_empty());
        assert_eq!(
            world.spawn(Transform::default(), binding, None),
            Err(RenderError::BudgetExceeded)
        );
        assert_eq!(world.transform(initial), Err(RenderError::InvalidResource));
        assert_eq!(
            world.transform(exhausted),
            Err(RenderError::InvalidResource)
        );
        Ok(())
    }
}
