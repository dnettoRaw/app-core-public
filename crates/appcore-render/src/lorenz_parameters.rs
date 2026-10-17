// =============================================================================
//        #######
//     ###       ###     F: lorenz_parameters.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::RenderError;
/// Bounded Lorenz coefficients for the fixed RK4 timestep 0.005.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LorenzParameters {
    /// Positive damping coefficient, in 1..=20.
    pub sigma: f32,
    /// Driving coefficient, in 1..=50.
    pub rho: f32,
    /// Positive vertical damping, in 0.5..=6.
    pub beta: f32,
}
impl Default for LorenzParameters {
    fn default() -> Self {
        Self {
            sigma: 10.0,
            rho: 28.0,
            beta: 8.0 / 3.0,
        }
    }
}
impl LorenzParameters {
    /// Reject nonfinite/out-of-profile values before changing state or uploading bytes.
    /// Bounds define this integration profile, not a universal stability guarantee.
    pub fn validate(self) -> Result<Self, RenderError> {
        if !self.sigma.is_finite()
            || !(1.0..=20.0).contains(&self.sigma)
            || !self.rho.is_finite()
            || !(1.0..=50.0).contains(&self.rho)
            || !self.beta.is_finite()
            || !(0.5..=6.0).contains(&self.beta)
        {
            return Err(RenderError::InvalidLimits);
        }
        Ok(self)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_nonfinite_and_out_of_profile_coefficients() {
        let default = LorenzParameters::default();
        assert_eq!(default.validate(), Ok(default));
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, 51.0] {
            assert!(LorenzParameters {
                sigma: bad,
                ..default
            }
            .validate()
            .is_err());
            assert!(LorenzParameters {
                rho: bad,
                ..default
            }
            .validate()
            .is_err());
            assert!(LorenzParameters {
                beta: bad,
                ..default
            }
            .validate()
            .is_err());
        }
    }
}
