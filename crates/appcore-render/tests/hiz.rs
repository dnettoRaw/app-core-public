// =============================================================================
//        #######
//     ###       ###     F: hiz.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

#![cfg(feature = "metal")]
use appcore_render::{gpu, HiZPyramid, RenderError, RenderLimits};
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
fn hiz_admission_counts_padded_mips_before_allocation() {
    assert_eq!(
        HiZPyramid::dimensions(3, 5, RenderLimits::default()),
        Ok((4, 8, 4, 172))
    );
    assert_eq!(
        HiZPyramid::dimensions(
            3,
            5,
            RenderLimits {
                managed_bytes: 171,
                ..Default::default()
            }
        ),
        Err(RenderError::BudgetExceeded)
    );
    assert_eq!(
        HiZPyramid::dimensions(0, 5, RenderLimits::default()),
        Err(RenderError::InvalidLimits)
    );
}
#[test]
#[ignore = "requires real native Metal; invoke explicitly"]
fn metal_max_reduction_preserves_odd_edges_and_far_padding(
) -> Result<(), Box<dyn std::error::Error>> {
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = run(instance.request_adapter(&Default::default()))?;
    let (device, queue) = run(adapter.request_device(&Default::default()))?;
    let pyramid = HiZPyramid::new(&device, 3, 5, RenderLimits::default())?;
    let depth = device.create_texture(&gpu::TextureDescriptor {
        label: None,
        size: gpu::Extent3d {
            width: 3,
            height: 5,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: gpu::TextureDimension::D2,
        format: gpu::TextureFormat::Depth32Float,
        usage: gpu::TextureUsages::RENDER_ATTACHMENT | gpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let pipeline = depth_writer(&device);
    let view = depth.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&gpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[],
            depth_stencil_attachment: Some(gpu::RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(gpu::Operations {
                    load: gpu::LoadOp::Clear(0.25),
                    store: gpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_scissor_rect(0, 0, 1, 1);
        pass.draw(0..3, 0..1);
        pass.set_scissor_rect(2, 4, 1, 1);
        pass.draw(0..3, 0..1);
    }
    pyramid.encode(&device, &mut encoder, &depth)?;
    queue.submit([encoder.finish()]);
    let base = capture(&device, &queue, pyramid.texture(), 0, 4, 8)?;
    for y in 0..8 {
        for x in 0..4 {
            assert_eq!(
                base[y * 4 + x],
                if (x == 0 && y == 0) || (x == 2 && y == 4) {
                    0.75
                } else if x < 3 && y < 5 {
                    0.25
                } else {
                    1.0
                }
            );
        }
    }
    assert_eq!(
        capture(&device, &queue, pyramid.texture(), 1, 2, 4)?,
        [0.75, 1.0, 0.25, 1.0, 1.0, 1.0, 1.0, 1.0]
    );
    assert_eq!(capture(&device, &queue, pyramid.texture(), 3, 1, 1)?, [1.0]);
    Ok(())
}
fn capture(
    device: &gpu::Device,
    queue: &gpu::Queue,
    texture: &gpu::Texture,
    mip: u32,
    width: u32,
    height: u32,
) -> Result<Vec<f32>, RenderError> {
    let buffer = device.create_buffer(&gpu::BufferDescriptor {
        label: None,
        size: u64::from(height) * 256,
        usage: gpu::BufferUsages::COPY_DST | gpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    let mut image = texture.as_image_copy();
    image.mip_level = mip;
    encoder.copy_texture_to_buffer(
        image,
        gpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: gpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(height),
            },
        },
        gpu::Extent3d {
            width,
            height,
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
    let mut values = Vec::new();
    for row in mapped.chunks_exact(256) {
        for pixel in row[..width as usize * 4].chunks_exact(4) {
            values.push(f32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]));
        }
    }
    drop(mapped);
    buffer.unmap();
    Ok(values)
}

fn depth_writer(device: &gpu::Device) -> gpu::RenderPipeline {
    let shader=device.create_shader_module(gpu::ShaderModuleDescriptor{label:None,source:gpu::ShaderSource::Wgsl("@vertex fn vertex(@builtin(vertex_index) id:u32)->@builtin(position)vec4<f32>{let p=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));return vec4(p[id],0.75,1.0);}".into())});
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
