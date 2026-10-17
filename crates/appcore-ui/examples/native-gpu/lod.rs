// =============================================================================
//        #######
//     ###       ###     F: lod.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use appcore_render::{
    gpu, LodSceneBatchDescriptor, LodView, MeshLodDescriptor, NativeRenderer, RenderError,
    RenderLimits,
};
use appcore_ui::{
    elements, UiNativeApplication, UiNativePage, UiNativePageV1, UiNativeWindow, UiRectangle,
};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
#[path = "lod_assets.rs"]
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
        ("quality", "LOD 12px"),
        ("availability", "Fine on"),
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
        title: "E3DN · native GPU LOD scene".into(),
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
    let groups = [&frame.instances()[..256], &frame.instances()[256..320]];
    let mut renderer = initialize(&window, &instance)?;
    enable(&mut renderer, &frame)?;
    profile(&renderer);
    let mut metrics = input_metrics::InputMetrics::default();
    let mut performance = diagnostics::Hud::default();
    window.enable_scene_input(diagnostics::OVERLAY)?;
    let mut size = (0, 0);
    let mut controls = Controls::default();
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
        commands(&window, &mut controls, &mut metrics)?;
        let input = window.take_scene_input(Duration::from_secs_f32(dt))?;
        performance.navigation(&window, input.active)?;
        if input.orbit != [0.; 2] || input.zoom != 0. {
            if controls.motion {
                window.set_label("pause", "Resume")?;
            }
            controls.motion = false;
        }
        controls.yaw += input.orbit[0];
        controls.pitch = (controls.pitch + input.orbit[1]).clamp(-1.45, 1.45);
        controls.distance = (controls.distance + input.zoom).clamp(7., 20.);
        let next = window.graphics_size()?;
        if next != size {
            match renderer.resize(next.0, next.1) {
                Ok(()) => size = next,
                Err(RenderError::FramesPending) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        if !controls.apply_pending(&mut renderer)? {
            continue;
        }
        if controls.motion {
            controls.yaw += dt * 0.12;
        }
        if size.0 == 0 || size.1 == 0 {
            continue;
        }
        let matrix = camera::matrix(
            controls.yaw,
            controls.pitch,
            controls.distance,
            size.0 as f32 / size.1 as f32,
        );
        let submitted = Instant::now();
        match renderer.present_mesh_lods(
            &groups,
            matrix,
            lod_view(
                controls.yaw,
                controls.pitch,
                controls.distance,
                size,
                [4., 12., 32., 96.][controls.quality],
            ),
        ) {
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
    metrics.observe(renderer.frame_progress());
    metrics.report();
    println!(
        "profile physical_size={size:?} completed={:?}",
        renderer.frame_progress()
    );
    drop(renderer);
    window.close();
    drop(window);
    println!(
        "frames={frames} instances=320 meshes=2 actions={} retained_windows={}",
        controls.actions,
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

fn lod_view(yaw: f32, pitch: f32, distance: f32, size: (u32, u32), error: f32) -> LodView {
    let forward = [
        yaw.sin() * pitch.cos(),
        -pitch.sin(),
        -yaw.cos() * pitch.cos(),
    ];
    LodView {
        eye: forward.map(|v| -v * distance),
        forward,
        near_distance: 0.1,
        focal_pixels: size.1 as f32 * 0.5 / (std::f32::consts::PI / 8.).tan(),
        viewport_half_diagonal: ((f64::from(size.0).powi(2) + f64::from(size.1).powi(2)).sqrt()
            * 0.5
            * 1.000001) as f32,
        maximum_error_pixels: error,
    }
}

struct Controls {
    motion: bool,
    yaw: f32,
    pitch: f32,
    distance: f32,
    quality: usize,
    fine: bool,
    pending_fine: Option<bool>,
    actions: u64,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            motion: true,
            yaw: 0.6,
            pitch: 0.65,
            distance: 12.,
            quality: 1,
            fine: true,
            pending_fine: None,
            actions: 0,
        }
    }
}
impl Controls {
    fn command(&mut self, action: &str) -> bool {
        match action {
            "pause" => self.motion = !self.motion,
            "quality" => self.quality = (self.quality + 1) % 4,
            "availability" => {
                self.fine = !self.fine;
                self.pending_fine = Some(self.fine);
            }
            "left" => self.yaw -= 0.2,
            "right" => self.yaw += 0.2,
            "near" => self.distance = (self.distance - 1.).max(7.),
            "far" => self.distance = (self.distance + 1.).min(20.),
            "reset" => {
                self.yaw = 0.6;
                self.pitch = 0.65;
                self.distance = 12.;
            }
            _ => return false,
        }
        self.actions = self.actions.saturating_add(1);
        true
    }
    fn apply_pending(&mut self, renderer: &mut NativeRenderer<'_>) -> Result<bool> {
        let Some(fine) = self.pending_fine else {
            return Ok(true);
        };
        match renderer.set_mesh_lod_resident(0, 0, fine) {
            Ok(()) => {
                renderer.set_mesh_lod_resident(1, 0, fine)?;
                self.pending_fine = None;
                Ok(true)
            }
            Err(RenderError::FramesPending) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}
fn commands(
    window: &UiNativeWindow,
    controls: &mut Controls,
    metrics: &mut input_metrics::InputMetrics,
) -> Result<()> {
    while let Some(event) = window.next_timed_activation() {
        let action = event.activation;
        if !controls.command(&action.action_id) {
            continue;
        }
        window.set_label(
            "quality",
            &format!("LOD {}px", [4., 12., 32., 96.][controls.quality]),
        )?;
        window.set_label(
            "availability",
            if controls.fine { "Fine on" } else { "Fine off" },
        )?;
        metrics.applied(event.timing);
        window.set_label("pause", if controls.motion { "Pause" } else { "Resume" })?;
        window.set_label(
            "status",
            &format!(
                "320 instances · 2 meshes · current-frame Hi-Z · actions {}",
                controls.actions
            ),
        )?;
    }
    Ok(())
}

fn enable(renderer: &mut NativeRenderer<'_>, frame: &appcore_render::PreparedScene) -> Result<()> {
    let spheres = [
        MeshLodDescriptor {
            seed: &frame.instances()[0],
            geometric_error: 0.,
        },
        MeshLodDescriptor {
            seed: &frame.instances()[320],
            geometric_error: 0.5,
        },
    ];
    let cubes = [
        MeshLodDescriptor {
            seed: &frame.instances()[256],
            geometric_error: 0.,
        },
        MeshLodDescriptor {
            seed: &frame.instances()[321],
            geometric_error: 1.2,
        },
    ];
    renderer.enable_mesh_lods(&[
        LodSceneBatchDescriptor {
            levels: &spheres,
            capacity: 256,
        },
        LodSceneBatchDescriptor {
            levels: &cubes,
            capacity: 64,
        },
    ])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lod_camera_matches_render_projection_and_commands_remain_bounded() -> Result<()> {
        let mut controls = Controls::default();
        assert!(!controls.command("unknown"));
        assert_eq!(controls.actions, 0);
        for _ in 0..4 {
            assert!(controls.command("quality"));
        }
        assert_eq!(controls.quality, 1);
        controls.command("availability");
        assert_eq!(controls.pending_fine, Some(false));
        for _ in 0..40 {
            controls.command("near");
        }
        assert_eq!(controls.distance, 7.);
        for _ in 0..40 {
            controls.command("far");
        }
        assert_eq!(controls.distance, 20.);
        controls.command("reset");
        let view = lod_view(controls.yaw, 0.65, controls.distance, (1024, 768), 12.);
        view.validate()?;
        let matrix = camera::matrix(controls.yaw, 0.65, controls.distance, 1024. / 768.);
        let point = std::array::from_fn::<_, 3, _>(|i| view.eye[i] + view.forward[i] * 2.);
        let clip = std::array::from_fn::<_, 4, _>(|row| {
            matrix[row] * point[0]
                + matrix[4 + row] * point[1]
                + matrix[8 + row] * point[2]
                + matrix[12 + row]
        });
        assert!(clip[0].abs() < 0.00001 && clip[1].abs() < 0.00001);
        assert!((clip[3] - 2.).abs() < 0.00001);
        assert!((0.0..1.0).contains(&(clip[2] / clip[3])));
        Ok(())
    }
}
