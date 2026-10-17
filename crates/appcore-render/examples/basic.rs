// =============================================================================
//        #######
//     ###       ###     F: basic.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::RenderLimits;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = RenderLimits::default().attachment_bytes(1280, 720)?;
    println!("managed_attachment_bytes={bytes}");
    Ok(())
}
