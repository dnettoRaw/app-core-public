// =============================================================================
//        #######
//     ###       ###     F: macos_web_view.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Web view activation policy: a user click activates the window and reaches the document once.
use objc2::{define_class, msg_send, rc::Retained, MainThreadOnly};
use objc2_app_kit::NSEvent;
use objc2_foundation::{MainThreadMarker, NSRect};
use objc2_web_kit::{WKWebView, WKWebViewConfiguration};
define_class!(
    // SAFETY: Only the documented NSView first-mouse hook is overridden; engine and delegates retain their behavior.
    #[unsafe(super = WKWebView)]
    #[name = "AppCoreUiWebViewV1"]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    pub(crate) struct WebView;
    impl WebView {
        // SAFETY: This is the documented nullable NSEvent signature, returning Objective-C BOOL.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _: Option<&NSEvent>) -> bool { true }
    }
);
impl WebView {
    pub(crate) fn create(
        mtm: MainThreadMarker,
        frame: NSRect,
        configuration: &WKWebViewConfiguration,
    ) -> Retained<WKWebView> {
        let object = Self::alloc(mtm).set_ivars(());
        // SAFETY: Main-thread allocation and validated viewport/configuration, with standard superclass initialization.
        let object: Retained<Self> =
            unsafe { msg_send![super(object),initWithFrame:frame,configuration:configuration] };
        object.into_super()
    }
}
