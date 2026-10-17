// =============================================================================
//        #######
//     ###       ###     F: lod_scene.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
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
#[test]
#[ignore = "requires real Metal; invoke explicitly"]
fn lod_scene_hiz_preserves_detail_residency_and_empty_reset(
) -> Result<(), Box<dyn std::error::Error>> {
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = run(instance.request_adapter(&Default::default()))?;
    let (device, queue) = run(adapter.request_device(&Default::default()))?;
    let frame = assets()?;
    let mut scene = make_scene(&device, &frame)?;
    let (color, _) = targets(&device);
    let target = color.create_view(&Default::default());
    let matrix = [
        1.,
        0.,
        0.,
        0.,
        0.,
        1.,
        0.,
        0.,
        0.,
        0.,
        100. / (-99.9),
        -1.,
        0.,
        0.,
        10. / (-99.9),
        0.,
    ];
    let view = LodView {
        eye: [0.; 3],
        forward: [0., 0., -1.],
        near_distance: 0.1,
        focal_pixels: 256.,
        viewport_half_diagonal: 363.,
        maximum_error_pixels: 32.,
    };
    let groups = [&frame.instances()[..1], &frame.instances()[1..3]];
    let mut encoder = device.create_command_encoder(&Default::default());
    assert_eq!(
        scene.encode(&device, &queue, &mut encoder, &groups, matrix, &target),
        Err(RenderError::InvalidLimits)
    );
    for resident in [true, false, true] {
        scene.set_lod_resident(0, 1, resident)?;
        scene.set_lod_resident(1, 1, resident)?;
        scene.encode_lod(
            &device,
            &queue,
            &mut encoder,
            LodSceneFrame {
                groups: &groups,
                matrix,
                view,
                target: &target,
            },
        )?;
        queue.submit([encoder.finish()]);
        let pixels = capture(&device, &queue, &color)?;
        assert_color(&pixels, 102, 256, 0);
        assert_color(&pixels, 256, 256, 1);
        assert_color(&pixels, 410, 256, 2);
        if resident {
            assert_color(&pixels, 260, 246, 1);
        } else {
            assert_eq!(pixel(&pixels, 260, 246), &[0, 0, 0, 255]);
        }
        encoder = device.create_command_encoder(&Default::default());
    }
    assert_eq!(
        scene.set_lod_resident(0, 2, false),
        Err(RenderError::InvalidResource)
    );
    scene.encode_lod(
        &device,
        &queue,
        &mut encoder,
        LodSceneFrame {
            groups: &[&[], &[]],
            matrix,
            view,
            target: &target,
        },
    )?;
    queue.submit([encoder.finish()]);
    assert_eq!(
        pixel(&capture(&device, &queue, &color)?, 256, 256),
        &[0, 0, 0, 255]
    );
    Ok(())
}
fn pixel(bytes: &[u8], x: usize, y: usize) -> &[u8] {
    let offset = (y * 512 + x) * 4;
    &bytes[offset..offset + 4]
}
fn assert_color(bytes: &[u8], x: usize, y: usize, channel: usize) {
    let color = pixel(bytes, x, y);
    assert!(color[channel] > 100, "{x},{y}: {color:?}");
    for (i, value) in color[..3].iter().enumerate() {
        if i != channel {
            assert_eq!(*value, 0);
        }
    }
    assert_eq!(color[3], 255);
}
fn assets() -> Result<PreparedScene, RenderError> {
    let mut registry = AssetRegistry::new(AssetLimits::default())?;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for y in 0..3 {
        for x in 0..3 {
            vertices.push([x as f32 * 0.3 - 0.3, y as f32 * 0.3 - 0.3, 0.]);
        }
    }
    for y in 0..2 {
        for x in 0..2 {
            let a = y * 3 + x;
            indices.extend([a, a + 1, a + 4, a, a + 4, a + 3]);
        }
    }
    let mesh = |r: &mut AssetRegistry, v: Vec<[f32; 3]>, i: Vec<u32>| {
        r.register_mesh(
            v.into_iter()
                .map(|p| MeshVertex::new(p, [0., 0., 1.]))
                .collect::<Result<Vec<_>, _>>()?,
            i,
        )
    };
    let fine = mesh(&mut registry, vertices, indices)?;
    let medium = mesh(
        &mut registry,
        vec![[0., 0.3, 0.], [0.3, 0., 0.], [0., -0.3, 0.], [-0.3, 0., 0.]],
        vec![0, 1, 2, 0, 2, 3],
    )?;
    let coarse = mesh(
        &mut registry,
        vec![[-0.3, -0.3, 0.], [0.3, -0.3, 0.], [0., 0.3, 0.]],
        vec![0, 1, 2],
    )?;
    let mut world = SceneWorld::new(SceneLimits::default())?;
    for (position, color, id) in [
        ([-1.2, 0., -2.], [1., 0., 0., 1.], fine),
        ([0., 0., -5.], [0., 1., 0., 1.], fine),
        ([6., 0., -10.], [0., 0., 1., 1.], fine),
        ([0.; 3], [1.; 4], medium),
        ([0.; 3], [1.; 4], coarse),
    ] {
        let material = registry.register_material(color)?;
        world.spawn(
            Transform::default().with_position(position)?,
            RenderBinding {
                mesh: id,
                material,
                tint: [1.; 4],
                animation: 0,
                morph: 0,
            },
            None,
        )?;
    }
    registry.prepare(world.snapshot()?)
}
fn targets(device: &gpu::Device) -> (gpu::Texture, gpu::Texture) {
    let create = |format, usage| {
        device.create_texture(&gpu::TextureDescriptor {
            label: None,
            size: gpu::Extent3d {
                width: 512,
                height: 512,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: gpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    (
        create(
            gpu::TextureFormat::Rgba8Unorm,
            gpu::TextureUsages::RENDER_ATTACHMENT | gpu::TextureUsages::COPY_SRC,
        ),
        create(
            gpu::TextureFormat::Depth32Float,
            gpu::TextureUsages::RENDER_ATTACHMENT,
        ),
    )
}
fn capture(
    device: &gpu::Device,
    queue: &gpu::Queue,
    target: &gpu::Texture,
) -> Result<Vec<u8>, RenderError> {
    let buffer = device.create_buffer(&gpu::BufferDescriptor {
        label: None,
        size: 512 * 2048,
        usage: gpu::BufferUsages::COPY_DST | gpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        gpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: gpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(2048),
                rows_per_image: Some(512),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    buffer.slice(..).map_async(gpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
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

fn make_scene(device: &gpu::Device, frame: &PreparedScene) -> Result<GpuMeshScene, RenderError> {
    let levels = [
        MeshLodDescriptor {
            seed: &frame.instances()[0],
            geometric_error: 0.,
        },
        MeshLodDescriptor {
            seed: &frame.instances()[3],
            geometric_error: 0.22,
        },
        MeshLodDescriptor {
            seed: &frame.instances()[4],
            geometric_error: 0.3,
        },
    ];
    let descriptors = [
        LodSceneBatchDescriptor {
            levels: &levels,
            capacity: 1,
        },
        LodSceneBatchDescriptor {
            levels: &levels,
            capacity: 2,
        },
    ];
    let scene = GpuMeshScene::new_lod(
        device,
        &descriptors,
        gpu::TextureFormat::Rgba8Unorm,
        512,
        512,
        RenderLimits::default(),
    )?;
    assert!(matches!(
        GpuMeshScene::new_lod(
            device,
            &descriptors,
            gpu::TextureFormat::Rgba8Unorm,
            512,
            512,
            RenderLimits {
                managed_bytes: scene.managed_bytes() - 1,
                ..Default::default()
            }
        ),
        Err(RenderError::BudgetExceeded)
    ));
    Ok(scene)
}
