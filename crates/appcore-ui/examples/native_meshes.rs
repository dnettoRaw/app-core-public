// =============================================================================
//        #######
//     ###       ###     F: native_meshes.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

//! Native multi-mesh GPU scene; composition owns input, camera and shutdown.
#[cfg(all(feature = "gpu-macos", target_os = "macos"))]
#[path = "native-gpu/meshes.rs"]
mod meshes;
#[cfg(all(feature = "gpu-macos", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    meshes::run()
}
#[cfg(not(all(feature = "gpu-macos", target_os = "macos")))]
fn main() {
    eprintln!("Select --features gpu-macos on macOS");
}
