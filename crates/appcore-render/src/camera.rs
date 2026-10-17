// =============================================================================
//        #######
//     ###       ###     F: camera.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::RenderError;
/// Bounded orbit camera for the native Lorenz scene; angles are in radians.
#[derive(Debug, Clone, Copy)]
pub struct LorenzView {
    /// Rotation about world Y.
    pub yaw: f32,
    /// Rotation about world X, in -1.45..1.45.
    pub pitch: f32,
    /// Eye distance in 7..20 scene units.
    pub distance: f32,
    /// Selected trajectory, in 0..6.
    pub selected: u32,
    /// Draw only the selected trajectory.
    pub isolate: bool,
}
impl Default for LorenzView {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.55,
            distance: 12.0,
            selected: 0,
            isolate: false,
        }
    }
}
impl LorenzView {
    /// Validate camera values before GPU upload or compute submission.
    pub fn validate(self) -> Result<Self, RenderError> {
        if !self.yaw.is_finite()
            || !self.pitch.is_finite()
            || !self.distance.is_finite()
            || !(-1.45..=1.45).contains(&self.pitch)
            || !(7.0..=20.0).contains(&self.distance)
            || self.selected >= 6
        {
            return Err(RenderError::InvalidLimits);
        }
        Ok(self)
    }
    #[cfg(any(feature = "metal", test))]
    pub(crate) fn matrix(self, aspect: f32) -> Result<[f32; 16], RenderError> {
        self.validate()?;
        if !aspect.is_finite() || aspect <= 0.0 {
            return Err(RenderError::InvalidLimits);
        }
        let mut rx = identity();
        let (c, s) = (self.pitch.cos(), self.pitch.sin());
        rx[5] = c;
        rx[6] = s;
        rx[9] = -s;
        rx[10] = c;
        let mut ry = identity();
        let (c, s) = (self.yaw.cos(), self.yaw.sin());
        ry[0] = c;
        ry[2] = -s;
        ry[8] = s;
        ry[10] = c;
        let mut eye = identity();
        eye[14] = -self.distance;
        let f = 1.0 / (std::f32::consts::PI / 8.0).tan();
        let near = 0.1;
        let far = 100.0;
        let projection = [
            f / aspect,
            0.0,
            0.0,
            0.0,
            0.0,
            f,
            0.0,
            0.0,
            0.0,
            0.0,
            far / (near - far),
            -1.0,
            0.0,
            0.0,
            near * far / (near - far),
            0.0,
        ];
        Ok(multiply(projection, multiply(eye, multiply(rx, ry))))
    }
}
#[cfg(any(feature = "metal", test))]
fn identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
#[cfg(any(feature = "metal", test))]
fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            for k in 0..4 {
                out[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k];
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_rejects_invalid_values_and_uses_webgpu_depth() -> Result<(), RenderError> {
        let camera = LorenzView {
            yaw: 0.0,
            pitch: 0.0,
            ..Default::default()
        };
        let m = camera.matrix(4.0 / 3.0)?;
        let depth = m[14] / m[15];
        assert!((0.0..1.0).contains(&depth));
        assert!(m.iter().all(|v| v.is_finite()));
        assert!(LorenzView {
            distance: f32::NAN,
            ..camera
        }
        .validate()
        .is_err());
        assert!(LorenzView {
            selected: 6,
            ..camera
        }
        .validate()
        .is_err());
        assert!(camera.matrix(0.0).is_err());
        Ok(())
    }
}
