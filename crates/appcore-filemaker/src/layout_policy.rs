// =============================================================================
//        #######
//     ###       ###     F: layout_policy.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{
    CollisionBounds, CollisionPolicy, CollisionResolution, DocumentIr, ElementIr, Result, Transform,
};

pub(crate) fn effective_collision_policy(
    element: &ElementIr,
    document: &DocumentIr,
    inherited: &CollisionPolicy,
) -> CollisionPolicy {
    let region = element
        .geometry
        .region
        .as_ref()
        .and_then(|name| document.regions.get(name))
        .and_then(|region| region.collision.as_ref())
        .unwrap_or(inherited);
    element.collision.as_ref().unwrap_or(region).clone()
}

pub(crate) fn validate_shrink_policy(policy: &CollisionPolicy) -> Result<()> {
    if policy.resolution == CollisionResolution::Shrink && policy.bounds != CollisionBounds::Layout
    {
        return Err(crate::layout_geometry::layout_error(
            "collision shrink requires layout bounds so size changes remain explicit",
        ));
    }
    Ok(())
}

pub(crate) fn validate_shrink_transform(
    policy: &CollisionPolicy,
    transform: Transform,
) -> Result<()> {
    if policy.resolution == CollisionResolution::Shrink
        && (transform.a != 1_000_000
            || transform.b != 0
            || transform.c != 0
            || transform.d != 1_000_000)
    {
        return Err(crate::layout_geometry::layout_error(
            "collision shrink cannot be combined with rotation, scale, flip, or mirror",
        ));
    }
    Ok(())
}
