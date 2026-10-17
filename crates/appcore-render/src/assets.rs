// =============================================================================
//        #######
//     ###       ###     F: assets.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    asset_data::{bounds, AssetLease, Material},
    scene_storage::reserved,
    MeshAsset, MeshVertex, PreparedInstance, PreparedScene, RenderError, RenderLimits,
    ResourceArena, ResourceId, SceneSnapshot,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
/// Bounded static asset/CPU preparation profile, separate from GPU/driver allocations.
#[derive(Debug, Clone, Copy)]
pub struct AssetLimits {
    /// Maximum registered meshes and materials each, in 1..65536.
    pub max_assets: u32,
    /// Per-mesh vertex count/capacity, in 3..65536.
    pub max_vertices: u32,
    /// Per-mesh index count/capacity, in 3..196608.
    pub max_indices: u32,
    /// Instances prepared per frame, in 1..65536.
    pub max_instances: u32,
    /// Shared payload ceiling, including assets retired but held by prepared frames.
    pub managed_cpu_bytes: u64,
}
impl Default for AssetLimits {
    fn default() -> Self {
        Self {
            max_assets: 512,
            max_vertices: 65536,
            max_indices: 196608,
            max_instances: 8192,
            managed_cpu_bytes: 128 * 1024 * 1024,
        }
    }
}
impl AssetLimits {
    /// Validate limits before creating any retained table.
    pub fn validate(self) -> Result<Self, RenderError> {
        if !(1..=65536).contains(&self.max_assets)
            || !(3..=65536).contains(&self.max_vertices)
            || !(3..=196608).contains(&self.max_indices)
            || !(1..=65536).contains(&self.max_instances)
            || self.managed_cpu_bytes == 0
            || self.managed_cpu_bytes > 512 * 1024 * 1024
        {
            return Err(RenderError::InvalidLimits);
        }
        Ok(self)
    }
}
/// Static mesh/material registry. IDs are typed by separate arenas and generations.
/// Geometry is immutable; deleting a registry entry does not invalidate already prepared frames.
pub struct AssetRegistry {
    limits: AssetLimits,
    meshes: ResourceArena<Arc<MeshAsset>>,
    materials: ResourceArena<Arc<Material>>,
    bytes: Arc<AtomicU64>,
}
impl AssetRegistry {
    /// Initialize explicit per-registry limits; no file/network access or GPU allocation.
    pub fn new(limits: AssetLimits) -> Result<Self, RenderError> {
        let limits = limits.validate()?;
        let arena = RenderLimits {
            resource_slots: limits.max_assets,
            managed_bytes: limits.managed_cpu_bytes,
            ..Default::default()
        };
        Ok(Self {
            limits,
            meshes: ResourceArena::new(arena)?,
            materials: ResourceArena::new(arena)?,
            bytes: Arc::new(AtomicU64::new(0)),
        })
    }
    fn reserve(&self, bytes: u64) -> Result<AssetLease, RenderError> {
        self.bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                n.checked_add(bytes)
                    .filter(|total| *total <= self.limits.managed_cpu_bytes)
            })
            .map_err(|_| RenderError::BudgetExceeded)?;
        Ok(AssetLease {
            counter: self.bytes.clone(),
            bytes,
        })
    }
    /// Accounted CPU payload, including prepared frames and retired shared assets.
    pub fn managed_bytes(&self) -> u64 {
        self.bytes.load(Ordering::Acquire)
    }
    /// Retain validated static triangles. Both Vec capacities are checked/accounted, not just length.
    pub fn register_mesh(
        &mut self,
        vertices: Vec<MeshVertex>,
        indices: Vec<u32>,
    ) -> Result<ResourceId, RenderError> {
        if vertices.len() < 3
            || vertices.capacity() > self.limits.max_vertices as usize
            || indices.is_empty()
            || !indices.len().is_multiple_of(3)
            || indices.capacity() > self.limits.max_indices as usize
            || indices.iter().any(|i| *i as usize >= vertices.len())
        {
            return Err(RenderError::InvalidLimits);
        }
        for triangle in indices.chunks_exact(3) {
            let a = vertices[triangle[0] as usize].position();
            let b = vertices[triangle[1] as usize].position();
            let c = vertices[triangle[2] as usize].position();
            let u: [f32; 3] = std::array::from_fn(|i| b[i] - a[i]);
            let v: [f32; 3] = std::array::from_fn(|i| c[i] - a[i]);
            let cross = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            if cross.iter().all(|v| *v == 0.0) {
                return Err(RenderError::InvalidLimits);
            }
        }
        let bytes = vertices.capacity() as u64 * std::mem::size_of::<MeshVertex>() as u64
            + indices.capacity() as u64 * 4;
        let lease = self.reserve(bytes)?;
        let sphere = bounds(&vertices);
        self.meshes.insert(
            Arc::new(MeshAsset {
                vertices,
                indices,
                bounds: sphere,
                _lease: lease,
            }),
            bytes,
        )
    }
    /// Register finite linear RGBA in 0..1; no implicit clamping or textures in this profile.
    pub fn register_material(&mut self, color: [f32; 4]) -> Result<ResourceId, RenderError> {
        if !color
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err(RenderError::InvalidLimits);
        }
        let bytes = 16;
        let lease = self.reserve(bytes)?;
        self.materials.insert(
            Arc::new(Material {
                color,
                _lease: lease,
            }),
            bytes,
        )
    }
    /// Remove registration; payload reservation remains until its last prepared owner drops.
    pub fn remove_mesh(&mut self, id: ResourceId) -> Result<(), RenderError> {
        self.meshes.remove(id)?;
        Ok(())
    }
    /// Remove material registration without invalidating already prepared frames.
    pub fn remove_material(&mut self, id: ResourceId) -> Result<(), RenderError> {
        self.materials.remove(id)?;
        Ok(())
    }
    /// Resolve every identity/selection before preparation. Static assets accept animation/morph 0 only.
    /// Consumes snapshot admission on success and releases it on controlled failure.
    pub fn prepare(&self, snapshot: SceneSnapshot) -> Result<PreparedScene, RenderError> {
        if snapshot.instances().len() > self.limits.max_instances as usize {
            return Err(RenderError::BudgetExceeded);
        }
        for instance in snapshot.instances() {
            self.meshes.get(instance.binding.mesh)?;
            self.materials.get(instance.binding.material)?;
            if instance.binding.animation != 0 || instance.binding.morph != 0 {
                return Err(RenderError::InvalidResource);
            }
            instance.binding.validate()?;
        }
        let lease = self.reserve(
            snapshot.instances().len() as u64 * std::mem::size_of::<PreparedInstance>() as u64,
        )?;
        let mut instances = reserved(snapshot.instances().len())?;
        for instance in snapshot.instances() {
            instances.push(PreparedInstance {
                entity: instance.entity,
                transform: instance.transform,
                mesh_id: instance.binding.mesh,
                mesh: self.meshes.get(instance.binding.mesh)?.clone(),
                material: self.materials.get(instance.binding.material)?.clone(),
                tint: instance.binding.tint,
            });
        }
        Ok(PreparedScene {
            instances,
            _snapshot: snapshot,
            _lease: lease,
        })
    }
}
