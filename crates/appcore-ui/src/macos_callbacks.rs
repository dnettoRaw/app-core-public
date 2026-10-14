// =============================================================================
//        #######
//     ###       ###     F: macos_callbacks.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Main-thread Objective-C callbacks with bounded, nonblocking activation delivery.
use crate::macos_window::UiNativeActivation;
use objc2::{define_class, msg_send, rc::Retained, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSControl, NSWindowDelegate};
use objc2_foundation::{MainThreadMarker, NSNotification, NSObject, NSObjectProtocol};
use std::{cell::Cell, rc::Rc, sync::mpsc::SyncSender};
pub(crate) struct Callbacks {
    pub(crate) element: String,
    pub(crate) action: String,
    pub(crate) sender: SyncSender<UiNativeActivation>,
    pub(crate) dropped: Rc<Cell<u64>>,
    pub(crate) queued: Rc<Cell<usize>>,
    pub(crate) failures: Rc<Cell<u64>>,
    pub(crate) closed: Rc<Cell<bool>>,
    pub(crate) on_close: Rc<dyn Fn()>,
}
define_class!(
    // SAFETY: NSObject has no additional subclassing requirements; this class has no Drop.
    #[unsafe(super = NSObject)]
    #[name = "AppCoreUiNativeCallbacksV1"]
    #[thread_kind = MainThreadOnly]
    #[ivars = Callbacks]
    pub(crate) struct UiCallbacks;
    // SAFETY: NSObjectProtocol has no additional safety requirements.
    unsafe impl NSObjectProtocol for UiCallbacks {}
    impl UiCallbacks {
        // SAFETY: The target selector uses the documented NSControl sender signature.
        #[unsafe(method(activate:))]
        fn activate(&self, _: &NSControl) {
            let state = self.ivars();
            if state.closed.get() { return; }
            let event = UiNativeActivation { element_id:state.element.clone(), action_id:state.action.clone() };
            if state.sender.try_send(event).is_err() {
                state.dropped.set(state.dropped.get().saturating_add(1));
            } else { state.queued.set(state.queued.get()+1); }
        }
    }
    // SAFETY: NSWindowDelegate has no additional safety requirements.
    unsafe impl NSWindowDelegate for UiCallbacks {
        #[unsafe(method(windowWillClose:))]
        fn will_close(&self, _: &NSNotification) {
            let state = self.ivars();
            if !state.closed.replace(true) {
                // Foreign callbacks must never unwind into Objective-C.
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (state.on_close)())).is_err() {
                    state.failures.set(state.failures.get().saturating_add(1));
                }
            }
        }
    }
);
impl UiCallbacks {
    pub(crate) fn new(mtm: MainThreadMarker, callbacks: Callbacks) -> Retained<Self> {
        let object = Self::alloc(mtm).set_ivars(callbacks);
        // SAFETY: NSObject init initializes the allocated superclass with the standard signature.
        unsafe { msg_send![super(object), init] }
    }
}
