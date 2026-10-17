// =============================================================================
//        #######
//     ###       ###     F: meshes.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use appcore_render::{gpu, MeshBatchDescriptor, NativeRenderer, RenderError, RenderLimits};
use appcore_ui::{
    elements, UiNativeApplication, UiNativePage, UiNativePageV1, UiNativeWindow, UiRectangle,
};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
#[path = "mesh_assets.rs"]
mod assets;
#[path = "mesh_camera.rs"]
mod camera;
#[path = "diagnostics.rs"]
mod diagnostics;
#[path = "input_metrics.rs"]
mod input_metrics;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn page() -> appcore_ui::UiResult<UiNativePage> {
    let mut nodes = vec![elements::text(
        "status",
        UiRectangle {
            x: 24.,
            y: 70.,
            width: 950.,
            height: 30.,
        },
        "320 instances · 2 meshes · current-frame Hi-Z · indirect GPU draw",
    )];
    for (i, (id, label)) in [
        ("pause", "Pause"),
        ("left", "Orbit left"),
        ("right", "Orbit right"),
        ("near", "Zoom +"),
        ("far", "Zoom −"),
        ("reset", "Reset"),
    ]
    .into_iter()
    .enumerate()
    {
        nodes.push(elements::button(
            id,
            UiRectangle {
                x: 20. + i as f64 * 110.,
                y: 24.,
                width: 104.,
                height: 32.,
            },
            label,
            id,
        ));
    }
    diagnostics::nodes(&mut nodes);
    UiNativePage::new(UiNativePageV1 {
        version: 1,
        title: "E3DN · native multi-mesh scene".into(),
        width: 1024.,
        height: 768.,
        nodes,
    })
}
fn initialize<'a>(
    window: &'a UiNativeWindow,
    instance: &gpu::Instance,
) -> Result<NativeRenderer<'a>> {
    let surface = window.graphics_surface(instance)?;
    let mut future = std::pin::pin!(NativeRenderer::new(
        instance,
        surface,
        RenderLimits::default()
    ));
    let mut context = Context::from_waker(Waker::noop());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return Ok(result?);
        }
        if !window.pump(Duration::from_millis(2))? || Instant::now() >= deadline {
            return Err("GPU initialization interrupted/timed out".into());
        }
    }
}
fn close(window: &UiNativeWindow, renderer: &mut NativeRenderer<'_>) -> Result<()> {
    renderer.begin_close();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match renderer.finish_close() {
            Ok(()) => return Ok(()),
            Err(RenderError::FramesPending) if Instant::now() < deadline => {
                if let Err(error) = window.pump(Duration::from_millis(2)) {
                    renderer.abort_close();
                    return Err(error.into());
                }
            }
            Err(error) => {
                renderer.abort_close();
                return Err(error.into());
            }
        }
    }
}
pub fn run() -> Result<()> {
    let app = UiNativeApplication::new(1)?;
    let window = UiNativeWindow::open(&app, &page()?, 64, || {})?;
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let frame = assets::scene()?;
    let groups = [&frame.instances()[..256], &frame.instances()[256..]];
    let mut renderer = initialize(&window, &instance)?;
    enable(&mut renderer, &groups)?;
    profile(&renderer);
    let mut metrics = input_metrics::InputMetrics::default();
    let mut performance = diagnostics::Hud::default();
    window.enable_scene_input(diagnostics::OVERLAY)?;
    let mut size = (0, 0);
    let mut yaw = 0.6_f32;
    let mut pitch = 0.65_f32;
    let mut distance = 12_f32;
    let mut motion = true;
    let mut actions = 0;
    let mut frames = 0;
    let mut previous = Instant::now();
    let deadline = diagnostics::deadline()?;
    while deadline.is_none_or(|end| Instant::now() < end)
        && window.pump(Duration::from_millis(1))?
    {
        let dt = previous.elapsed().as_secs_f32().min(0.04);
        previous = Instant::now();
        renderer.poll()?;
        metrics.observe(renderer.frame_progress());
        while let Some(event) = window.next_timed_activation() {
            let action = event.activation;
            match action.action_id.as_str() {
                "pause" => motion = !motion,
                "left" => yaw -= 0.2,
                "right" => yaw += 0.2,
                "near" => distance = (distance - 1.).max(7.),
                "far" => distance = (distance + 1.).min(20.),
                "reset" => {
                    yaw = 0.6;
                    pitch = 0.65;
                    distance = 12.;
                }
                _ => {}
            }
            metrics.applied(event.timing);
            actions += 1;
            window.set_label("pause", if motion { "Pause" } else { "Resume" })?;
            window.set_label(
                "status",
                &format!("320 instances · 2 meshes · current-frame Hi-Z · actions {actions}"),
            )?;
        }
        navigate(
            &window,
            &mut performance,
            dt,
            (&mut yaw, &mut pitch, &mut distance),
            &mut motion,
        )?;
        let next = window.graphics_size()?;
        if next != size {
            match renderer.resize(next.0, next.1) {
                Ok(()) => size = next,
                Err(RenderError::FramesPending) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        if motion {
            yaw += dt * 0.12;
        }
        if size.0 == 0 || size.1 == 0 {
            continue;
        }
        let matrix = camera::matrix(yaw, pitch, distance, size.0 as f32 / size.1 as f32);
        let submitted = Instant::now();
        match renderer.present_meshes(&groups, matrix) {
            Ok(true) => {
                performance.presented(submitted.elapsed());
                frames += 1;
                metrics.presented(renderer.frame_progress().submitted);
            }
            Ok(false) | Err(RenderError::FramesPending) => {}
            Err(RenderError::SurfaceUnavailable) => size = (0, 0),
            Err(error) => return Err(error.into()),
        }
        performance.update(&window, &renderer, size)?;
    }
    close(&window, &mut renderer)?;
    report(&mut metrics, &renderer, size);
    drop(renderer);
    window.close();
    drop(window);
    println!(
        "frames={frames} instances=320 meshes=2 actions={actions} retained_windows={}",
        app.retained_windows()
    );
    Ok(())
}

fn profile(renderer: &NativeRenderer<'_>) {
    let name: String = renderer
        .adapter_info()
        .name
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    println!(
        "profile backend={:?} adapter={name} meshes=2 instances=320 pump_wait_ms=1 timing={}",
        renderer.adapter_info().backend,
        cfg!(feature = "native-input-timing")
    );
}

fn enable(
    renderer: &mut NativeRenderer<'_>,
    groups: &[&[appcore_render::PreparedInstance]; 2],
) -> Result<()> {
    renderer.enable_meshes(&[
        MeshBatchDescriptor {
            seed: &groups[0][0],
            capacity: 256,
        },
        MeshBatchDescriptor {
            seed: &groups[1][0],
            capacity: 64,
        },
    ])?;
    Ok(())
}

fn navigate(
    window: &UiNativeWindow,
    performance: &mut diagnostics::Hud,
    dt: f32,
    (yaw, pitch, distance): (&mut f32, &mut f32, &mut f32),
    motion: &mut bool,
) -> Result<()> {
    let input = window.take_scene_input(Duration::from_secs_f32(dt))?;
    performance.navigation(window, input.active)?;
    if input.orbit != [0.; 2] || input.zoom != 0. {
        if *motion {
            window.set_label("pause", "Resume")?;
        }
        *motion = false;
    }
    *yaw += input.orbit[0];
    *pitch = (*pitch + input.orbit[1]).clamp(-1.45, 1.45);
    *distance = (*distance + input.zoom).clamp(7., 20.);
    Ok(())
}

fn report(
    metrics: &mut input_metrics::InputMetrics,
    renderer: &NativeRenderer<'_>,
    size: (u32, u32),
) {
    metrics.observe(renderer.frame_progress());
    metrics.report();
    println!(
        "profile physical_size={size:?} completed={:?}",
        renderer.frame_progress()
    );
}
