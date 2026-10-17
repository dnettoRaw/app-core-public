// =============================================================================
//        #######
//     ###       ###     F: asset_frame.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    asset_data::{world_bounds, AssetLease, Material},
    BoundingSphere, EntityId, MeshAsset, ResourceId, SceneSnapshot, Transform,
};
use std::sync::Arc;
/// Instance with resolved immutable asset ownership; no lookup or fallback is deferred to shaders.
pub struct PreparedInstance {
    pub(crate) entity: EntityId,
    pub(crate) transform: Transform,
    pub(crate) mesh_id: ResourceId,
    pub(crate) mesh: Arc<MeshAsset>,
    pub(crate) material: Arc<Material>,
    pub(crate) tint: [f32; 4],
}
impl PreparedInstance {
    /// Extracted stable entity identity.
    pub fn entity(&self) -> EntityId {
        self.entity
    }
    /// Validated local transform.
    pub fn transform(&self) -> Transform {
        self.transform
    }
    /// Mesh identity for batching; its immutable payload remains owned by this frame.
    pub fn mesh_id(&self) -> ResourceId {
        self.mesh_id
    }
    /// Shared mesh payload.
    pub fn mesh(&self) -> &MeshAsset {
        &self.mesh
    }
    /// Validated combined linear material/tint color.
    pub fn color(&self) -> [f32; 4] {
        std::array::from_fn(|i| self.material.color[i] * self.tint[i])
    }
    /// Conservative transformed sphere, including nonuniform scale and quaternion tolerance.
    pub fn world_bounds(&self) -> BoundingSphere {
        world_bounds(self.mesh.bounds(), self.transform)
    }
}
/// Owned resolved frame, retaining snapshot admission and CPU assets through registry retirement.
/// GPU upload/retirement remains a separate submission-completion boundary.
pub struct PreparedScene {
    pub(crate) instances: Vec<PreparedInstance>,
    pub(crate) _snapshot: SceneSnapshot,
    pub(crate) _lease: AssetLease,
}
impl PreparedScene {
    /// Resolved immutable instances; prepared payload capacity is accounted by the registry.
    pub fn instances(&self) -> &[PreparedInstance] {
        &self.instances
    }
}
