// =============================================================================
//        #######
//     ###       ###     F: macos_web_handler.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Local-only system web resource handling and navigation rejection.
use crate::{UiWebBundle, UiWebMedia};
use objc2::{
    define_class, msg_send, rc::Retained, runtime::ProtocolObject, AnyThread, DefinedClass,
    MainThreadOnly,
};
use objc2_foundation::{
    MainThreadMarker, NSData, NSDictionary, NSError, NSHTTPURLResponse, NSObject, NSObjectProtocol,
    NSString, NSURL,
};
use objc2_web_kit::{
    WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate,
    WKURLSchemeHandler, WKURLSchemeTask, WKWebView,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};
pub(crate) struct WebState {
    pub(crate) bundle: Arc<UiWebBundle>,
    pub(crate) host: String,
    pub(crate) closed: Rc<Cell<bool>>,
    pub(crate) loaded: Cell<bool>,
    pub(crate) bridge_locked: Cell<bool>,
    pub(crate) document_started: Cell<bool>,
    pub(crate) process_failed: Cell<bool>,
    pub(crate) renderer_failures: Cell<u64>,
    pub(crate) failures: Cell<u64>,
    pub(crate) tasks: RefCell<HashMap<usize, Rc<Cell<bool>>>>,
}
impl WebState {
    fn path(&self, url: &NSURL) -> Option<String> {
        let absolute = url.absoluteString()?;
        if self.closed.get() || absolute.length() > 1024 {
            return None;
        }
        if url.scheme()?.to_string() != "appcore-ui"
            || url.host()?.to_string() != self.host
            || url.user().is_some()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || absolute.to_string().contains('%')
        {
            return None;
        }
        let path = url.path()?.to_string();
        let path = path.strip_prefix('/')?;
        self.bundle.resource(path).ok()?;
        Some(path.to_owned())
    }
    fn document(&self, url: &NSURL) -> bool {
        self.path(url)
            .and_then(|path| self.bundle.resource(&path).ok())
            .is_some_and(|resource| resource.media == UiWebMedia::Html)
    }
    pub(crate) fn renderer_failed(&self) {
        if !self.process_failed.replace(true) {
            self.renderer_failures
                .set(self.renderer_failures.get().saturating_add(1));
        }
        self.loaded.set(false);
    }
    pub(crate) fn failed(&self) {
        self.failures.set(self.failures.get().saturating_add(1));
    }
}
define_class!(
    // SAFETY: NSObject has no additional subclassing requirements; no Drop implementation.
    #[unsafe(super = NSObject)]
    #[name = "AppCoreUiWebResourceHandlerV1"]
    #[thread_kind = MainThreadOnly]
    #[ivars = WebState]
    pub(crate) struct WebHandler;
    // SAFETY: NSObjectProtocol has no additional safety requirements.
    unsafe impl NSObjectProtocol for WebHandler {}
    // SAFETY: All operations are synchronous/main-thread; cancellation is observed between callbacks.
    unsafe impl WKURLSchemeHandler for WebHandler {
        #[unsafe(method(webView:startURLSchemeTask:))]
        fn start(&self, _: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            let state = self.ivars();
            let key = std::ptr::from_ref(task) as *const () as usize;
            let cancelled = Rc::new(Cell::new(false));
            if state.tasks.borrow().len() >= 16 {
                self.reject(task);
                return;
            }
            state.tasks.borrow_mut().insert(key, cancelled.clone());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.serve(task, &cancelled)
            }));
            state.tasks.borrow_mut().remove(&key);
            if !matches!(result, Ok(true)) && !cancelled.get() {
                self.reject(task);
            }
        }
        #[unsafe(method(webView:stopURLSchemeTask:))]
        fn stop(&self, _: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            let key = std::ptr::from_ref(task) as *const () as usize;
            if let Some(cancelled) = self.ivars().tasks.borrow().get(&key) {
                cancelled.set(true);
            }
        }
    }
    // SAFETY: The decision block is called exactly once with an explicit allow/cancel result.
    unsafe impl WKNavigationDelegate for WebHandler {
        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn process_terminated(&self, _: &WKWebView) {
            self.ivars().renderer_failed();
        }
        #[unsafe(method(webView:didFinishNavigation:))]
        fn finished(&self, view: &WKWebView, _: Option<&WKNavigation>) {
            // SAFETY: Framework-owned view and current URL are used synchronously.
            self.ivars()
                .loaded
                .set(unsafe { view.URL().is_some_and(|url| self.ivars().document(&url)) });
        }
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn navigate(
            &self,
            _: &WKWebView,
            action: &WKNavigationAction,
            decision: &block2::DynBlock<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            // SAFETY: Framework-provided request/frame objects, used only for current decision.
            let mut allowed = unsafe {
                action
                    .targetFrame()
                    .is_some_and(|frame| frame.isMainFrame())
                    && action
                        .request()
                        .URL()
                        .is_some_and(|url| self.ivars().document(&url))
            };
            if self.ivars().bridge_locked.get() && self.ivars().document_started.get() {
                allowed = false;
            }
            if allowed {
                self.ivars().document_started.set(true);
            }
            if !allowed {
                self.ivars().failed();
            }
            decision.call((if allowed {
                WKNavigationActionPolicy::Allow
            } else {
                WKNavigationActionPolicy::Cancel
            },));
        }
    }
);
impl WebHandler {
    pub(crate) fn new(mtm: MainThreadMarker, state: WebState) -> Retained<Self> {
        let object = Self::alloc(mtm).set_ivars(state);
        // SAFETY: NSObject init initializes the allocated superclass with the standard signature.
        unsafe { msg_send![super(object), init] }
    }
    fn reject(&self, task: &ProtocolObject<dyn WKURLSchemeTask>) {
        self.ivars().failed();
        // SAFETY: A controlled domain/code and no borrowed userInfo are passed to the system.
        let error = unsafe {
            NSError::errorWithDomain_code_userInfo(&NSString::from_str("appcore-ui"), 1, None)
        };
        // SAFETY: Rejection occurs once during the task's start callback, before completion.
        unsafe {
            task.didFailWithError(&error);
        }
    }
    fn serve(&self, task: &ProtocolObject<dyn WKURLSchemeTask>, cancelled: &Cell<bool>) -> bool {
        // SAFETY: The framework keeps this request valid during the synchronous callback.
        let request = unsafe { task.request() };
        if request
            .HTTPMethod()
            .is_none_or(|method| method.to_string() != "GET")
        {
            return false;
        }
        let Some(url) = request.URL() else {
            return false;
        };
        let Some(path) = self.ivars().path(&url) else {
            return false;
        };
        let Ok(resource) = self.ivars().bundle.resource(&path) else {
            return false;
        };
        let keys = [
            NSString::from_str("Content-Type"),
            NSString::from_str("Content-Security-Policy"),
            NSString::from_str("X-Content-Type-Options"),
        ];
        let values = [
            NSString::from_str(resource.media.mime_type()),
            NSString::from_str(self.ivars().bundle.content_security_policy()),
            NSString::from_str("nosniff"),
        ];
        let headers = NSDictionary::from_slices(
            &keys.iter().map(|value| &**value).collect::<Vec<_>>(),
            &values.iter().map(|value| &**value).collect::<Vec<_>>(),
        );
        let Some(response) = NSHTTPURLResponse::initWithURL_statusCode_HTTPVersion_headerFields(
            NSHTTPURLResponse::alloc(),
            &url,
            200,
            Some(&NSString::from_str("HTTP/1.1")),
            Some(&headers),
        ) else {
            return false;
        };
        let data = NSData::with_bytes(resource.bytes);
        // SAFETY: Ordered response/data/finish, checking reentrant stop between each call.
        unsafe {
            if cancelled.get() {
                return true;
            }
            task.didReceiveResponse(&response);
            if cancelled.get() {
                return true;
            }
            task.didReceiveData(&data);
            if cancelled.get() {
                return true;
            }
            task.didFinish();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiWebAsset, UiWebMedia};
    #[test]
    fn system_urls_cannot_escape_the_declared_web_origin_or_resource_set() {
        let state = WebState {
            bundle: Arc::new(
                UiWebBundle::new(
                    "index.html".into(),
                    [UiWebAsset {
                        path: "index.html".into(),
                        media: UiWebMedia::Html,
                        bytes: b"<!doctype html>".to_vec(),
                    }],
                )
                .unwrap(),
            ),
            host: "scope-a".into(),
            closed: Rc::new(Cell::new(false)),
            loaded: Cell::new(false),
            bridge_locked: Cell::new(false),
            document_started: Cell::new(false),
            process_failed: Cell::new(false),
            renderer_failures: Cell::new(0),
            failures: Cell::new(0),
            tasks: RefCell::new(HashMap::new()),
        };
        let valid =
            NSURL::URLWithString(&NSString::from_str("appcore-ui://scope-a/index.html")).unwrap();
        assert_eq!(state.path(&valid), Some("index.html".into()));
        assert!(state.document(&valid));
        let css =
            NSURL::URLWithString(&NSString::from_str("appcore-ui://scope-a/page.css")).unwrap();
        assert!(!state.document(&css));
        for input in [
            "https://scope-a/index.html",
            "appcore-ui://other/index.html",
            "appcore-ui://scope-a:80/index.html",
            "appcore-ui://user@scope-a/index.html",
            "appcore-ui://scope-a/%2e%2e/index.html",
            "appcore-ui://scope-a/index.html?principal=a",
            "appcore-ui://scope-a/missing.html",
        ] {
            let url = NSURL::URLWithString(&NSString::from_str(input)).unwrap();
            assert!(state.path(&url).is_none(), "{input}");
        }
        state.closed.set(true);
        assert!(state.path(&valid).is_none());
    }
}
