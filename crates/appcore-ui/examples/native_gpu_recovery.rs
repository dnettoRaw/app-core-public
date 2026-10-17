// =============================================================================
//        #######
//     ###       ###     F: native_gpu_recovery.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

//! Explicit opt-in destructive device qualification on isolated native windows.
#[cfg(all(feature = "gpu-qualification", target_os = "macos"))]
#[path = "native-gpu/qualification.rs"]
mod qualification;
#[cfg(all(feature = "gpu-qualification", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    qualification::run()
}
#[cfg(not(all(feature = "gpu-qualification", target_os = "macos")))]
fn main() {
    eprintln!("Select --features gpu-qualification on macOS");
}
