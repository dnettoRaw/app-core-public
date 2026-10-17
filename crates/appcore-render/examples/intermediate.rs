// =============================================================================
//        #######
//     ###       ###     F: intermediate.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::{RenderLimits, ResourceArena};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arena = ResourceArena::new(RenderLimits::default())?;
    let id = arena.insert("cpu-owned example resource", 64)?;
    println!("managed_bytes={}", arena.managed_bytes());
    let _resource = arena.remove(id)?;
    assert!(arena.get(id).is_err());
    Ok(())
}
