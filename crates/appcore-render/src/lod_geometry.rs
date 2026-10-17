// =============================================================================
//        #######
//     ###       ###     F: lod_geometry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    asset_data::MeshAsset, lod::validate_levels, BoundingSphere, LodLevel, PreparedInstance,
    RenderError,
};
use std::sync::Arc;
/// Immutable resolved mesh for a finest-to-coarsest packed geometry level.
pub struct MeshLodDescriptor<'a> {
    /// Resolved static opaque asset; finest identity is used for per-frame instance bindings.
    pub seed: &'a PreparedInstance,
    /// Conservative model-space deviation from finest; producer owns this guarantee.
    pub geometric_error: f32,
}
pub(crate) struct GeometryPlan {
    pub levels: Vec<LodLevel>,
    pub bounds: BoundingSphere,
    pub bytes: u64,
    pub vertex_bytes: u64,
    pub index_bytes: u64,
}
pub(crate) struct PackedGeometry {
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
    pub owners: Vec<Arc<MeshAsset>>,
}
pub(crate) fn plan(descriptors: &[MeshLodDescriptor<'_>]) -> Result<GeometryPlan, RenderError> {
    if descriptors.is_empty() || descriptors.len() > 8 {
        return Err(RenderError::InvalidLimits);
    }
    let mut levels = Vec::new();
    levels
        .try_reserve_exact(descriptors.len())
        .map_err(|_| RenderError::CpuAllocationFailed)?;
    let (mut vertices, mut indices, mut radius) = (0u32, 0u32, 0f64);
    for descriptor in descriptors {
        if descriptor.seed.color()[3] != 1. {
            return Err(RenderError::InvalidResource);
        }
        let mesh = descriptor.seed.mesh();
        levels.push(LodLevel {
            index_count: mesh.indices().len() as u32,
            first_index: indices,
            base_vertex: vertices,
            geometric_error: descriptor.geometric_error,
            resident: true,
        });
        vertices += mesh.vertices().len() as u32;
        indices += mesh.indices().len() as u32;
        for vertex in mesh.vertices() {
            radius = radius.max(
                vertex
                    .position()
                    .into_iter()
                    .map(|v| f64::from(v).powi(2))
                    .sum::<f64>()
                    .sqrt(),
            );
        }
    }
    validate_levels(&levels)?;
    Ok(GeometryPlan {
        levels,
        bounds: BoundingSphere {
            center: [0.; 3],
            radius: (radius * 1.0001) as f32,
        },
        bytes: u64::from(vertices) * 24 + u64::from(indices) * 4,
        vertex_bytes: u64::from(vertices) * 24,
        index_bytes: u64::from(indices) * 4,
    })
}
pub(crate) fn pack(descriptors: &[MeshLodDescriptor<'_>]) -> Result<PackedGeometry, RenderError> {
    let vertex_bytes = descriptors
        .iter()
        .map(|d| d.seed.mesh().vertices().len() * 24)
        .sum();
    let index_bytes = descriptors
        .iter()
        .map(|d| d.seed.mesh().indices().len() * 4)
        .sum();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut owners = Vec::new();
    vertices
        .try_reserve_exact(vertex_bytes)
        .map_err(|_| RenderError::CpuAllocationFailed)?;
    indices
        .try_reserve_exact(index_bytes)
        .map_err(|_| RenderError::CpuAllocationFailed)?;
    owners
        .try_reserve_exact(descriptors.len())
        .map_err(|_| RenderError::CpuAllocationFailed)?;
    for descriptor in descriptors {
        for vertex in descriptor.seed.mesh().vertices() {
            for value in vertex.position().into_iter().chain(vertex.normal()) {
                vertices.extend_from_slice(&value.to_le_bytes());
            }
        }
        for index in descriptor.seed.mesh().indices() {
            indices.extend_from_slice(&index.to_le_bytes());
        }
        owners.push(descriptor.seed.mesh.clone());
    }
    Ok(PackedGeometry {
        vertices,
        indices,
        owners,
    })
}
