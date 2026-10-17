// =============================================================================
//        #######
//     ###       ###     F: scene.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::*;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
mod assets;
mod lod;
mod timestamps;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const MATRIX: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
fn ready<F: Future>(future: F) -> Result<F::Output> {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err("GPU request timeout".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
pub fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).take(2).collect();
    let lod = match args.as_slice() {
        [] => false,
        [flag] if flag == "--lod" => true,
        _ => return Err("use no arguments or --lod".into()),
    };
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = ready(instance.request_adapter(&Default::default()))??;
    let required = gpu::Features::TIMESTAMP_QUERY;
    if adapter.get_info().backend != gpu::Backend::Metal || !adapter.features().contains(required) {
        return Err("Metal timestamps unavailable; profile not qualified".into());
    }
    let (device, queue) = ready(adapter.request_device(&gpu::DeviceDescriptor {
        required_features: required,
        ..Default::default()
    }))??;
    let failed = Arc::new(AtomicBool::new(false));
    let errors = failed.clone();
    device.on_uncaptured_error(Box::new(move |_| {
        errors.store(true, Ordering::Release);
    }));
    let losses = failed.clone();
    device.set_device_lost_callback(move |_, _| {
        losses.store(true, Ordering::Release);
    });
    let info = adapter.get_info();
    let name: String = info
        .name
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    eprintln!("backend={:?} adapter={name} timestamp_period_ns={} timestamp_mode=render_pass_end resolve=after_sampling_completion; ending 1x1 marker included; uploads, queue delay, readback and display excluded", info.backend, queue.get_timestamp_period());
    if lod {
        lod::run(&device, &queue, &failed)?;
        device.destroy();
        return Ok(());
    }
    eprintln!("static profile: warmup=2 samples=7 batches=2 capacity=2048 projection=orthographic triangle_count=3072");
    let frame = assets::frame()?;
    let clock = timestamps::Clock::new(&device, &queue, 1)?;
    println!("width,height,workload,sample,gpu_ns,cpu_encode_ns,visible_front,visible_back,managed_gpu_payload_bytes");
    for (width, height) in [(512, 512), (960, 540), (1920, 1080)] {
        profile_size(&device, &queue, &failed, &clock, &frame, (width, height))?;
    }
    device.destroy();
    Ok(())
}
fn profile_size(
    device: &gpu::Device,
    queue: &gpu::Queue,
    failed: &AtomicBool,
    clock: &timestamps::Clock,
    frame: &PreparedScene,
    size: (u32, u32),
) -> Result<()> {
    let (width, height) = size;
    let extra = u64::from(width) * u64::from(height) * 4 + clock.payload_bytes();
    let limits = RenderLimits::default();
    let scene = GpuMeshScene::new(
        device,
        &[
            MeshBatchDescriptor {
                seed: &frame.instances()[0],
                capacity: 1024,
            },
            MeshBatchDescriptor {
                seed: &frame.instances()[1024],
                capacity: 1024,
            },
        ],
        gpu::TextureFormat::Rgba8Unorm,
        width,
        height,
        RenderLimits {
            managed_bytes: limits
                .managed_bytes
                .checked_sub(extra)
                .ok_or(RenderError::BudgetExceeded)?,
            ..limits
        },
    )?;
    let color = device.create_texture(&gpu::TextureDescriptor {
        label: Some("E3DN profile color"),
        size: gpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: gpu::TextureDimension::D2,
        format: gpu::TextureFormat::Rgba8Unorm,
        usage: gpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target = color.create_view(&Default::default());
    for dense in [false, true] {
        let groups: [&[PreparedInstance]; 2] = if dense {
            [&frame.instances()[..1024], &frame.instances()[1024..]]
        } else {
            [&[], &[]]
        };
        for sample in 0..9 {
            let started = Instant::now();
            let mut encoder = device.create_command_encoder(&Default::default());
            clock.begin(&mut encoder);
            scene.encode(device, queue, &mut encoder, &groups, MATRIX, &target)?;
            clock.end(&mut encoder);
            let encoded = encoder.finish();
            let cpu_ns = started.elapsed().as_nanos();
            queue.submit([encoded]);
            let measurement = clock.capture(
                device,
                queue,
                [scene.draw_arguments(0)?, scene.draw_arguments(1)?],
            )?;
            if failed.load(Ordering::Acquire) {
                return Err(RenderError::DeviceUnavailable.into());
            }
            if (!dense && measurement.visible != [0, 0]) || (dense && measurement.visible[0] == 0) {
                return Err("unexpected indirect counts; GPU profile invalid".into());
            }
            if sample >= 2 {
                println!(
                    "{width},{height},{},{},{:.0},{cpu_ns},{},{},{}",
                    if dense { "grid" } else { "empty" },
                    sample - 2,
                    measurement.gpu_ns,
                    measurement.visible[0],
                    measurement.visible[1],
                    scene.managed_bytes() + extra
                );
            }
        }
    }
    Ok(())
}
