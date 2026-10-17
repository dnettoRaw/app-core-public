// =============================================================================
//        #######
//     ###       ###     F: admission.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::{RenderError, RenderLimits, ResourceArena};
#[test]
fn attachment_admission_bounds_dimensions_and_memory() {
    let limits = RenderLimits::default();
    assert_eq!(limits.attachment_bytes(1280, 720), Ok(7_372_800));
    assert_eq!(
        limits.attachment_bytes(0, 1),
        Err(RenderError::InvalidLimits)
    );
    assert_eq!(
        limits.attachment_bytes(4097, 1),
        Err(RenderError::InvalidLimits)
    );
    assert_eq!(
        RenderLimits {
            managed_bytes: 7,
            ..limits
        }
        .attachment_bytes(1, 1),
        Err(RenderError::BudgetExceeded)
    );
}
#[test]
fn stale_foreign_and_duplicate_handles_never_resolve() -> Result<(), RenderError> {
    let limits = RenderLimits {
        resource_slots: 1,
        managed_bytes: 8,
        ..RenderLimits::default()
    };
    let mut a = ResourceArena::new(limits)?;
    let mut b = ResourceArena::new(limits)?;
    let old = a.insert("old", 8)?;
    assert_eq!(a.insert("excess", 1), Err(RenderError::BudgetExceeded));
    assert_eq!(a.managed_bytes(), 8);
    let foreign = b.insert("foreign", 8)?;
    assert_eq!(a.get(foreign), Err(RenderError::InvalidResource));
    assert_eq!(a.remove(old)?, "old");
    let new = a.insert("new", 8)?;
    assert_ne!(old, new);
    assert_eq!(a.get(old), Err(RenderError::InvalidResource));
    assert_eq!(a.remove(old), Err(RenderError::InvalidResource));
    assert_eq!(a.get(new)?, &"new");
    Ok(())
}
