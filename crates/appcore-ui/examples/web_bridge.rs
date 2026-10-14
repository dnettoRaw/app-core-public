// =============================================================================
//        #######
//     ###       ###     F: web_bridge.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! SDK-prepared generic fixture: real web queries/commands and Runtime-owned replay.
#[cfg(all(feature = "web-macos", target_os = "macos"))]
#[path = "web-bridge/runtime.rs"]
mod fixture_runtime;
#[cfg(all(feature = "web-macos", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_ui::{core::*, *};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let effects = Arc::new(AtomicUsize::new(0));
    let app = appcore_sdk::App::new("web-bridge-fixture")?;
    let prepared = app.prepare(
        &crate::fixture_runtime::ApplicationState(effects.clone()),
        NodeId::new("node-a").map_err(|_| appcore_ui::UiError::InvalidRequest)?,
    )?;
    let parts = prepared.into_parts();
    let instance = parts.0;
    let queries = parts.1;
    let identity = CoreIdentity {
        tenant_id: TenantId::new("tenant-a").map_err(|_| UiError::InvalidRequest)?,
        cluster_id: ClusterId::new("cluster-a").map_err(|_| UiError::InvalidRequest)?,
        core_id: CoreId::new("core-a").map_err(|_| UiError::InvalidRequest)?,
        instance_id: InstanceId::new("instance-a").map_err(|_| UiError::InvalidRequest)?,
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: instance.identity().clone(),
    };
    let context = Arc::new(crate::fixture_runtime::ContextOwner(
        instance.identity().clone(),
    ));
    let controller = RuntimeController::new(instance);
    for event in [
        RuntimeLifecycleEvent::ConfigLoaded,
        RuntimeLifecycleEvent::SecurityChecked,
        RuntimeLifecycleEvent::StorageOpened,
        RuntimeLifecycleEvent::ApiStarted,
    ] {
        controller
            .apply_lifecycle_event(event)
            .map_err(|_| UiError::BackendFailure)?;
    }
    let backend = Arc::new(crate::fixture_runtime::OwnerBackend {
        dispatch: UiRuntimeDispatch::new(
            controller.clone(),
            queries,
            context,
            UiLimits::default(),
        )?,
        identity,
    });
    let bundle = bundle()?;
    let application = UiNativeApplication::new(1)?;
    let mut window = UiWebWindow::open(
        &application,
        bundle,
        "AppCore Web Bridge".into(),
        620.0,
        300.0,
        || {},
    )?;
    let notifications = std::rc::Rc::new(std::cell::RefCell::new(None));
    let attached = notifications.clone();
    window.attach_surface(move |caller| {
        let surface = crate::fixture_runtime::surface(caller, backend)?;
        *attached.borrow_mut() = Some(surface.clone());
        Ok(surface)
    })?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    let mut published = 0;
    while std::time::Instant::now() < deadline
        && window.pump(std::time::Duration::from_millis(10))?
    {
        let emitted = controller.instance().event_bus().len();
        if emitted > published {
            if let Some(surface) = notifications.borrow().as_ref() {
                surface.publish(
                    "state.changed",
                    serde_json::json!({"effects":effects.load(Ordering::SeqCst)}),
                )?;
            }
            published = emitted;
        }
    }
    window.close();
    let pending = window.pending_requests();
    drop(window);
    println!(
        "effects={} events={} pending_requests={pending} retained_windows={}",
        effects.load(Ordering::SeqCst),
        controller.instance().event_bus().len(),
        application.retained_windows()
    );
    Ok(())
}
#[cfg(all(feature = "web-macos", target_os = "macos"))]
fn bundle() -> Result<std::sync::Arc<appcore_ui::UiWebBundle>, Box<dyn std::error::Error>> {
    use appcore_ui::*;
    use std::sync::Arc;
    let mut assets = vec![
        UiWebAsset {
            path: "index.html".into(),
            media: UiWebMedia::Html,
            bytes: include_bytes!("web-bridge/index.html").to_vec(),
        },
        UiWebAsset {
            path: "page.js".into(),
            media: UiWebMedia::JavaScript,
            bytes: include_bytes!("web-bridge/page.js").to_vec(),
        },
        UiWebAsset {
            path: "page.css".into(),
            media: UiWebMedia::Css,
            bytes: include_bytes!("web-preview/page.css").to_vec(),
        },
    ];
    for (name, bytes) in [
        (
            "index.js",
            include_bytes!("web-bridge/client/index.js").as_slice(),
        ),
        (
            "client.js",
            include_bytes!("web-bridge/client/client.js").as_slice(),
        ),
        (
            "protocol.js",
            include_bytes!("web-bridge/client/protocol.js").as_slice(),
        ),
        (
            "reply.js",
            include_bytes!("web-bridge/client/reply.js").as_slice(),
        ),
        (
            "web.js",
            include_bytes!("web-bridge/client/web.js").as_slice(),
        ),
    ] {
        assets.push(UiWebAsset {
            path: format!("client/{name}"),
            media: UiWebMedia::JavaScript,
            bytes: bytes.to_vec(),
        });
    }
    Ok(Arc::new(UiWebBundle::new("index.html".into(), assets)?))
}
#[cfg(not(all(feature = "web-macos", target_os = "macos")))]
fn main() {
    eprintln!("This fixture requires web-macos on macOS");
}
