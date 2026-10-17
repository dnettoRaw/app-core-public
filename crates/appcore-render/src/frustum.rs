// =============================================================================
//        #######
//     ###       ###     F: frustum.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{BoundingSphere, RenderError};
/// Six normalized inward-facing planes using WebGPU's 0..1 clip-depth convention.
#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    planes: [[f32; 4]; 6],
}
impl Frustum {
    /// Extract from a finite column-major world-to-clip matrix, with explicit normalization.
    pub fn from_matrix(matrix: [f32; 16]) -> Result<Self, RenderError> {
        if !matrix.iter().all(|v| v.is_finite()) {
            return Err(RenderError::InvalidLimits);
        }
        let row = |r: usize| std::array::from_fn::<_, 4, _>(|c| f64::from(matrix[c * 4 + r]));
        let rows = [row(0), row(1), row(2), row(3)];
        let combine =
            |axis: usize, sign: f64| std::array::from_fn(|i| rows[3][i] + sign * rows[axis][i]);
        let raw = [
            combine(0, 1.0),
            combine(0, -1.0),
            combine(1, 1.0),
            combine(1, -1.0),
            rows[2],
            combine(2, -1.0),
        ];
        let mut planes = [[0.0; 4]; 6];
        for (index, plane) in raw.into_iter().enumerate() {
            let length = plane[..3].iter().map(|v| v * v).sum::<f64>().sqrt();
            if !length.is_finite() || length == 0.0 {
                return Err(RenderError::InvalidLimits);
            }
            for (i, value) in plane.into_iter().enumerate() {
                planes[index][i] = (value / length) as f32;
            }
            if !planes[index].iter().all(|v| v.is_finite()) {
                return Err(RenderError::InvalidLimits);
            }
        }
        Ok(Self { planes })
    }
    /// Normalized planes, ordered left/right/bottom/top/near/far.
    pub fn planes(&self) -> &[[f32; 4]; 6] {
        &self.planes
    }
    /// Conservative CPU reference, including a scale-aware rounding margin.
    pub fn intersects(&self, sphere: BoundingSphere) -> Result<bool, RenderError> {
        validate_sphere(sphere)?;
        Ok(self.planes.iter().all(|plane| {
            let distance = (0..3)
                .map(|i| f64::from(plane[i]) * f64::from(sphere.center[i]))
                .sum::<f64>()
                + f64::from(plane[3]);
            let magnitude = (0..3)
                .map(|i| (f64::from(plane[i]) * f64::from(sphere.center[i])).abs())
                .sum::<f64>()
                + f64::from(plane[3]).abs()
                + f64::from(sphere.radius)
                + 1.0;
            distance >= -f64::from(sphere.radius) - magnitude * 0.00001
        }))
    }
}
pub(crate) fn validate_sphere(sphere: BoundingSphere) -> Result<(), RenderError> {
    if !sphere
        .center
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 1e12)
        || !sphere.radius.is_finite()
        || !(0.0..=1e12).contains(&sphere.radius)
    {
        return Err(RenderError::InvalidLimits);
    }
    Ok(())
}
