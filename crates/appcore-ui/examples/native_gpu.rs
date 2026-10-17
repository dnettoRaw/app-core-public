// =============================================================================
//        #######
//     ###       ###     F: native_gpu.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

//! Deployment-owned native surface qualification; no Runtime or hidden executor.
#[cfg(all(feature = "gpu-macos", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_render::{gpu, NativeRenderer, RenderError, RenderLimits};
    use appcore_ui::{UiNativeApplication, UiNativeWindow};
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
        time::{Duration, Instant},
    };
    let app = UiNativeApplication::new(1)?;
    let page = qualification_page()?;
    let window = UiNativeWindow::open(&app, &page, 32, || {})?;
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let surface = window.graphics_surface(&instance)?;
    assert!(window.graphics_surface(&instance).is_err());
    let mut renderer = {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = std::pin::pin!(NativeRenderer::new(
            &instance,
            surface,
            RenderLimits::default()
        ));
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
                break result?;
            }
            if !window.pump(Duration::from_millis(2))? || Instant::now() >= deadline {
                return Err("GPU startup interrupted/timed out".into());
            }
        }
    };
    let deadline = Instant::now() + Duration::from_secs(180);
    let start = Instant::now();
    let mut size = (0, 0);
    let mut frames = 0;
    let mut clicks = 0u32;
    while Instant::now() < deadline && window.pump(Duration::from_millis(8))? {
        while let Some(action) = window.next_activation() {
            if action.action_id == "count" {
                clicks = clicks.saturating_add(1);
                window.set_label("clicks", &format!("Clicks: {clicks}"))?;
            }
        }
        let next = window.graphics_size()?;
        if next != size {
            match renderer.resize(next.0, next.1) {
                Ok(()) => size = next,
                Err(RenderError::FramesPending) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        let t = start.elapsed().as_secs_f64();
        match renderer.present_clear([0.04, 0.15 + t.sin().abs() * 0.2, 0.3, 1.0]) {
            Ok(true) => frames += 1,
            Ok(false) | Err(RenderError::FramesPending) => {}
            Err(RenderError::SurfaceUnavailable) => size = (0, 0),
            Err(error) => return Err(error.into()),
        }
    }
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
    println!(
        "frames={frames} clicks={clicks} retained_windows={}",
        app.retained_windows()
    );
    Ok(())
}
#[cfg(not(all(feature = "gpu-macos", target_os = "macos")))]
fn main() {
    eprintln!("Select --features gpu-macos on macOS");
}

#[cfg(all(feature = "gpu-macos", target_os = "macos"))]
fn qualification_page() -> appcore_ui::UiResult<appcore_ui::UiNativePage> {
    use appcore_ui::{elements, UiNativePage, UiNativePageV1, UiRectangle};
    UiNativePage::new(UiNativePageV1 {
        version: 1,
        title: "E3DN native GPU surface".into(),
        width: 800.0,
        height: 600.0,
        nodes: vec![
            elements::button(
                "count",
                UiRectangle {
                    x: 24.0,
                    y: 24.0,
                    width: 160.0,
                    height: 36.0,
                },
                "Count click",
                "count",
            ),
            elements::text(
                "clicks",
                UiRectangle {
                    x: 200.0,
                    y: 24.0,
                    width: 240.0,
                    height: 36.0,
                },
                "Clicks: 0",
            ),
        ],
    })
}
