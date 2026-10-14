// =============================================================================
//        #######
//     ###       ###     F: macos_web_bridge.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Bounded native Promise replies, with caller identity obtained from the owning view.
use crate::{decode_ui_request_v1, UiActionV1, UiCaller, UiError, UiResult, UiSurface};
use block2::{DynBlock, RcBlock};
use objc2::{
    define_class, msg_send, rc::Retained, runtime::AnyObject, DefinedClass, MainThreadOnly,
};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSString};
use objc2_web_kit::{WKScriptMessage, WKScriptMessageHandlerWithReply, WKUserContentController};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Reply = RcBlock<dyn Fn(*mut AnyObject, *mut NSString)>;
type Work = Pin<Box<dyn Future<Output = UiResult<String>>>>;
struct Pending {
    work: Work,
    reply: Reply,
    bytes: usize,
    control: bool,
}
pub(crate) struct BridgeState {
    pub(crate) caller: UiCaller,
    pub(crate) document: String,
    pub(crate) surface: RefCell<Option<UiSurface>>,
    pub(crate) closed: Rc<Cell<bool>>,
    pending: RefCell<Vec<Pending>>,
    polling: Cell<bool>,
}
impl BridgeState {
    pub(crate) fn new(caller: UiCaller, document: String, closed: Rc<Cell<bool>>) -> Self {
        Self {
            caller,
            document,
            surface: RefCell::new(None),
            closed,
            pending: RefCell::new(vec![]),
            polling: Cell::new(false),
        }
    }
    fn accept(
        &self,
        message: &WKScriptMessage,
        reply: &DynBlock<dyn Fn(*mut AnyObject, *mut NSString)>,
    ) -> UiResult<()> {
        if self.polling.get() {
            return Err(UiError::Busy);
        }
        if self.closed.get() {
            return Err(UiError::Closed);
        }
        let surface = self.surface.borrow().clone().ok_or(UiError::Unavailable)?;
        // SAFETY: Frame information and body are framework-owned for this synchronous callback.
        let frame = unsafe { message.frameInfo() };
        let url = unsafe { frame.request() }.URL().ok_or(UiError::Forbidden)?;
        if !unsafe { frame.isMainFrame() }
            || url
                .absoluteString()
                .is_none_or(|url| url.to_string() != self.document)
        {
            return Err(UiError::Forbidden);
        }
        let body = unsafe { message.body() };
        let text = body
            .downcast_ref::<NSString>()
            .ok_or(UiError::InvalidFrame)?;
        let limits = surface.limits();
        if text.length() > limits.max_frame_bytes {
            return Err(UiError::PayloadTooLarge);
        }
        let raw = text.to_string();
        let frame = decode_ui_request_v1(raw.as_bytes(), limits)?;
        if frame.surface_id != self.caller.surface_id() {
            return Err(UiError::Forbidden);
        }
        let control = matches!(frame.action, UiActionV1::Cancel { .. } | UiActionV1::Close);
        let pending = self.pending.borrow();
        let count = pending
            .iter()
            .filter(|entry| entry.control == control)
            .count();
        if count >= if control { 2 } else { limits.max_inflight } {
            return Err(UiError::Busy);
        }
        if pending.iter().map(|entry| entry.bytes).sum::<usize>() + raw.len() > 16 * 1024 * 1024 {
            return Err(UiError::PayloadTooLarge);
        }
        drop(pending);
        // SAFETY: The framework passes a live block. Copy retains it until exactly one reply.
        let reply = unsafe { RcBlock::copy(std::ptr::from_ref(reply).cast_mut()) }
            .ok_or(UiError::Unavailable)?;
        let bytes = raw.len();
        let caller = self.caller.clone();
        let work = Box::pin(async move { surface.dispatch_json(&caller, raw.as_bytes()).await });
        self.pending.borrow_mut().push(Pending {
            work,
            reply,
            bytes,
            control,
        });
        Ok(())
    }
    pub(crate) fn pump(&self) {
        if self.closed.get() {
            self.close();
            return;
        }
        self.polling.set(true);
        // Poll once per deployment tick; no event-loop task or executor is hidden here.
        let mut pending = std::mem::take(&mut *self.pending.borrow_mut());
        let mut context = Context::from_waker(Waker::noop());
        let mut waiting = vec![];
        for mut entry in pending.drain(..) {
            match entry.work.as_mut().poll(&mut context) {
                Poll::Ready(result) => answer(&entry.reply, result),
                Poll::Pending => waiting.push(entry),
            }
        }
        self.pending.borrow_mut().extend(waiting);
        self.polling.set(false);
    }
    pub(crate) fn close(&self) {
        if let Some(surface) = self.surface.borrow_mut().take() {
            surface.close();
        }
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        for entry in pending {
            answer(&entry.reply, Err(UiError::Closed));
        }
    }
    pub(crate) fn pending(&self) -> usize {
        self.pending.borrow().len()
    }
}
fn answer(reply: &DynBlock<dyn Fn(*mut AnyObject, *mut NSString)>, result: UiResult<String>) {
    match result {
        Ok(raw) => {
            let text = NSString::from_str(&raw);
            reply.call((
                std::ptr::from_ref::<NSString>(&text).cast_mut().cast(),
                std::ptr::null_mut(),
            ));
        }
        Err(code) => {
            let code = serde_json::to_value(code)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| "backend_failure".into());
            let text = NSString::from_str(&code);
            reply.call((
                std::ptr::null_mut(),
                std::ptr::from_ref::<NSString>(&text).cast_mut(),
            ));
        }
    }
}
define_class!(
    // SAFETY: NSObject imposes no subclassing requirements and this class has no Drop.
    #[unsafe(super = NSObject)]
    #[name = "AppCoreUiWebMessageBridgeV1"]
    #[thread_kind = MainThreadOnly]
    #[ivars = BridgeState]
    pub(crate) struct MessageBridge;
    // SAFETY: NSObjectProtocol has no additional safety requirements.
    unsafe impl NSObjectProtocol for MessageBridge {}
    // SAFETY: Only retained Promise blocks are stored, and each is answered at most once.
    unsafe impl WKScriptMessageHandlerWithReply for MessageBridge {
        #[unsafe(method(userContentController:didReceiveScriptMessage:replyHandler:))]
        fn receive(
            &self,
            _: &WKUserContentController,
            message: &WKScriptMessage,
            reply: &DynBlock<dyn Fn(*mut AnyObject, *mut NSString)>,
        ) {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.ivars().accept(message, reply)
            }))
            .unwrap_or(Err(UiError::BackendFailure));
            if let Err(error) = result {
                answer(reply, Err(error));
            }
        }
    }
);
impl MessageBridge {
    pub(crate) fn new(mtm: MainThreadMarker, state: BridgeState) -> Retained<Self> {
        let object = Self::alloc(mtm).set_ivars(state);
        // SAFETY: Standard NSObject initialization of its allocated superclass.
        unsafe { msg_send![super(object), init] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_native_promises_are_rejected_once_and_released_on_close() {
        objc2::rc::autoreleasepool(|_| {
            let closed = Rc::new(Cell::new(false));
            let state = BridgeState::new(
                UiCaller::new("surface-a", "appcore-ui://surface-a").unwrap(),
                "appcore-ui://surface-a/index.html".into(),
                closed,
            );
            let replies = Rc::new(Cell::new(0));
            let called = replies.clone();
            let reply = RcBlock::new(move |value: *mut AnyObject, error: *mut NSString| {
                assert!(value.is_null());
                assert!(!error.is_null());
                // SAFETY: answer owns this NSString for the duration of the callback.
                assert_eq!(unsafe { &*error }.to_string(), "closed");
                called.set(called.get() + 1);
            });
            state.pending.borrow_mut().push(Pending {
                work: Box::pin(std::future::pending()),
                reply,
                bytes: 10,
                control: false,
            });
            state.close();
            state.close();
            assert_eq!(state.pending(), 0);
            assert_eq!(replies.get(), 1);
        });
    }
    #[test]
    fn completed_native_reply_is_delivered_without_a_second_callback() {
        objc2::rc::autoreleasepool(|_| {
            let state = BridgeState::new(
                UiCaller::new("surface-a", "appcore-ui://surface-a").unwrap(),
                "appcore-ui://surface-a/index.html".into(),
                Rc::new(Cell::new(false)),
            );
            let replies = Rc::new(Cell::new(0));
            let called = replies.clone();
            let reply = RcBlock::new(move |value: *mut AnyObject, error: *mut NSString| {
                assert!(error.is_null());
                assert!(!value.is_null());
                // SAFETY: answer owns a string object for the synchronous callback.
                let string = unsafe { &*value }.downcast_ref::<NSString>().unwrap();
                assert_eq!(string.to_string(), "bounded-reply");
                called.set(called.get() + 1);
            });
            state.pending.borrow_mut().push(Pending {
                work: Box::pin(std::future::ready(Ok("bounded-reply".into()))),
                reply,
                bytes: 10,
                control: false,
            });
            state.pump();
            state.pump();
            state.close();
            assert_eq!(replies.get(), 1);
            assert_eq!(state.pending(), 0);
        });
    }
}
