// =============================================================================
//        #######
//     ###       ###     F: gpu_scene_profile.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

//! Explicit blocking diagnostic profile; never run inside an interactive pump.
#[cfg(all(feature = "metal", target_os = "macos"))]
#[path = "profile/scene.rs"]
mod profile;
#[cfg(all(feature = "metal", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    profile::run()
}
#[cfg(not(all(feature = "metal", target_os = "macos")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("GPU profile unavailable: select --features metal on macOS".into())
}
