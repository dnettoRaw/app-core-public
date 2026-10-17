// =============================================================================
//        #######
//     ###       ###     F: lod.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{frustum::validate_sphere, BoundingSphere, Frustum, RenderError};
/// Packed triangle range and conservative model-space geometric error, finest first.
#[derive(Debug, Clone, Copy)]
pub struct LodLevel {
    /// Triangle indices, in 3..=196608 and divisible by three.
    pub index_count: u32,
    /// Offset in the caller-owned packed index buffer.
    pub first_index: u32,
    /// Nonnegative base vertex in the caller-owned packed vertex buffer.
    pub base_vertex: u32,
    /// Conservative deviation from finest geometry; finest must be zero.
    /// The asset producer owns this bound; the selector cannot infer it from metadata.
    pub geometric_error: f32,
    /// Whether the corresponding packed geometry is currently available. Coarsest stays resident.
    pub resident: bool,
}
/// Per-instance conservative bounds and maximum absolute transform scale.
#[derive(Debug, Clone, Copy)]
pub struct LodObject {
    /// Sphere enclosing every LOD, in the same coordinate system as LodView.eye.
    pub bounds: BoundingSphere,
    /// Maximum absolute scale, in 0.000001..=10000.
    pub max_scale: f32,
}
impl LodObject {
    pub(crate) fn validate(self) -> Result<(), RenderError> {
        validate_sphere(self.bounds)?;
        if !self.max_scale.is_finite() || !(0.000001..=10000.0).contains(&self.max_scale) {
            return Err(RenderError::InvalidLimits);
        }
        Ok(())
    }
}
/// Explicit centered-perspective error budget; caller supplies matching frustum and camera provenance.
#[derive(Debug, Clone, Copy)]
pub struct LodView {
    /// Camera position in the same coordinates as object bounds.
    pub eye: [f32; 3],
    /// Camera forward axis; validate normalizes a finite nonzero vector.
    pub forward: [f32; 3],
    /// Conservative physical viewport half-diagonal upper bound, in 0.5..=32768 pixels.
    pub viewport_half_diagonal: f32,
    /// Near-camera guard distance, in 0.0001..=1000000.
    pub near_distance: f32,
    /// Vertical focal length in physical pixels, in 0.5..=32768.
    pub focal_pixels: f32,
    /// Maximum projected deviation, in 0.125..=1024 physical pixels.
    pub maximum_error_pixels: f32,
}
impl LodView {
    /// Reject invalid numeric inputs before upload. Orthographic projection is not this profile.
    pub fn validate(mut self) -> Result<Self, RenderError> {
        if !self.eye.iter().all(|v| v.is_finite() && v.abs() <= 1e12)
            || !self.near_distance.is_finite()
            || !(0.0001..=1000000.0).contains(&self.near_distance)
            || !self.focal_pixels.is_finite()
            || !(0.5..=32768.0).contains(&self.focal_pixels)
            || !self.maximum_error_pixels.is_finite()
            || !(0.125..=1024.0).contains(&self.maximum_error_pixels)
        {
            return Err(RenderError::InvalidLimits);
        }
        if !self.forward.iter().all(|v| v.is_finite() && v.abs() <= 1e6)
            || !self.viewport_half_diagonal.is_finite()
            || !(0.5..=32768.0).contains(&self.viewport_half_diagonal)
        {
            return Err(RenderError::InvalidLimits);
        }
        let length = self
            .forward
            .iter()
            .map(|v| f64::from(*v).powi(2))
            .sum::<f64>()
            .sqrt();
        if length == 0.0 {
            return Err(RenderError::InvalidLimits);
        }
        self.forward = self.forward.map(|v| (f64::from(v) / length) as f32);
        Ok(self)
    }
}
/// Selected geometry and explicit degradation when the desired level is not resident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LodChoice {
    /// Index in the finest-to-coarsest level list.
    pub level: u32,
    /// True when residency forced a coarser level beyond the desired error selection.
    pub degraded: bool,
}
pub(crate) fn validate_levels(levels: &[LodLevel]) -> Result<(), RenderError> {
    if levels.is_empty()
        || levels.len() > 8
        || levels[0].geometric_error != 0.0
        || !levels.last().is_some_and(|l| l.resident)
    {
        return Err(RenderError::InvalidLimits);
    }
    let mut previous_error = 0.0;
    let mut previous_end = 0;
    for level in levels {
        if level.index_count == 0
            || level.index_count > 196608
            || !level.index_count.is_multiple_of(3)
            || level.first_index < previous_end
            || level.base_vertex > 524288
            || !level.geometric_error.is_finite()
            || !(previous_error..=1000000.0).contains(&level.geometric_error)
        {
            return Err(RenderError::InvalidLimits);
        }
        previous_end = level
            .first_index
            .checked_add(level.index_count)
            .filter(|end| *end <= 1572864)
            .ok_or(RenderError::InvalidLimits)?;
        previous_error = level.geometric_error;
    }
    Ok(())
}
/// f64 visibility/LOD reference using perspective screen-error bounds.
/// Bounds must enclose every level. Clipped spheres request finest. Missing geometry
/// chooses the next resident coarser level and reports degradation; culled returns None.
pub fn choose_lod(
    levels: &[LodLevel],
    object: LodObject,
    view: LodView,
    frustum: Frustum,
) -> Result<Option<LodChoice>, RenderError> {
    validate_levels(levels)?;
    object.validate()?;
    let view = view.validate()?;
    if !frustum.intersects(object.bounds)? {
        return Ok(None);
    }
    let clipped = frustum.planes().iter().any(|plane| {
        let distance = (0..3)
            .map(|i| f64::from(plane[i]) * f64::from(object.bounds.center[i]))
            .sum::<f64>()
            + f64::from(plane[3]);
        let magnitude = (0..3)
            .map(|i| (f64::from(plane[i]) * f64::from(object.bounds.center[i])).abs())
            .sum::<f64>()
            + f64::from(plane[3]).abs()
            + f64::from(object.bounds.radius)
            + 1.0;
        distance <= f64::from(object.bounds.radius) + magnitude * 0.00001
    });
    let delta = std::array::from_fn::<_, 3, _>(|i| {
        f64::from(object.bounds.center[i]) - f64::from(view.eye[i])
    });
    let magnitude = (0..3)
        .map(|i| (delta[i] * f64::from(view.forward[i])).abs())
        .sum::<f64>()
        + f64::from(object.bounds.radius)
        + 1.0;
    let depth = (0..3)
        .map(|i| delta[i] * f64::from(view.forward[i]))
        .sum::<f64>()
        - f64::from(object.bounds.radius)
        - magnitude * 0.00001;
    let mut desired = 0;
    if !clipped && depth > f64::from(view.near_distance) {
        for (index, level) in levels.iter().enumerate().skip(1) {
            let error = f64::from(level.geometric_error) * f64::from(object.max_scale);
            if depth - error <= f64::from(view.near_distance) {
                continue;
            }
            let projected = (f64::from(view.focal_pixels) + f64::from(view.viewport_half_diagonal))
                * error
                / (depth - error);
            if projected * (1.0 + 0.00001) <= f64::from(view.maximum_error_pixels) {
                desired = index;
            }
        }
    }
    let selected = (desired..levels.len())
        .find(|i| levels[*i].resident)
        .ok_or(RenderError::InvalidResource)?;
    Ok(Some(LodChoice {
        level: selected as u32,
        degraded: selected != desired,
    }))
}
