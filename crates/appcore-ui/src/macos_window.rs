// =============================================================================
//        #######
//     ###       ###     F: macos_window.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Native `AppKit` page rendering. Deployment explicitly owns the main-thread event pump.
use crate::{
    macos_callbacks::{Callbacks, UiCallbacks},
    macos_controls::NativeControl,
    UiError, UiNativePage, UiNativeValue, UiResult,
};
use objc2::{
    rc::{autoreleasepool, Retained},
    runtime::ProtocolObject,
    MainThreadOnly,
};
#[cfg(feature = "web-macos")]
use objc2_app_kit::NSView;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSAutoresizingMaskOptions, NSBackingStoreType,
    NSEventMask, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSDate, NSDefaultRunLoopMode, NSPoint, NSRect, NSRunLoop, NSSize, NSString,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::mpsc::{sync_channel, Receiver},
    time::Duration,
};
/// Trusted activation from a native control, containing no payload or authority.
#[derive(Debug)]
pub struct UiNativeActivation {
    /// Page-local control identity.
    pub element_id: String,
    /// Application binding to a declared operation; it does not grant execution.
    pub action_id: String,
}
/// Explicit deployment-owned native application preparation and window budget.
/// Create one controller per process; it starts no Runtime or background worker.
pub struct UiNativeApplication {
    mtm: MainThreadMarker,
    live: Rc<Cell<usize>>,
    capacity: usize,
}
impl UiNativeApplication {
    /// Prepares system event handling explicitly on the main thread; capacity is 1–8.
    pub fn new(capacity: usize) -> UiResult<Self> {
        if !(1..=8).contains(&capacity) {
            return Err(UiError::InvalidRequest);
        }
        let mtm = MainThreadMarker::new().ok_or(UiError::Unavailable)?;
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        app.finishLaunching();
        Ok(Self {
            mtm,
            live: Rc::new(Cell::new(0)),
            capacity,
        })
    }
    /// Retained physical windows, including closed handles not yet dropped.
    pub fn retained_windows(&self) -> usize {
        self.live.get()
    }
}
struct WindowPermit(Rc<Cell<usize>>);
impl Drop for WindowPermit {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
/// One main-thread-owned native window. Drop closes it and releases its callbacks.
///
/// The caller supplies a close hook, e.g. to close its authenticated `UiSurface`.
/// This type never starts a process, listener, Runtime or background task.
pub struct UiNativeWindow {
    window: Retained<NSWindow>,
    callbacks: Vec<Retained<UiCallbacks>>,
    controls: BTreeMap<String, NativeControl>,
    page: RefCell<UiNativePage>,
    receiver: Receiver<UiNativeActivation>,
    closed: Rc<Cell<bool>>,
    dropped: Rc<Cell<u64>>,
    queued: Rc<Cell<usize>>,
    queue_capacity: usize,
    failures: Rc<Cell<u64>>,
    mtm: MainThreadMarker,
    _permit: WindowPermit,
}
impl UiNativeWindow {
    /// Renders a validated Rust page using system text and button controls.
    /// Must be called on the main thread; queue capacity must be 1–256.
    pub fn open(
        application: &UiNativeApplication,
        page: &UiNativePage,
        queue_capacity: usize,
        on_close: impl Fn() + 'static,
    ) -> UiResult<Self> {
        let mtm = application.mtm;
        if application.live.get() >= application.capacity {
            return Err(UiError::Busy);
        }
        if !(1..=256).contains(&queue_capacity) {
            return Err(UiError::InvalidRequest);
        }
        application.live.set(application.live.get() + 1);
        let permit = WindowPermit(application.live.clone());
        autoreleasepool(|_| Self::create(page, queue_capacity, Rc::new(on_close), mtm, permit))
    }
    fn create(
        page: &UiNativePage,
        capacity: usize,
        on_close: Rc<dyn Fn()>,
        mtm: MainThreadMarker,
        permit: WindowPermit,
    ) -> UiResult<Self> {
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: Valid main-thread allocation, validated finite dimensions; disable auto-release below.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(page.width(), page.height()),
                ),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Resizable
                    | NSWindowStyleMask::Miniaturizable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: Retained exclusively owns this window's release; closing must not auto-release it.
        unsafe {
            window.setReleasedWhenClosed(false);
        }
        window.setTitle(&NSString::from_str(page.title()));
        window.setContentMinSize(NSSize::new(page.width(), page.height()));
        let scale = (4_194_304.0 / (page.width() * page.height())).sqrt();
        window.setContentMaxSize(NSSize::new(page.width() * scale, page.height() * scale));
        let content = window.contentView().ok_or(UiError::Unavailable)?;
        let (sender, receiver) = sync_channel(capacity);
        let closed = Rc::new(Cell::new(false));
        let dropped = Rc::new(Cell::new(0));
        let queued = Rc::new(Cell::new(0));
        let failures = Rc::new(Cell::new(0));
        let delegate = UiCallbacks::new(
            mtm,
            Callbacks {
                element: String::new(),
                action: String::new(),
                sender: sender.clone(),
                closed: closed.clone(),
                dropped: dropped.clone(),
                queued: queued.clone(),
                failures: failures.clone(),
                on_close: on_close.clone(),
            },
        );
        window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        let mut callbacks = vec![delegate];
        let mut controls = BTreeMap::new();
        for node in page.nodes() {
            let target = node.action().map(|action| {
                UiCallbacks::new(
                    mtm,
                    Callbacks {
                        element: node.id().to_owned(),
                        action: action.to_owned(),
                        sender: sender.clone(),
                        closed: closed.clone(),
                        dropped: dropped.clone(),
                        queued: queued.clone(),
                        failures: failures.clone(),
                        on_close: on_close.clone(),
                    },
                )
            });
            let control = NativeControl::create(node, mtm, target.as_deref());
            let view = control.view();
            controls.insert(node.id().to_owned(), control);
            if let Some(target) = target {
                callbacks.push(target);
            }
            let rect = node.rect();
            view.setFrame(NSRect::new(
                NSPoint::new(rect.x, page.height() - rect.y - rect.height),
                NSSize::new(rect.width, rect.height),
            ));
            view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            content.addSubview(&view);
        }
        window.center();
        window.makeKeyAndOrderFront(None);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        Ok(Self {
            window,
            callbacks,
            controls,
            page: RefCell::new(page.clone()),
            receiver,
            closed,
            dropped,
            queued,
            queue_capacity: capacity,
            failures,
            mtm,
            _permit: permit,
        })
    }
    /// Updates an existing native label after validating per-label and aggregate page budgets.
    /// Does not change action bindings, geometry or authority. Closed windows reject updates.
    pub fn set_label(&self, id: &str, label: &str) -> UiResult<()> {
        if self.closed.get() {
            return Err(UiError::Closed);
        }
        if label.len() > 4096 {
            return Err(UiError::PayloadTooLarge);
        }
        let page = self.page.borrow().with_label(id, label)?;
        let control = self.controls.get(id).ok_or(UiError::InvalidRequest)?;
        self.page.replace(page);
        control.set_label(label)
    }
    /// Enables/disables an interactive control; presentation does not grant authorization.
    /// Already queued activations still require application-side state/policy validation.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> UiResult<()> {
        if self.closed.get() {
            return Err(UiError::Closed);
        }
        self.controls
            .get(id)
            .ok_or(UiError::InvalidRequest)?
            .set_enabled(enabled)
    }
    /// Reads one bounded typed input value and validates it against the declared control.
    /// The application must still validate business meaning and authorize resulting commands.
    pub fn value(&self, id: &str) -> UiResult<UiNativeValue> {
        if self.closed.get() {
            return Err(UiError::Closed);
        }
        let value = self
            .controls
            .get(id)
            .ok_or(UiError::InvalidRequest)?
            .value()?;
        self.page.borrow().with_value(id, value.clone())?;
        Ok(value)
    }
    /// Applies a validated typed value without emitting an action or changing authorization.
    pub fn set_value(&self, id: &str, value: UiNativeValue) -> UiResult<()> {
        if self.closed.get() {
            return Err(UiError::Closed);
        }
        let page = self.page.borrow().with_value(id, value.clone())?;
        let control = self.controls.get(id).ok_or(UiError::InvalidRequest)?;
        self.page.replace(page);
        control.set_value(&value)
    }
    #[cfg(feature = "web-macos")]
    pub(crate) fn mount_view(&self, view: &NSView) {
        self.window.setContentView(Some(view));
    }
    #[cfg(feature = "web-macos")]
    pub(crate) fn close_signal(&self) -> Rc<Cell<bool>> {
        self.closed.clone()
    }
    /// Waits at most one second for the first event, then drains at most 64 queued events.
    /// The nonblocking drain has a cooperative 2 ms budget; native tracking/callbacks can run longer.
    /// Returns false after this window closes; deployment decides when its process exits.
    pub fn pump(&self, wait: Duration) -> UiResult<bool> {
        if wait > Duration::from_secs(1) {
            return Err(UiError::InvalidRequest);
        }
        if self.closed.get() {
            return Ok(false);
        }
        autoreleasepool(|_| {
            let app = NSApplication::sharedApplication(self.mtm);
            // SAFETY: Apple exports this immutable system run-loop mode for the process lifetime.
            let mode = unsafe { NSDefaultRunLoopMode };
            let backpressured = self.queued.get() >= self.queue_capacity;
            if !backpressured {
                crate::event_pump::drain(
                    wait,
                    |wait| {
                        let expiration = NSDate::dateWithTimeIntervalSinceNow(wait.as_secs_f64());
                        app.nextEventMatchingMask_untilDate_inMode_dequeue(
                            NSEventMask::Any,
                            Some(&expiration),
                            mode,
                            true,
                        )
                    },
                    |event| {
                        app.sendEvent(&event);
                        !self.closed.get() && self.queued.get() < self.queue_capacity
                    },
                );
            }
            if !self.closed.get() {
                // Service pending IPC/timers even while native input is continuously ready.
                let until = NSDate::dateWithTimeIntervalSinceNow(if backpressured {
                    wait.as_secs_f64()
                } else {
                    0.0
                });
                NSRunLoop::currentRunLoop().runMode_beforeDate(mode, &until);
            }
            app.updateWindows();
        });
        Ok(!self.closed.get())
    }
    /// Receives one pending activation without blocking the event pump.
    pub fn next_activation(&self) -> Option<UiNativeActivation> {
        let activation = self.receiver.try_recv().ok()?;
        self.queued.set(self.queued.get() - 1);
        Some(activation)
    }
    /// Pending activations; drain them after each pump to release native input backpressure.
    pub fn pending_activations(&self) -> usize {
        self.queued.get()
    }
    /// Cumulative activations rejected by the bounded queue.
    pub fn dropped_activations(&self) -> u64 {
        self.dropped.get()
    }
    /// Close-hook panics caught at the foreign callback boundary.
    pub fn callback_failures(&self) -> u64 {
        self.failures.get()
    }
    /// Closes this window, clears queued activations and invokes its close hook once.
    pub fn close(&self) {
        self.window.close();
        while self.receiver.try_recv().is_ok() {}
        self.queued.set(0);
    }
}
impl Drop for UiNativeWindow {
    fn drop(&mut self) {
        self.close();
        self.window.setDelegate(None);
        self.window.setContentView(None);
        self.callbacks.clear();
        self.controls.clear();
    }
}
