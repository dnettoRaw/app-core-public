// =============================================================================
//        #######
//     ###       ###     F: coordinator.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Policy applied when a heartbeat cannot reach the control plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartbeatPolicy {
    /// Degrade instead of failing when the client reports offline status.
    pub allow_degraded_on_offline: bool,
}

impl Default for HeartbeatPolicy {
    fn default() -> Self {
        Self {
            allow_degraded_on_offline: true,
        }
    }
}

/// Coordinates heartbeat results with the runtime operational mode.
pub struct ControlPlaneCoordinator<C> {
    client: C,
    policy: HeartbeatPolicy,
}

impl<C> ControlPlaneCoordinator<C>
where
    C: ControlPlaneProvider,
{
    /// Creates a coordinator with an explicit offline policy.
    pub fn new(client: C, policy: HeartbeatPolicy) -> Self {
        Self { client, policy }
    }

    /// Sends one heartbeat and returns the resulting operational mode.
    pub async fn heartbeat_once(
        &self,
        request: HeartbeatRequest,
    ) -> ControlPlaneResult<RuntimeOperationalMode> {
        match self.client.heartbeat(request).await {
            Ok(response) => Ok(response.operation_mode),
            Err(ControlPlaneError::Offline) if self.policy.allow_degraded_on_offline => {
                Ok(RuntimeOperationalMode::Degraded)
            }
            Err(error) => Err(error),
        }
    }
}
