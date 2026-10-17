// =============================================================================
//        #######
//     ###       ###     F: occlusion.rs
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
fn conservative_current_frame_queries_rebuild_after_depth_changes(
) -> Result<(), Box<dyn std::error::Error>> {
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = run(instance.request_adapter(&Default::default()))?;
    let (device, queue) = run(adapter.request_device(&Default::default()))?;
    let pyramid = HiZPyramid::new(&device, 16, 16, RenderLimits::default())?;
    let query = GpuOcclusion::new(&device, 8, RenderLimits::default())?;
    let visibility = GpuVisibility::new(&device, 8, RenderLimits::default())?;
    let depth = device.create_texture(&gpu::TextureDescriptor {
        label: None,
        size: gpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: gpu::TextureDimension::D2,
        format: gpu::TextureFormat::Depth32Float,
        usage: gpu::TextureUsages::RENDER_ATTACHMENT | gpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let writer = depth_writer(&device);
    let spheres = [
        BoundingSphere {
            center: [0.0, 0.0, 0.7],
            radius: 0.1,
        },
        BoundingSphere {
            center: [0.0, 0.0, 0.2],
            radius: 0.1,
        },
        BoundingSphere {
            center: [0.95, 0.0, 0.7],
            radius: 0.1,
        },
        BoundingSphere {
            center: [0.0, 0.0, 0.05],
            radius: 0.1,
        },
        BoundingSphere {
            center: [0.6, 0.0, 0.7],
            radius: 0.05,
        },
    ];
    for (clear, expected) in [(0.3, [0, 1, 1, 1, 1]), (1.0, [1, 1, 1, 1, 1])] {
        let mut encoder = device.create_command_encoder(&Default::default());
        encode_depth(&device, &mut encoder, &depth, &writer, clear);
        query.encode_current(
            &device,
            &queue,
            &mut encoder,
            OcclusionInput {
                pyramid: &pyramid,
                depth: &depth,
                spheres: &spheres,
                matrix: IDENTITY,
            },
        )?;
        visibility.encode(
            &device,
            &queue,
            &mut encoder,
            VisibilityInput {
                spheres: &spheres,
                frustum: Frustum::from_matrix(IDENTITY)?,
                index_count: 36,
                occlusion: Some(query.flags()),
            },
        )?;
        queue.submit([encoder.finish()]);
        assert_eq!(capture(&device, &queue, query.flags())?, expected);
        assert_eq!(
            capture(&device, &queue, visibility.draw_arguments())?,
            [36, expected.iter().sum(), 0, 0, 0]
        );
    }
    let mut encoder = device.create_command_encoder(&Default::default());
    assert!(query
        .encode_current(
            &device,
            &queue,
            &mut encoder,
            OcclusionInput {
                pyramid: &pyramid,
                depth: &depth,
                spheres: &[spheres[0]; 9],
                matrix: IDENTITY
            }
        )
        .is_err());
    assert_eq!(capture(&device, &queue, query.flags())?, [1, 1, 1, 1, 1]);
    Ok(())
}
fn capture(
    device: &gpu::Device,
    queue: &gpu::Queue,
    source: &gpu::Buffer,
) -> Result<Vec<u32>, RenderError> {
    let buffer = device.create_buffer(&gpu::BufferDescriptor {
        label: None,
        size: 20,
        usage: gpu::BufferUsages::COPY_DST | gpu::BufferUsages::MAP_READ,
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
    let values = mapped
        .chunks_exact(4)
        .map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
        .collect();
    drop(mapped);
    buffer.unmap();
    Ok(values)
}

fn depth_writer(device: &gpu::Device) -> gpu::RenderPipeline {
    let shader=device.create_shader_module(gpu::ShaderModuleDescriptor{label:None,source:gpu::ShaderSource::Wgsl("@vertex fn vertex(@builtin(vertex_index) id:u32)->@builtin(position)vec4<f32>{let p=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));return vec4(p[id],0.3,1.0);}".into())});
    device.create_render_pipeline(&gpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: gpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: None,
        primitive: Default::default(),
        depth_stencil: Some(gpu::DepthStencilState {
            format: gpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: gpu::CompareFunction::Always,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview: None,
        cache: None,
    })
}

fn encode_depth(
    device: &gpu::Device,
    encoder: &mut gpu::CommandEncoder,
    depth: &gpu::Texture,
    writer: &gpu::RenderPipeline,
    clear: f64,
) {
    let _ = device;
    let view = depth.create_view(&Default::default());
    let mut pass = encoder.begin_render_pass(&gpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[],
        depth_stencil_attachment: Some(gpu::RenderPassDepthStencilAttachment {
            view: &view,
            depth_ops: Some(gpu::Operations {
                load: gpu::LoadOp::Clear(1.0),
                store: gpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
    });
    if clear < 1.0 {
        pass.set_pipeline(writer);
        pass.set_scissor_rect(4, 4, 8, 8);
        pass.draw(0..3, 0..1);
    }
}
