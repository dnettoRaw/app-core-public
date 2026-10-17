// =============================================================================
//        #######
//     ###       ###     F: scene_components.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{RenderError, ResourceId};
/// Validated local GPU-space transform, aligned for contiguous SIMD-friendly columns.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }
    }
}
impl Transform {
    /// Construct with a unit quaternion and positive finite scale, without implicit normalization.
    pub fn new(
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    ) -> Result<Self, RenderError> {
        let norm = rotation.iter().map(|v| v * v).sum::<f32>();
        if !position
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
            || !rotation.iter().all(|v| v.is_finite())
            || (norm - 1.0).abs() > 0.0001
            || !scale
                .iter()
                .all(|v| v.is_finite() && *v >= 0.000001 && *v <= 10_000.0)
        {
            return Err(RenderError::InvalidLimits);
        }
        Ok(Self {
            position,
            rotation,
            scale,
        })
    }
    /// Local translation; global f64 coordinates belong to world/chunk positioning.
    pub fn position(self) -> [f32; 3] {
        self.position
    }
    /// Unit quaternion in x,y,z,w order.
    pub fn rotation(self) -> [f32; 4] {
        self.rotation
    }
    /// Positive nonuniform scale.
    pub fn scale(self) -> [f32; 3] {
        self.scale
    }
    /// Validated replacement; rejection leaves the existing value unchanged.
    pub fn with_position(self, position: [f32; 3]) -> Result<Self, RenderError> {
        Self::new(position, self.rotation, self.scale)
    }
}
/// Validated bounded velocity component; no fluid/rigid-body solver is implied.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Velocity([f32; 3]);
impl Velocity {
    /// Reject non-finite or excessively large local velocities before storing them.
    pub fn new(value: [f32; 3]) -> Result<Self, RenderError> {
        if !value.iter().all(|v| v.is_finite() && v.abs() <= 100_000.0) {
            return Err(RenderError::InvalidLimits);
        }
        Ok(Self(value))
    }
    /// Local units per second.
    pub fn value(self) -> [f32; 3] {
        self.0
    }
}
/// Application-selected generic asset identities and small instance appearance data.
/// Asset lifetime/type validation belongs to the renderer's asset registry before upload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderBinding {
    /// Mesh identity; never a pointer into a mutable resource arena.
    pub mesh: ResourceId,
    /// Material identity, resolved separately from mesh identity.
    pub material: ResourceId,
    /// Finite linear RGBA multipliers in 0..1.
    pub tint: [f32; 4],
    /// Animation selection; the asset owner validates supported clips.
    pub animation: u32,
    /// Morph selection; the asset owner validates supported targets.
    pub morph: u32,
}
impl RenderBinding {
    /// Validate numeric instance data without resolving or assuming asset availability.
    pub fn validate(self) -> Result<Self, RenderError> {
        if !self
            .tint
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err(RenderError::InvalidLimits);
        }
        Ok(self)
    }
}
