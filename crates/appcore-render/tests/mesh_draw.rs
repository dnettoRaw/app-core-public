// =============================================================================
//        #######
//     ###       ###     F: mesh_draw.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

#![cfg(feature = "metal")]
use appcore_render::*;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return result;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
#[test]
#[ignore = "requires real native Metal; invoke explicitly"]
fn actual_indirect_draw_uses_gpu_visibility_color_and_depth(
) -> Result<(), Box<dyn std::error::Error>> {
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = run(instance.request_adapter(&Default::default()))?;
    let (device, queue) = run(adapter.request_device(&Default::default()))?;
    let mut assets = AssetRegistry::new(AssetLimits::default())?;
    let vertices = [[-0.4, -0.4, 0.5], [0.4, -0.4, 0.5], [0.0, 0.4, 0.5]]
        .into_iter()
        .map(|p| MeshVertex::new(p, [0.0, 0.0, 1.0]))
        .collect::<Result<Vec<_>, _>>()?;
    let mesh = assets.register_mesh(vertices, vec![0, 1, 2])?;
    let red = assets.register_material([1.0, 0.0, 0.0, 1.0])?;
    let green = assets.register_material([0.0, 1.0, 0.0, 1.0])?;
    let binding = |material| RenderBinding {
        mesh,
        material,
        tint: [1.0; 4],
        animation: 0,
        morph: 0,
    };
    let mut world = SceneWorld::new(SceneLimits::default())?;
    let a = world.spawn(Transform::default(), binding(red), None)?;
    let b = world.spawn(
        Transform::default().with_position([5.0, 0.0, 0.0])?,
        binding(green),
        None,
    )?;
    let c = world.spawn(
        Transform::default().with_position([0.0, 0.0, 0.4])?,
        binding(green),
        None,
    )?;
    let frame = assets.prepare(world.snapshot()?)?;
    let batch = GpuMeshBatch::new(
        &device,
        &frame.instances()[0],
        8,
        gpu::TextureFormat::Rgba8Unorm,
        RenderLimits::default(),
    )?;
    assert!(matches!(
        GpuMeshBatch::new(
            &device,
            &frame.instances()[0],
            8,
            gpu::TextureFormat::Rgba8Unorm,
            RenderLimits {
                managed_bytes: batch.managed_bytes() - 1,
                ..Default::default()
            }
        ),
        Err(RenderError::BudgetExceeded)
    ));
    let (color, depth) = targets(&device);
    batch.prepare(&device, &queue, frame.instances(), IDENTITY)?;
    let mut encoder = device.create_command_encoder(&Default::default());
    batch.draw(
        &mut encoder,
        &color.create_view(&Default::default()),
        &depth.create_view(&Default::default()),
        true,
    );
    queue.submit([encoder.finish()]);
    let pixels = capture(&device, &queue, &color)?;
    let center = (32 * 64 + 32) * 4;
    assert!(pixels[center] > 100);
    assert_eq!(&pixels[center + 1..center + 4], &[0, 0, 255]);
    assert_eq!(&pixels[..4], &[0, 0, 0, 255]);
    export_image(&pixels)?;
    masked_draw(&device, &queue, &batch, frame.instances(), &color, &depth)?;
    world.despawn(a)?;
    world.despawn(b)?;
    world.despawn(c)?;
    let empty = assets.prepare(world.snapshot()?)?;
    batch.prepare(&device, &queue, empty.instances(), IDENTITY)?;
    let mut encoder = device.create_command_encoder(&Default::default());
    batch.draw(
        &mut encoder,
        &color.create_view(&Default::default()),
        &depth.create_view(&Default::default()),
        true,
    );
    queue.submit([encoder.finish()]);
    assert_eq!(
        &capture(&device, &queue, &color)?[center..center + 4],
        &[0, 0, 0, 255]
    );
    Ok(())
}
fn capture(
    device: &gpu::Device,
    queue: &gpu::Queue,
    color: &gpu::Texture,
) -> Result<Vec<u8>, RenderError> {
    let buffer = device.create_buffer(&gpu::BufferDescriptor {
        label: None,
        size: 64 * 256,
        usage: gpu::BufferUsages::MAP_READ | gpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        color.as_image_copy(),
        gpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: gpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        gpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(gpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        device
            .poll(gpu::PollType::Poll)
            .map_err(|_| RenderError::DeviceUnavailable)?;
        match rx.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(_)) => return Err(RenderError::CaptureFailed),
            Err(_) => {}
        }
        if Instant::now() >= deadline {
            return Err(RenderError::CaptureTimeout);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let mapped = buffer.slice(..).get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    Ok(bytes)
}

fn export_image(pixels: &[u8]) -> Result<(), std::io::Error> {
    let mut ppm = b"P6\n64 64\n255\n".to_vec();
    for pixel in pixels.chunks_exact(4) {
        ppm.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(std::env::temp_dir().join("e3dn-indirect-mesh.ppm"), ppm)?;

    Ok(())
}

fn targets(device: &gpu::Device) -> (gpu::Texture, gpu::Texture) {
    let extent = gpu::Extent3d {
        width: 64,
        height: 64,
        depth_or_array_layers: 1,
    };
    let target = |format, usage| {
        device.create_texture(&gpu::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: gpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let color = target(
        gpu::TextureFormat::Rgba8Unorm,
        gpu::TextureUsages::RENDER_ATTACHMENT | gpu::TextureUsages::COPY_SRC,
    );
    let depth = target(
        gpu::TextureFormat::Depth32Float,
        gpu::TextureUsages::RENDER_ATTACHMENT,
    );

    (color, depth)
}

fn masked_draw(
    device: &gpu::Device,
    queue: &gpu::Queue,
    batch: &GpuMeshBatch,
    frame: &[PreparedInstance],
    color: &gpu::Texture,
    depth: &gpu::Texture,
) -> Result<(), RenderError> {
    let mask = device.create_buffer(&gpu::BufferDescriptor {
        label: Some("test visibility mask"),
        size: frame.len() as u64 * 4,
        usage: gpu::BufferUsages::STORAGE | gpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for flag in [0_u32, 1] {
        let flags: Vec<_> = (0..frame.len()).flat_map(|_| flag.to_le_bytes()).collect();
        queue.write_buffer(&mask, 0, &flags);
        let mut encoder = device.create_command_encoder(&Default::default());
        batch.encode_prepare(device, queue, &mut encoder, frame, IDENTITY, Some(&mask))?;
        batch.draw(
            &mut encoder,
            &color.create_view(&Default::default()),
            &depth.create_view(&Default::default()),
            true,
        );
        queue.submit([encoder.finish()]);
        let pixels = capture(device, queue, color)?;
        let center = (32 * 64 + 32) * 4;
        if flag == 0 {
            assert_eq!(&pixels[center..center + 4], &[0, 0, 0, 255]);
        } else {
            assert!(pixels[center] > 100);
            assert_eq!(&pixels[center + 1..center + 4], &[0, 0, 255]);
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires real native Metal; invoke explicitly"]
fn multi_mesh_scene_composes_and_clears_without_history() -> Result<(), Box<dyn std::error::Error>>
{
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = run(instance.request_adapter(&Default::default()))?;
    let (device, queue) = run(adapter.request_device(&Default::default()))?;
    let frame = multi_mesh_frame()?;
    let scene = multi_mesh_renderer(&device, &frame)?;
    let (color, _) = targets(&device);
    let target = color.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    assert_eq!(
        scene.encode(
            &device,
            &queue,
            &mut encoder,
            &[&frame.instances()[1..], &frame.instances()[..1]],
            IDENTITY,
            &target
        ),
        Err(RenderError::InvalidResource)
    );
    scene.encode(
        &device,
        &queue,
        &mut encoder,
        &[&frame.instances()[..1], &frame.instances()[1..]],
        IDENTITY,
        &target,
    )?;
    queue.submit([encoder.finish()]);
    assert_eq!(
        indirect_count(&device, &queue, scene.draw_arguments(0)?)?,
        1
    );
    assert_eq!(
        indirect_count(&device, &queue, scene.draw_arguments(1)?)?,
        1
    );
    assert!(matches!(
        scene.draw_arguments(2),
        Err(RenderError::InvalidResource)
    ));
    let pixels = capture(&device, &queue, &color)?;
    let center = (32 * 64 + 32) * 4;
    assert!(pixels[center] > 100);
    assert_eq!(&pixels[center + 1..center + 4], &[0, 0, 255]);
    let right = (32 * 64 + 54) * 4;
    assert_eq!(&pixels[right..right + 2], &[0, 0]);
    assert!(pixels[right + 2] > 100);
    assert_disocclusion(&device, &queue, &scene, &frame.instances()[1..], &color)?;
    let mut encoder = device.create_command_encoder(&Default::default());
    scene.encode(
        &device,
        &queue,
        &mut encoder,
        &[&[], &[]],
        IDENTITY,
        &target,
    )?;
    queue.submit([encoder.finish()]);
    assert_eq!(
        indirect_count(&device, &queue, scene.draw_arguments(0)?)?,
        0
    );
    assert_eq!(
        indirect_count(&device, &queue, scene.draw_arguments(1)?)?,
        0
    );
    let cleared = capture(&device, &queue, &color)?;
    assert_eq!(&cleared[center..center + 4], &[0, 0, 0, 255]);
    assert_eq!(&cleared[right..right + 4], &[0, 0, 0, 255]);
    Ok(())
}

fn indirect_count(
    device: &gpu::Device,
    queue: &gpu::Queue,
    source: &gpu::Buffer,
) -> Result<u32, RenderError> {
    let buffer = device.create_buffer(&gpu::BufferDescriptor {
        label: None,
        size: 20,
        usage: gpu::BufferUsages::MAP_READ | gpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(source, 0, &buffer, 0, 20);
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(gpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        device
            .poll(gpu::PollType::Poll)
            .map_err(|_| RenderError::DeviceUnavailable)?;
        match rx.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(_)) => return Err(RenderError::CaptureFailed),
            Err(_) => {}
        }
        if Instant::now() >= deadline {
            return Err(RenderError::CaptureTimeout);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let mapped = buffer.slice(..).get_mapped_range();
    let count = u32::from_le_bytes([mapped[4], mapped[5], mapped[6], mapped[7]]);
    drop(mapped);
    buffer.unmap();
    Ok(count)
}

fn assert_disocclusion(
    device: &gpu::Device,
    queue: &gpu::Queue,
    scene: &GpuMeshScene,
    remaining: &[PreparedInstance],
    color: &gpu::Texture,
) -> Result<(), RenderError> {
    let mut encoder = device.create_command_encoder(&Default::default());
    scene.encode(
        device,
        queue,
        &mut encoder,
        &[&[], remaining],
        IDENTITY,
        &color.create_view(&Default::default()),
    )?;
    queue.submit([encoder.finish()]);
    assert_eq!(indirect_count(device, queue, scene.draw_arguments(0)?)?, 0);
    assert_eq!(indirect_count(device, queue, scene.draw_arguments(1)?)?, 2);
    let pixels = capture(device, queue, color)?;
    let center = (32 * 64 + 32) * 4;
    assert_eq!(&pixels[center..center + 2], &[0, 0]);
    assert!(pixels[center + 2] > 100);
    Ok(())
}

fn multi_mesh_frame() -> Result<PreparedScene, Box<dyn std::error::Error>> {
    let mut assets = AssetRegistry::new(AssetLimits::default())?;
    let make_vertices = |size: f32, z| {
        [[-size, -size, z], [size, -size, z], [0.0, size, z]]
            .into_iter()
            .map(|p| MeshVertex::new(p, [0.0, 0.0, 1.0]))
            .collect::<Result<Vec<_>, _>>()
    };
    let quad = [
        [-0.4, -0.4, 0.3],
        [0.4, -0.4, 0.3],
        [0.4, 0.4, 0.3],
        [-0.4, 0.4, 0.3],
    ]
    .into_iter()
    .map(|p| MeshVertex::new(p, [0.0, 0.0, 1.0]))
    .collect::<Result<Vec<_>, _>>()?;
    let large = assets.register_mesh(quad, vec![0, 1, 2, 0, 2, 3])?;
    let small = assets.register_mesh(make_vertices(0.05, 0.7)?, vec![0, 1, 2])?;
    let red = assets.register_material([1.0, 0.0, 0.0, 1.0])?;
    let blue = assets.register_material([0.0, 0.0, 1.0, 1.0])?;
    let binding = |mesh, material| RenderBinding {
        mesh,
        material,
        tint: [1.0; 4],
        animation: 0,
        morph: 0,
    };
    let mut world = SceneWorld::new(SceneLimits::default())?;
    world.spawn(Transform::default(), binding(large, red), None)?;
    world.spawn(Transform::default(), binding(small, blue), None)?;
    world.spawn(
        Transform::default().with_position([0.7, 0.0, 0.0])?,
        binding(small, blue),
        None,
    )?;
    Ok(assets.prepare(world.snapshot()?)?)
}

fn multi_mesh_renderer(
    device: &gpu::Device,
    frame: &PreparedScene,
) -> Result<GpuMeshScene, Box<dyn std::error::Error>> {
    let descriptors = [
        MeshBatchDescriptor {
            seed: &frame.instances()[0],
            capacity: 4,
        },
        MeshBatchDescriptor {
            seed: &frame.instances()[1],
            capacity: 4,
        },
    ];
    let scene = GpuMeshScene::new(
        device,
        &descriptors,
        gpu::TextureFormat::Rgba8Unorm,
        64,
        64,
        RenderLimits::default(),
    )?;
    assert!(matches!(
        GpuMeshScene::new(
            device,
            &descriptors,
            gpu::TextureFormat::Rgba8Unorm,
            64,
            64,
            RenderLimits {
                managed_bytes: scene.managed_bytes() - 1,
                ..Default::default()
            }
        ),
        Err(RenderError::BudgetExceeded)
    ));
    let exact = GpuMeshScene::new(
        device,
        &descriptors,
        gpu::TextureFormat::Rgba8Unorm,
        64,
        64,
        RenderLimits {
            managed_bytes: scene.managed_bytes(),
            ..Default::default()
        },
    )?;
    assert_eq!(exact.resized_bytes(64, 64)?, scene.managed_bytes());
    assert_eq!(
        exact.resized_bytes(32, 32),
        Err(RenderError::BudgetExceeded)
    );
    assert_eq!(exact.managed_bytes(), scene.managed_bytes());
    Ok(scene)
}
