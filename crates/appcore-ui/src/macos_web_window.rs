// =============================================================================
//        #######
//     ###       ###     F: macos_web_window.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Explicit web mounting in the system engine with ephemeral data and local-only resources.
use crate::{
    macos_web_bridge::{BridgeState, MessageBridge},
    macos_web_handler::{WebHandler, WebState},
    UiCaller, UiError, UiNativeApplication, UiNativePage, UiNativePageV1, UiNativeWindow, UiResult,
    UiSurface, UiWebBundle,
};
use objc2::{rc::Retained, runtime::ProtocolObject, DefinedClass, MainThreadOnly};
use objc2_app_kit::NSAutoresizingMaskOptions;
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString, NSURLRequest, NSURL};
use objc2_web_kit::{
    WKContentWorld, WKUserScript, WKUserScriptInjectionTime, WKWebView, WKWebViewConfiguration,
    WKWebsiteDataStore,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::Arc,
    time::Duration,
};
/// Explicitly mounted local web page. Deployment owns its main-thread pump and close hook.
/// A Runtime surface can be attached explicitly before the first pump. Backends must be nonblocking.
pub struct UiWebWindow {
    native: UiNativeWindow,
    view: Retained<WKWebView>,
    handler: Retained<WebHandler>,
    load_deadline: Cell<std::time::Instant>,
    url: Retained<NSURL>,
    requested: Cell<bool>,
    bridge: Option<Retained<MessageBridge>>,
}
impl UiWebWindow {
    /// Mounts declared immutable resources, applying CSP and denying foreign navigation.
    pub fn open(
        application: &UiNativeApplication,
        bundle: Arc<UiWebBundle>,
        title: String,
        width: f64,
        height: f64,
        on_close: impl Fn() + 'static,
    ) -> UiResult<Self> {
        let mtm = MainThreadMarker::new().ok_or(UiError::Unavailable)?;
        let page = UiNativePage::new(UiNativePageV1 {
            version: 1,
            title,
            width,
            height,
            nodes: vec![],
        })?;
        let native = UiNativeWindow::open(application, &page, 1, on_close)?;
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|_| UiError::Unavailable)?;
        let host = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let url = NSURL::URLWithString(&NSString::from_str(&format!(
            "appcore-ui://{host}/{}",
            bundle.entry()
        )))
        .ok_or(UiError::InvalidRequest)?;
        let handler = WebHandler::new(
            mtm,
            WebState {
                bundle,
                host,
                closed: native.close_signal(),
                loaded: Cell::new(false),
                bridge_locked: Cell::new(false),
                document_started: Cell::new(false),
                process_failed: Cell::new(false),
                renderer_failures: Cell::new(0),
                failures: Cell::new(0),
                tasks: RefCell::new(HashMap::new()),
            },
        );
        // SAFETY: Configuration is created and consumed only on the owning main thread.
        let configuration = unsafe { WKWebViewConfiguration::new(mtm) };
        // SAFETY: A retained handler for a custom non-HTTP scheme, and a system ephemeral data store.
        let view = unsafe {
            configuration.setWebsiteDataStore(&WKWebsiteDataStore::nonPersistentDataStore(mtm));
            configuration.setURLSchemeHandler_forURLScheme(
                Some(ProtocolObject::from_ref(&*handler)),
                &NSString::from_str("appcore-ui"),
            );
            crate::macos_web_view::WebView::create(
                mtm,
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height)),
                &configuration,
            )
        };
        view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        // SAFETY: The delegate is retained in this owner until stopLoading and delegate removal on Drop.
        unsafe {
            view.setNavigationDelegate(Some(ProtocolObject::from_ref(&*handler)));
        }
        native.mount_view(&view);
        Ok(Self {
            native,
            view,
            handler,
            load_deadline: Cell::new(std::time::Instant::now() + Duration::from_secs(30)),
            url,
            requested: Cell::new(false),
            bridge: None,
        })
    }
    /// Attaches a mandatory-authorized Runtime surface before loading begins.
    /// The deployment factory receives the actual window identity/origin, never UI JSON.
    /// Authorization and handlers must be nonblocking or use a deployment-owned blocking boundary.
    /// Document changes are denied after attachment; reconnect creates a new window/surface.
    pub fn attach_surface(
        &mut self,
        create: impl FnOnce(UiCaller) -> UiResult<UiSurface>,
    ) -> UiResult<()> {
        if self.requested.get() || self.bridge.is_some() {
            return Err(UiError::Busy);
        }
        let host = &self.handler.ivars().host;
        let caller = UiCaller::new(host.clone(), format!("appcore-ui://{host}"))?;
        let surface = create(caller.clone())?;
        if surface.id() != caller.surface_id() || surface.origin() != caller.origin() {
            surface.close();
            return Err(UiError::Forbidden);
        }
        let document = self
            .url
            .absoluteString()
            .ok_or(UiError::Unavailable)?
            .to_string();
        let mtm = MainThreadMarker::new().ok_or(UiError::Unavailable)?;
        let state = BridgeState::new(caller, document, self.native.close_signal());
        *state.surface.borrow_mut() = Some(surface);
        let bridge = MessageBridge::new(mtm, state);
        let source=format!("Object.defineProperty(window,'appcoreUiV1',{{value:Object.freeze({{version:1,surface_id:{}}}),writable:false,configurable:false}});",serde_json::to_string(host).map_err(|_|UiError::InvalidRequest)?);
        // SAFETY: Retained main-thread controller/handler, one fixed name and main-frame-only bootstrap.
        unsafe {
            let controller = self.view.configuration().userContentController();
            controller.addScriptMessageHandlerWithReply_contentWorld_name(
                ProtocolObject::from_ref(&*bridge),
                &WKContentWorld::pageWorld(mtm),
                &NSString::from_str("appcoreUiV1"),
            );
            controller.addUserScript(
                &WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                    WKUserScript::alloc(mtm),
                    &NSString::from_str(&source),
                    WKUserScriptInjectionTime::AtDocumentStart,
                    true,
                ),
            );
        }
        self.handler.ivars().bridge_locked.set(true);
        self.bridge = Some(bridge);
        Ok(())
    }
    /// Native replies retained until completion, bounded by surface admission and two control slots.
    pub fn pending_requests(&self) -> usize {
        self.bridge
            .as_ref()
            .map_or(0, |bridge| bridge.ivars().pending())
    }
    /// Pumps a bounded native input batch and services bridge work before and after dispatch.
    /// Engine loss or initial load exceeding 30 seconds closes the surface; no automatic retry.
    /// The deployment must continue pumping for this cooperative deadline to advance.
    pub fn pump(&self, wait: Duration) -> UiResult<bool> {
        if self.native.close_signal().get() {
            if let Some(bridge) = &self.bridge {
                bridge.ivars().close();
            }
            return Ok(false);
        }
        if !self.requested.replace(true) {
            self.load_deadline
                .set(std::time::Instant::now() + Duration::from_secs(30));
            // SAFETY: Only the host-selected local entry starts this view's loading.
            unsafe {
                self.view
                    .loadRequest(&NSURLRequest::requestWithURL(&self.url));
            }
        }
        if let Some(bridge) = &self.bridge {
            bridge.ivars().pump();
        }
        if self.handler.ivars().process_failed.get()
            || (!self.loaded() && std::time::Instant::now() >= self.load_deadline.get())
        {
            self.handler.ivars().renderer_failed();
            self.close();
            return Ok(false);
        }
        let alive = self.native.pump(wait)?;
        if let Some(bridge) = &self.bridge {
            if alive {
                bridge.ivars().pump();
            } else {
                bridge.ivars().close();
            }
        }
        Ok(alive)
    }
    /// Whether the selected local document completed loading successfully.
    pub fn loaded(&self) -> bool {
        self.handler.ivars().loaded.get()
    }
    /// Resource/navigation rejection counter without URLs or page content.
    pub fn rejected_requests(&self) -> u64 {
        self.handler.ivars().failures.get()
    }
    /// Engine loss or initial-load timeout, counted once per mounted view.
    pub fn renderer_failures(&self) -> u64 {
        self.handler.ivars().renderer_failures.get()
    }
    /// Closes the window and stops loading resources.
    pub fn close(&self) {
        if let Some(bridge) = &self.bridge {
            bridge.ivars().close();
        }
        self.native.close();
        // SAFETY: Main-thread-owned view remains valid until Drop completes.
        unsafe {
            self.view.stopLoading();
        }
    }
}
impl Drop for UiWebWindow {
    fn drop(&mut self) {
        self.close();
        // SAFETY: Clearing the nonowning delegate before the retained handler is released.
        unsafe {
            self.view.setNavigationDelegate(None);
            self.view
                .configuration()
                .userContentController()
                .removeAllScriptMessageHandlers();
            self.view
                .configuration()
                .userContentController()
                .removeAllUserScripts();
        }
    }
}
