// =============================================================================
//        #######
//     ###       ###     F: native_lod.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

//! Native GPU LOD, current-frame Hi-Z and explicit logical availability controls.
#[cfg(all(feature = "gpu-macos", target_os = "macos"))]
#[path = "native-gpu/lod.rs"]
mod lod;
#[cfg(all(feature = "gpu-macos", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    lod::run()
}
#[cfg(not(all(feature = "gpu-macos", target_os = "macos")))]
fn main() {
    eprintln!("Select --features gpu-macos on macOS");
}
