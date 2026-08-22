// =============================================================================
//        #######
//     ###       ###     F: resource_platform.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(crate) use crate::resource_fallback::FallbackHardwareProbe as PlatformHardwareProbe;
#[cfg(target_os = "linux")]
pub(crate) use crate::resource_linux::LinuxHardwareProbe as PlatformHardwareProbe;
#[cfg(target_os = "macos")]
pub(crate) use crate::resource_macos::MacOsHardwareProbe as PlatformHardwareProbe;
#[cfg(windows)]
pub(crate) use crate::resource_windows::WindowsHardwareProbe as PlatformHardwareProbe;
