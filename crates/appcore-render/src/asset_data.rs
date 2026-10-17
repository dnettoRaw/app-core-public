// =============================================================================
//        #######
//     ###       ###     F: asset_data.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{RenderError, Transform};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
pub(crate) struct AssetLease {
    pub(crate) counter: Arc<AtomicU64>,
    pub(crate) bytes: u64,
}
impl Drop for AssetLease {
    fn drop(&mut self) {
        self.counter.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
/// Validated static mesh vertex, with finite bounded position and unit normal.
#[derive(Debug, Clone, Copy)]
pub struct MeshVertex {
    position: [f32; 3],
    normal: [f32; 3],
}
impl MeshVertex {
    /// Reject invalid normals rather than substituting or normalizing them implicitly.
    pub fn new(position: [f32; 3], normal: [f32; 3]) -> Result<Self, RenderError> {
        if !position
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
            || !normal.iter().all(|v| v.is_finite())
            || (normal.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() > 0.0001
        {
            return Err(RenderError::InvalidLimits);
        }
        Ok(Self { position, normal })
    }
    /// Local vertex position.
    pub fn position(self) -> [f32; 3] {
        self.position
    }
    /// Unit local normal.
    pub fn normal(self) -> [f32; 3] {
        self.normal
    }
}
/// Conservative local sphere for visibility and spatial indexing.
#[derive(Debug, Clone, Copy)]
pub struct BoundingSphere {
    /// Local/world center depending on the owning prepared instance.
    pub center: [f32; 3],
    /// Nonnegative radius.
    pub radius: f32,
}
/// Immutable static triangle mesh. Cloned Arc ownership keeps retired payloads accounted.
pub struct MeshAsset {
    pub(crate) vertices: Vec<MeshVertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) bounds: BoundingSphere,
    pub(crate) _lease: AssetLease,
}
impl MeshAsset {
    /// Validated immutable vertices.
    pub fn vertices(&self) -> &[MeshVertex] {
        &self.vertices
    }
    /// Triangle indices, all within vertices().
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    /// Conservative local sphere.
    pub fn bounds(&self) -> BoundingSphere {
        self.bounds
    }
}
pub(crate) struct Material {
    pub(crate) color: [f32; 4],
    pub(crate) _lease: AssetLease,
}
pub(crate) fn bounds(vertices: &[MeshVertex]) -> BoundingSphere {
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for vertex in vertices {
        for i in 0..3 {
            low[i] = low[i].min(vertex.position[i]);
            high[i] = high[i].max(vertex.position[i]);
        }
    }
    let center = std::array::from_fn(|i| (low[i] + high[i]) * 0.5);
    let radius = vertices
        .iter()
        .map(|v| {
            (0..3)
                .map(|i| (v.position[i] - center[i]).powi(2))
                .sum::<f32>()
                .sqrt()
        })
        .fold(0.0, f32::max);
    BoundingSphere {
        center,
        radius: radius * (1.0 + 0.0001),
    }
}
pub(crate) fn world_bounds(local: BoundingSphere, transform: Transform) -> BoundingSphere {
    let scaled: [f32; 3] = std::array::from_fn(|i| local.center[i] * transform.scale()[i]);
    let q = transform.rotation();
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let u = [q[0], q[1], q[2]];
    let v = cross(u, scaled);
    let w = cross(u, v);
    let center =
        std::array::from_fn(|i| scaled[i] + 2.0 * (q[3] * v[i] + w[i]) + transform.position()[i]);
    let radius = local.radius * transform.scale().into_iter().fold(0.0, f32::max) * (1.0 + 0.0002);
    BoundingSphere { center, radius }
}
