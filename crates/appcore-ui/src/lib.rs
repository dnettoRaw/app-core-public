// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Versioned, bounded UI boundaries; deployment owns hosting and authorization.
#![deny(missing_docs)]
#![deny(unsafe_code)]
mod error;
mod json_budget;
mod limits;
mod protocol;
mod validation;
/// Existing Runtime API request contracts, unchanged by the UI bridge.
pub use appcore_api::{CommandRequest, QueryRequest};
pub use error::{UiError, UiResult};
pub use limits::UiLimits;
pub use protocol::{UiActionV1, UiConnectionState, UiRequestV1, UI_PROTOCOL_V1};
pub use validation::decode_ui_request_v1;

mod backend;
mod reply;
mod subscriptions;
mod surface;
mod surface_state;
/// Existing capability contracts and policy owner reused by the bridge.
pub use appcore_capabilities as capabilities;
/// Existing versioned manifest identity types.
pub use appcore_contracts as contracts;
/// Existing foundational Runtime identity and capability types.
pub use appcore_core as core;
pub use backend::{UiBackend, UiBackendFuture, UiCaller, UiCancellation, UiGrant, UiInvocationV1};
pub use reply::{UiEventV1, UiOutcomeV1, UiReplyV1};
pub use subscriptions::UiPublishReport;
pub use surface::{UiSurface, UiSurfaceSnapshot, UiTopicBinding};

mod runtime_dispatch;
pub use runtime_dispatch::UiRuntimeDispatch;

/// Function-based element construction, independent of platform rendering.
pub mod elements;
mod native_page;
pub use native_page::{
    UiNativeNodeV1, UiNativePage, UiNativePageV1, UiNativeValue, UiRectangle, UI_NATIVE_PAGE_V1,
};

#[cfg(all(feature = "native-macos", target_os = "macos"))]
#[allow(unsafe_code)]
mod macos_callbacks;
#[cfg(all(feature = "native-macos", target_os = "macos"))]
#[allow(unsafe_code)]
mod macos_window;
#[cfg(all(feature = "native-macos", target_os = "macos"))]
pub use macos_window::{UiNativeActivation, UiNativeApplication, UiNativeWindow};

mod web_bundle;
pub use web_bundle::{UiWebAsset, UiWebBundle, UiWebMedia, UiWebResource};

#[cfg(all(feature = "web-macos", target_os = "macos"))]
#[allow(unsafe_code)]
mod macos_web_handler;
#[cfg(all(feature = "web-macos", target_os = "macos"))]
#[allow(unsafe_code)]
mod macos_web_window;
#[cfg(all(feature = "web-macos", target_os = "macos"))]
pub use macos_web_window::UiWebWindow;

#[cfg(all(feature = "web-macos", target_os = "macos"))]
#[allow(unsafe_code)]
mod macos_web_bridge;

#[cfg(all(feature = "native-macos", target_os = "macos"))]
#[allow(unsafe_code)]
mod macos_controls;

#[cfg(any(test, all(feature = "native-macos", target_os = "macos")))]
mod event_pump;

#[cfg(all(feature = "web-macos", target_os = "macos"))]
#[allow(unsafe_code)]
mod macos_web_view;

#[cfg(all(feature = "native-macos", target_os = "macos"))]
mod macos_timing;
#[cfg(all(feature = "native-macos", target_os = "macos"))]
pub use macos_timing::{UiNativeInputTiming, UiNativeTimedActivation};

#[cfg(all(target_os = "macos", feature = "gpu-macos"))]
mod macos_scene_input;
#[cfg(all(target_os = "macos", feature = "gpu-macos"))]
pub use macos_scene_input::UiNativeSceneInput;

#[cfg(all(target_os = "macos", feature = "gpu-macos"))]
#[allow(unsafe_code)]
mod macos_memory;
#[cfg(all(target_os = "macos", feature = "gpu-macos"))]
pub use macos_memory::native_process_resident_bytes;
