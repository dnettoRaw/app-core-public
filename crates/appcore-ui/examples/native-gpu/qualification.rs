// =============================================================================
//        #######
//     ###       ###     F: qualification.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use appcore_render::{gpu, NativeRenderer, PresentationState, RenderError, RenderLimits};
use appcore_ui::{UiNativeApplication, UiNativePage, UiNativePageV1, UiNativeWindow};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
#[path = "lorenz_recovery.rs"]
mod lorenz_recovery;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn initialize<'a>(
    window: &'a UiNativeWindow,
    instance: &gpu::Instance,
    limits: RenderLimits,
) -> Result<NativeRenderer<'a>> {
    let surface = window.graphics_surface(instance)?;
    let mut future = std::pin::pin!(NativeRenderer::new(instance, surface, limits));
    let mut context = Context::from_waker(Waker::noop());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return Ok(result?);
        }
        if !window.pump(Duration::from_millis(2))? || Instant::now() >= deadline {
            return Err("initialization interrupted/timed out".into());
        }
    }
}
fn configure(
    window: &UiNativeWindow,
    renderer: &mut NativeRenderer<'_>,
    size: (u32, u32),
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match renderer.resize(size.0, size.1) {
            Ok(()) => return Ok(()),
            Err(RenderError::FramesPending) if Instant::now() < deadline => {
                window.pump(Duration::from_millis(2))?;
            }
            Err(error) => return Err(error.into()),
        }
    }
}
fn minimize_cycle(window: &UiNativeWindow, renderer: &mut NativeRenderer<'_>) -> Result<()> {
    window.qualification_minimized(true)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while window.graphics_size()? != (0, 0) {
        if Instant::now() >= deadline {
            return Err("minimize timeout".into());
        }
        window.pump(Duration::from_millis(2))?;
    }
    configure(window, renderer, (0, 0))?;
    assert_eq!(renderer.state(), PresentationState::Suspended);
    assert!(!renderer.present_clear([0.1, 0.2, 0.3, 1.0])?);
    window.qualification_minimized(false)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while window.graphics_size()? == (0, 0) {
        if Instant::now() >= deadline {
            return Err("restore timeout".into());
        }
        window.pump(Duration::from_millis(2))?;
    }
    configure(window, renderer, window.graphics_size()?)?;
    assert_eq!(renderer.state(), PresentationState::Ready);
    Ok(())
}
fn run_generation(app: &UiNativeApplication, destroy: bool) -> Result<()> {
    let page = UiNativePage::new(UiNativePageV1 {
        version: 1,
        title: format!("E3DN recovery generation {}", if destroy { 1 } else { 2 }),
        width: 480.0,
        height: 320.0,
        nodes: vec![],
    })?;
    let window = UiNativeWindow::open(app, &page, 32, || {})?;
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let mut renderer = initialize(&window, &instance, RenderLimits::default())?;
    configure(&window, &mut renderer, window.graphics_size()?)?;
    for _ in 0..3 {
        minimize_cycle(&window, &mut renderer)?;
    }
    renderer.qualification_surface_loss()?;
    assert_eq!(renderer.state(), PresentationState::Suspended);
    assert!(!renderer.present_clear([0.1, 0.2, 0.3, 1.0])?);
    configure(&window, &mut renderer, window.graphics_size()?)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut frames = 0;
    while Instant::now() < deadline && window.pump(Duration::from_millis(8))? {
        match renderer.present_clear([0.1, 0.35, 0.5, 1.0]) {
            Ok(true) => frames += 1,
            Ok(false) | Err(RenderError::FramesPending) => {}
            Err(error) => return Err(error.into()),
        }
    }
    assert!(frames > 0);
    if destroy {
        renderer.qualification_destroy_device();
        let deadline = Instant::now() + Duration::from_secs(2);
        while renderer.state() != PresentationState::Lost {
            let _ = renderer.poll();
            if Instant::now() >= deadline {
                return Err("device-loss callback timeout".into());
            }
            window.pump(Duration::from_millis(2))?;
        }
        assert_eq!(
            renderer.present_clear([0.1, 0.2, 0.3, 1.0]),
            Err(RenderError::DeviceUnavailable)
        );
    } else {
        renderer.begin_close();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match renderer.finish_close() {
                Ok(()) => break,
                Err(RenderError::FramesPending) if Instant::now() < deadline => {
                    window.pump(Duration::from_millis(2))?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        assert_eq!(
            renderer.present_clear([0.1, 0.2, 0.3, 1.0]),
            Err(RenderError::RendererClosed)
        );
    }
    let pending = renderer.inflight();
    drop(renderer);
    window.close();
    drop(window);
    assert_eq!(app.retained_windows(), 0);
    println!(
        "destroyed={destroy} frames={frames} pending_before_drop={pending} retained_windows={}",
        app.retained_windows()
    );
    Ok(())
}
fn aborted_generation(app: &UiNativeApplication) -> Result<()> {
    let page = UiNativePage::new(UiNativePageV1 {
        version: 1,
        title: "E3DN aborted generation".into(),
        width: 480.,
        height: 320.,
        nodes: vec![],
    })?;
    let window = UiNativeWindow::open(app, &page, 32, || {})?;
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let mut renderer = initialize(&window, &instance, RenderLimits::default())?;
    configure(&window, &mut renderer, window.graphics_size()?)?;
    assert!(renderer.present_clear([0.1, 0.2, 0.3, 1.])?);
    let before = renderer.frame_progress();
    let pending = renderer.inflight();
    renderer.abort_close();
    renderer.abort_close();
    assert_eq!(renderer.state(), PresentationState::Closed);
    assert_eq!(renderer.frame_progress(), before);
    assert_eq!(renderer.inflight(), pending);
    assert_eq!(renderer.finish_close(), Err(RenderError::RendererClosed));
    renderer.begin_close();
    assert_eq!(renderer.finish_close(), Err(RenderError::RendererClosed));
    assert_eq!(renderer.resize(480, 320), Err(RenderError::RendererClosed));
    assert_eq!(
        renderer.present_clear([0., 0., 0., 1.]),
        Err(RenderError::RendererClosed)
    );
    drop(renderer);
    window.close();
    drop(window);
    assert_eq!(app.retained_windows(), 0);
    println!("abort_idempotent=true completion_preserved=true pending_at_abort={pending} retained_windows=0");
    Ok(())
}
fn lorenz_resize_budget(app: &UiNativeApplication) -> Result<()> {
    let page = UiNativePage::new(UiNativePageV1 {
        version: 1,
        title: "E3DN Lorenz resize admission".into(),
        width: 480.,
        height: 320.,
        nodes: vec![],
    })?;
    let window = UiNativeWindow::open(app, &page, 32, || {})?;
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let limits = RenderLimits {
        managed_bytes: appcore_render::LorenzCompute::BUFFER_BYTES
            + 96
            + 960 * 640 * 8
            + 480 * 320 * 4
            - 1,
        ..RenderLimits::default()
    };
    let mut renderer = initialize(&window, &instance, limits)?;
    configure(&window, &mut renderer, (480, 320))?;
    renderer.enable_lorenz()?;
    assert_eq!(renderer.resize(960, 640), Err(RenderError::BudgetExceeded));
    assert_eq!(renderer.state(), PresentationState::Ready);
    assert_eq!(renderer.lorenz_samples(), 0);
    configure(&window, &mut renderer, (480, 320))?;
    assert!(renderer.present_lorenz(1, appcore_render::LorenzView::default())?);
    assert_eq!(renderer.lorenz_samples(), 1);
    renderer.begin_close();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match renderer.finish_close() {
            Ok(()) => break,
            Err(RenderError::FramesPending) if Instant::now() < deadline => {
                window.pump(Duration::from_millis(2))?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    drop(renderer);
    window.close();
    drop(window);
    assert_eq!(app.retained_windows(), 0);
    println!("lorenz_resize_peak_rejected=true previous_scene_presented=true retained_windows=0");
    Ok(())
}
pub fn run() -> Result<()> {
    let app = UiNativeApplication::new(1)?;
    run_generation(&app, true)?;
    run_generation(&app, false)?;
    aborted_generation(&app)?;
    lorenz_resize_budget(&app)?;
    lorenz_recovery::run(&app)?;
    println!("minimize_restore_cycles=6 surface_loss_injections=2 device_destroyed=1 recreated=1");
    Ok(())
}
