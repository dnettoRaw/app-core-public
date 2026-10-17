// =============================================================================
//        #######
//     ###       ###     F: lorenz_scene_gpu_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use super::*;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        assert!(Instant::now() < deadline, "Metal request timeout");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn history(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &LorenzScene,
) -> Result<Vec<u8>, RenderError> {
    let bytes = 6 * 4096 * 16;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Lorenz history proof"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    let copied = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Lorenz history test copy"),
        size: bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    copy_trails(device, &mut encoder, scene, &copied);
    encoder.copy_buffer_to_buffer(&copied, 0, &buffer, 0, bytes);
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        device
            .poll(wgpu::PollType::Poll)
            .map_err(|_| RenderError::DeviceUnavailable)?;
        match rx.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(_)) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err(RenderError::CaptureFailed)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        if Instant::now() >= deadline {
            buffer.unmap();
            return Err(RenderError::CaptureTimeout);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let mapped = buffer.slice(..).get_mapped_range();
    let result = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    Ok(result)
}
fn copy_trails(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: &LorenzScene,
    output: &wgpu::Buffer,
) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Bit-preserving history test copy"),
        source: wgpu::ShaderSource::Wgsl("@group(0) @binding(0) var<storage, read> input: array<vec4<u32>>; @group(0) @binding(1) var<storage, read_write> output: array<vec4<u32>>; @compute @workgroup_size(64) fn copy(@builtin(global_invocation_id) id: vec3<u32>) { if (id.x < 24576u) { output[id.x] = input[id.x]; } }".into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Lorenz history proof copy"),
        layout: None,
        module: &shader,
        entry_point: Some("copy"),
        compilation_options: Default::default(),
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: scene.compute.trails().as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut pass = encoder.begin_compute_pass(&Default::default());
    pass.set_pipeline(&pipeline);
    pass.set_bind_group(0, &group, &[]);
    pass.dispatch_workgroups(384, 1, 1);
}
fn draw_paused(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &LorenzScene,
    size: (u32, u32),
) -> Result<(), RenderError> {
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Lorenz resize proof"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    scene.draw(
        queue,
        &mut encoder,
        &color.create_view(&Default::default()),
        LorenzView::default(),
        size.0,
        size.1,
    )?;
    queue.submit([encoder.finish()]);
    Ok(())
}
fn prove_ring_wrap(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &mut LorenzScene,
) -> Result<(), RenderError> {
    let deadline = Instant::now() + Duration::from_secs(30);
    for i in 0u32..511 {
        scene.compute.submit_steps(device, queue, 8)?;
        if !i.is_multiple_of(2) || i == 510 {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            queue.on_submitted_work_done(move || {
                let _ = tx.send(());
            });
            loop {
                device
                    .poll(wgpu::PollType::Poll)
                    .map_err(|_| RenderError::DeviceUnavailable)?;
                match rx.try_recv() {
                    Ok(()) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return Err(RenderError::DeviceUnavailable)
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
                if Instant::now() >= deadline {
                    return Err(RenderError::CaptureTimeout);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    assert_eq!(scene.compute.count(), 4096);
    assert_eq!(scene.compute.cursor(), 1);
    let full = history(device, queue, scene)?;
    for entry in full.chunks_exact(16) {
        for component in entry.chunks_exact(4) {
            assert!(f32::from_le_bytes(component.try_into().unwrap()).is_finite());
        }
        assert_eq!(f32::from_le_bytes(entry[12..16].try_into().unwrap()), 1.);
    }
    scene.compute.submit_steps(device, queue, 1)?;
    let next = history(device, queue, scene)?;
    let changed: Vec<_> = full
        .chunks_exact(16)
        .zip(next.chunks_exact(16))
        .enumerate()
        .filter_map(|(index, (old, new))| (old != new).then_some(index))
        .collect();
    assert_eq!(
        changed,
        (0..6)
            .map(|trajectory| trajectory * 4096 + 1)
            .collect::<Vec<_>>()
    );
    assert_eq!(scene.compute.count(), 4096);
    assert_eq!(scene.compute.cursor(), 2);
    Ok(())
}
#[test]
#[ignore = "requires real Metal; explicit diagnostic readback, not an interactive path"]
fn depth_resize_and_paused_drawing_preserve_actual_gpu_trail_bytes(
) -> Result<(), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = ready(instance.request_adapter(&Default::default()))?;
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let (device, queue) = ready(adapter.request_device(&Default::default()))?;
    device.on_uncaptured_error(Box::new(|error| {
        panic!("GPU history proof failed: {error}")
    }));
    let limits = RenderLimits::default();
    let mut scene = LorenzScene::new(&device, wgpu::TextureFormat::Rgba8Unorm, limits, 64, 64)?;
    scene.compute.submit_steps(&device, &queue, 8)?;
    let before = history(&device, &queue, &scene)?;
    assert!(before.iter().any(|byte| *byte != 0));
    for size in [(128, 96), (32, 64), (64, 64)] {
        scene.resize(&device, limits, size.0, size.1)?;
        draw_paused(&device, &queue, &scene, size)?;
        assert_eq!(history(&device, &queue, &scene)?, before);
        assert_eq!(scene.compute.count(), 8);
    }
    let previous = (scene.depth.width(), scene.depth.height());
    assert_eq!(
        scene.resize(
            &device,
            RenderLimits {
                managed_bytes: LorenzScene::BYTES + 128 * 96 * 8,
                ..limits
            },
            128,
            96
        ),
        Err(RenderError::BudgetExceeded)
    );
    assert_eq!((scene.depth.width(), scene.depth.height()), previous);
    assert_eq!(history(&device, &queue, &scene)?, before);
    scene.compute.submit_steps(&device, &queue, 1)?;
    assert_ne!(history(&device, &queue, &scene)?, before);
    assert_eq!(scene.compute.count(), 9);
    prove_ring_wrap(&device, &queue, &mut scene)?;
    Ok(())
}
