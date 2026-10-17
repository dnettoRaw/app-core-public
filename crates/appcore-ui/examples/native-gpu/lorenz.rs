// =============================================================================
//        #######
//     ###       ###     F: lorenz.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use appcore_render::{
    gpu, LorenzParameters, LorenzView, NativeRenderer, RenderError, RenderLimits,
};
use appcore_ui::{
    elements, UiNativeApplication, UiNativePage, UiNativePageV1, UiNativeWindow, UiRectangle,
};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
#[path = "diagnostics.rs"]
mod diagnostics;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
struct Controls {
    view: LorenzView,
    motion: bool,
    reset: bool,
    step: bool,
    warm: u32,
    preset: u32,
    parameters: Option<LorenzParameters>,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            view: Default::default(),
            motion: true,
            reset: false,
            step: false,
            warm: 2048,
            preset: 0,
            parameters: None,
        }
    }
}
impl Controls {
    fn command(&mut self, action: &str) {
        match action {
            "parameters" => {
                self.preset = (self.preset + 1) % 3;
                self.parameters = Some(match self.preset {
                    1 => LorenzParameters {
                        sigma: 12.0,
                        rho: 35.0,
                        beta: 3.0,
                    },
                    2 => LorenzParameters {
                        sigma: 10.0,
                        rho: 20.0,
                        beta: 8.0 / 3.0,
                    },
                    _ => LorenzParameters::default(),
                });
            }
            "pause" => self.motion = !self.motion,
            "step" => {
                self.motion = false;
                self.step = true;
            }
            "reset" => {
                self.reset = true;
                self.motion = true;
                self.view = Default::default();
            }
            "left" => self.view.yaw -= 0.15,
            "right" => self.view.yaw += 0.15,
            "up" => self.view.pitch = (self.view.pitch - 0.1).max(-1.45),
            "down" => self.view.pitch = (self.view.pitch + 0.1).min(1.45),
            "near" => self.view.distance = (self.view.distance - 1.0).max(7.0),
            "far" => self.view.distance = (self.view.distance + 1.0).min(20.0),
            "trail" => self.view.selected = (self.view.selected + 1) % 6,
            "isolate" => self.view.isolate = !self.view.isolate,
            _ => {}
        }
    }
}
fn page() -> appcore_ui::UiResult<UiNativePage> {
    let mut nodes = vec![elements::text(
        "status",
        UiRectangle {
            x: 24.0,
            y: 70.0,
            width: 900.0,
            height: 30.0,
        },
        "GPU Lorenz · initializing",
    )];
    for (i, (id, label)) in [
        ("pause", "Pause"),
        ("step", "Step"),
        ("reset", "Reset"),
        ("left", "Orbit ←"),
        ("right", "Orbit →"),
        ("up", "Tilt ↑"),
        ("down", "Tilt ↓"),
        ("near", "Zoom +"),
        ("far", "Zoom −"),
        ("trail", "Next trail"),
        ("isolate", "Isolate"),
        ("parameters", "Params"),
    ]
    .into_iter()
    .enumerate()
    {
        nodes.push(elements::button(
            id,
            UiRectangle {
                x: 20.0 + i as f64 * 82.0,
                y: 24.0,
                width: 78.0,
                height: 32.0,
            },
            label,
            id,
        ));
    }
    diagnostics::nodes(&mut nodes);
    UiNativePage::new(UiNativePageV1 {
        version: 1,
        title: "E3DN · native Lorenz compute".into(),
        width: 1024.0,
        height: 768.0,
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
    let mut renderer = initialize(&window, &instance)?;
    renderer.enable_lorenz()?;
    let mut controls = Controls::default();
    let mut performance = diagnostics::Hud::default();
    window.enable_scene_input(diagnostics::OVERLAY)?;
    let mut size = (0, 0);
    let mut frames = 0;
    let mut actions = 0;
    let deadline = diagnostics::deadline()?;
    let mut previous = Instant::now();
    let mut hud = Instant::now();
    let mut accumulator = 0.0f64;
    while deadline.is_none_or(|end| Instant::now() < end)
        && window.pump(Duration::from_millis(1))?
    {
        let now = Instant::now();
        let dt = (now - previous).as_secs_f64().min(0.04);
        previous = now;
        while let Some(action) = window.next_activation() {
            controls.command(&action.action_id);
            window.set_label("pause", if controls.motion { "Pause" } else { "Resume" })?;
            actions += 1;
        }
        let input = window.take_scene_input(Duration::from_secs_f64(dt))?;
        performance.navigation(&window, input.active)?;
        controls.view.yaw += input.orbit[0];
        controls.view.pitch = (controls.view.pitch + input.orbit[1]).clamp(-1.45, 1.45);
        controls.view.distance = (controls.view.distance + input.zoom).clamp(7., 20.);
        let next = window.graphics_size()?;
        if next != size {
            match renderer.resize(next.0, next.1) {
                Ok(()) => size = next,
                Err(RenderError::FramesPending) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        if !apply_pending(&mut renderer, &mut controls, &mut accumulator)? {
            continue;
        }
        let steps = if controls.step || (controls.motion && controls.warm > 0) {
            8
        } else if controls.motion {
            accumulator = (accumulator + dt).min(0.04);
            ((accumulator + 1e-9) / 0.005).floor().min(8.0) as u32
        } else {
            accumulator = 0.0;
            0
        };
        let submitted = Instant::now();
        match renderer.present_lorenz(steps, controls.view) {
            Ok(true) => {
                performance.presented(submitted.elapsed());
                frames += 1;
                controls.step = false;
                if controls.warm > 0 {
                    controls.warm = controls.warm.saturating_sub(steps);
                } else {
                    accumulator = (accumulator - f64::from(steps) * 0.005).max(0.0);
                }
            }
            Ok(false) | Err(RenderError::FramesPending) => {}
            Err(RenderError::SurfaceUnavailable) => size = (0, 0),
            Err(error) => return Err(error.into()),
        }
        performance.update(&window, &renderer, size)?;
        if hud.elapsed() >= Duration::from_millis(150) {
            window.set_label(
                "status",
                &format!(
                    "{} · GPU samples {} / 24576 · trail {} · actions {} · warmup {} · params {}",
                    if controls.motion { "Running" } else { "Paused" },
                    renderer.lorenz_samples() * 6,
                    controls.view.selected + 1,
                    actions,
                    controls.warm,
                    controls.preset
                ),
            )?;
            hud = Instant::now();
        }
    }
    let samples = renderer.lorenz_samples();
    close(&window, &mut renderer)?;
    drop(renderer);
    window.close();
    drop(window);
    println!(
        "frames={frames} samples_per_trail={samples} actions={actions} retained_windows={}",
        app.retained_windows()
    );
    Ok(())
}

fn apply_pending(
    renderer: &mut NativeRenderer<'_>,
    controls: &mut Controls,
    accumulator: &mut f64,
) -> Result<bool> {
    if let Some(parameters) = controls.parameters {
        match renderer.configure_lorenz(parameters) {
            Ok(()) => {
                controls.parameters = None;
                controls.warm = 2048;
                *accumulator = 0.0;
            }
            Err(RenderError::FramesPending) => return Ok(false),
            Err(error) => return Err(error.into()),
        }
    }
    if controls.reset {
        match renderer.reset_lorenz() {
            Ok(()) => {
                controls.reset = false;
                controls.warm = 2048;
                *accumulator = 0.0;
            }
            Err(RenderError::FramesPending) => return Ok(false),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(true)
}
