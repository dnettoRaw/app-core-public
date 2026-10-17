// =============================================================================
//        #######
//     ###       ###     F: lorenz_recovery.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use super::{configure, initialize, minimize_cycle, Result};
use appcore_render::{
    gpu, LorenzView, NativeRenderer, PresentationState, RenderError, RenderLimits,
};
use appcore_ui::{UiError, UiNativeApplication, UiNativePage, UiNativePageV1, UiNativeWindow};
use std::time::{Duration, Instant};

fn drain(window: &UiNativeWindow, renderer: &NativeRenderer<'_>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        renderer.poll()?;
        if renderer.inflight() == 0 {
            assert_eq!(
                renderer.frame_progress().submitted,
                renderer.frame_progress().completed
            );
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("Lorenz drain timeout".into());
        }
        if !window.pump(Duration::from_millis(2))? {
            return Err("Lorenz window closed during drain".into());
        }
    }
}
fn draw(
    window: &UiNativeWindow,
    renderer: &mut NativeRenderer<'_>,
    steps: u32,
    view: LorenzView,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match renderer.present_lorenz(steps, view) {
            Ok(true) => return drain(window, renderer),
            Ok(false) | Err(RenderError::FramesPending) if Instant::now() < deadline => {
                if !window.pump(Duration::from_millis(2))? {
                    return Err("Lorenz window closed during draw".into());
                }
            }
            Ok(false) | Err(RenderError::FramesPending) => {
                return Err("Lorenz presentation timeout".into())
            }
            Err(error) => return Err(error.into()),
        }
    }
}
fn resize(
    window: &UiNativeWindow,
    renderer: &mut NativeRenderer<'_>,
    size: (u32, u32),
) -> Result<()> {
    let previous = window.graphics_size()?;
    window.qualification_content_size(size.0, size.1)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while window.graphics_size()? == previous {
        if Instant::now() >= deadline {
            return Err("native content resize was not observed".into());
        }
        if !window.pump(Duration::from_millis(2))? {
            return Err("window closed during resize".into());
        }
    }
    configure(window, renderer, window.graphics_size()?)?;
    assert_eq!(renderer.state(), PresentationState::Ready);
    Ok(())
}
fn cycle(window: &UiNativeWindow, renderer: &mut NativeRenderer<'_>, index: u32) -> Result<()> {
    let view = LorenzView {
        yaw: index as f32 * 0.2,
        selected: index,
        isolate: index.is_multiple_of(2),
        ..Default::default()
    };
    draw(window, renderer, 8, view)?;
    assert_eq!(renderer.lorenz_samples(), 8);
    for size in [(960, 640), (640, 480), (320, 240)] {
        resize(window, renderer, size)?;
        assert_eq!(renderer.lorenz_samples(), 8);
        draw(window, renderer, 0, view)?;
        assert_eq!(renderer.lorenz_samples(), 8);
    }
    minimize_cycle(window, renderer)?;
    assert_eq!(renderer.lorenz_samples(), 8);
    draw(window, renderer, 0, view)?;
    renderer.reset_lorenz()?;
    assert_eq!(renderer.lorenz_samples(), 0);
    draw(window, renderer, 0, view)?;
    assert_eq!(renderer.lorenz_samples(), 0);
    draw(window, renderer, 1, view)?;
    assert_eq!(renderer.lorenz_samples(), 1);
    renderer.reset_lorenz()?;
    Ok(())
}
pub fn run(app: &UiNativeApplication) -> Result<()> {
    let page = UiNativePage::new(UiNativePageV1 {
        version: 1,
        title: "E3DN Lorenz resize recovery".into(),
        width: 320.,
        height: 240.,
        nodes: vec![],
    })?;
    let window = UiNativeWindow::open(app, &page, 32, || {})?;
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let mut renderer = initialize(&window, &instance, RenderLimits::default())?;
    configure(&window, &mut renderer, window.graphics_size()?)?;
    renderer.enable_lorenz()?;
    let initial_size = window.graphics_size()?;
    for size in [
        (0, 240),
        (319, 240),
        (320, 239),
        (4096, 4096),
        (u32::MAX, u32::MAX),
    ] {
        assert!(matches!(
            window.qualification_content_size(size.0, size.1),
            Err(UiError::InvalidRequest)
        ));
        assert_eq!(window.graphics_size()?, initial_size);
    }
    for index in 0..3 {
        cycle(&window, &mut renderer, index)?;
    }
    let progress = renderer.frame_progress();
    assert_eq!(progress.submitted, 21);
    assert_eq!(progress.completed, 21);
    renderer.begin_close();
    renderer.finish_close()?;
    drop(renderer);
    window.close();
    assert!(matches!(
        window.qualification_content_size(640, 480),
        Err(UiError::Unavailable)
    ));
    drop(window);
    assert_eq!(app.retained_windows(), 0);
    println!("lorenz_native_resizes=9 lorenz_minimize_restore_cycles=3 paused_samples_preserved=true reset_step_preserved=true submitted={} completed={} retained_windows=0",progress.submitted,progress.completed);
    Ok(())
}
