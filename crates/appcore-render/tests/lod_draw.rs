// =============================================================================
//        #######
//     ###       ###     F: lod_draw.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

#![cfg(feature = "metal")]
use appcore_render::*;
use gpu::util::DeviceExt;
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
fn packed_geometry_draws_gpu_selected_levels_and_restores_missing_detail(
) -> Result<(), Box<dyn std::error::Error>> {
    let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
        backends: gpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = run(instance.request_adapter(&Default::default()))?;
    let (device, queue) = run(adapter.request_device(&Default::default()))?;
    let levels = [
        LodLevel {
            index_count: 24,
            first_index: 0,
            base_vertex: 0,
            geometric_error: 0.,
            resident: true,
        },
        LodLevel {
            index_count: 6,
            first_index: 24,
            base_vertex: 9,
            geometric_error: 0.22,
            resident: true,
        },
        LodLevel {
            index_count: 3,
            first_index: 30,
            base_vertex: 13,
            geometric_error: 0.3,
            resident: true,
        },
    ];
    let mut selection = GpuLodVisibility::new(&device, 3, &levels, RenderLimits::default())?;
    let objects = [[-1.2, 0., -2.], [0., 0., -5.], [6., 0., -10.]].map(|center| LodObject {
        bounds: BoundingSphere {
            center,
            radius: 0.5,
        },
        max_scale: 1.,
    });
    let fixture = Fixture::new(&device, &selection, &objects)?;
    let view = LodView {
        eye: [0.; 3],
        forward: [0., 0., -1.],
        near_distance: 0.1,
        focal_pixels: 256.,
        viewport_half_diagonal: 363.,
        maximum_error_pixels: 32.,
    };
    let frustum = Frustum::from_matrix([
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
    ])?;
    for resident in [true, false, true] {
        selection.set_resident(1, resident)?;
        let mut encoder = device.create_command_encoder(&Default::default());
        selection.encode(
            &device,
            &queue,
            &mut encoder,
            LodVisibilityInput {
                objects: &objects,
                frustum,
                view,
                occlusion: None,
            },
        )?;
        fixture.draw(&mut encoder, &selection)?;
        queue.submit([encoder.finish()]);
        let pixels = capture(&device, &queue, &fixture.target)?;
        assert_pixel(&pixels, 102, [255, 0, 0, 255]);
        assert_pixel(
            &pixels,
            256,
            if resident {
                [0, 255, 0, 255]
            } else {
                [0, 0, 255, 255]
            },
        );
        assert_pixel(&pixels, 410, [0, 0, 255, 255]);
        assert_eq!(&pixels[..4], &[0, 0, 0, 255]);
        export(&pixels, resident)?;
    }
    Ok(())
}
fn assert_pixel(pixels: &[u8], x: usize, expected: [u8; 4]) {
    let offset = (256 * 512 + x) * 4;
    assert_eq!(&pixels[offset..offset + 4], &expected);
}
struct Fixture {
    target: gpu::Texture,
    vertices: gpu::Buffer,
    indices: gpu::Buffer,
    pipeline: gpu::RenderPipeline,
    groups: Vec<gpu::BindGroup>,
}
impl Fixture {
    fn new(
        device: &gpu::Device,
        selection: &GpuLodVisibility,
        objects: &[LodObject],
    ) -> Result<Self, RenderError> {
        let (vertices, indices) = geometry(device);
        let target = device.create_texture(&gpu::TextureDescriptor {
            label: None,
            size: gpu::Extent3d {
                width: 512,
                height: 512,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: gpu::TextureDimension::D2,
            format: gpu::TextureFormat::Rgba8Unorm,
            usage: gpu::TextureUsages::RENDER_ATTACHMENT | gpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let pipeline = pipeline(device);
        let centers: Vec<_> = objects
            .iter()
            .flat_map(|o| {
                o.bounds
                    .center
                    .into_iter()
                    .chain([0.])
                    .flat_map(f32::to_le_bytes)
            })
            .collect();
        let centers = device.create_buffer_init(&gpu::util::BufferInitDescriptor {
            label: None,
            contents: &centers,
            usage: gpu::BufferUsages::STORAGE,
        });
        let mut groups = Vec::new();
        for (level, color) in [[1., 0., 0., 1.], [0., 1., 0., 1.], [0., 0., 1., 1.]]
            .into_iter()
            .enumerate()
        {
            let bytes: Vec<_> = color.into_iter().flat_map(f32::to_le_bytes).collect();
            let tint = device.create_buffer_init(&gpu::util::BufferInitDescriptor {
                label: None,
                contents: &bytes,
                usage: gpu::BufferUsages::UNIFORM,
            });
            groups.push(device.create_bind_group(&gpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    gpu::BindGroupEntry {
                        binding: 0,
                        resource: gpu::BindingResource::Buffer(selection.visible_binding(level)?),
                    },
                    gpu::BindGroupEntry {
                        binding: 1,
                        resource: centers.as_entire_binding(),
                    },
                    gpu::BindGroupEntry {
                        binding: 2,
                        resource: tint.as_entire_binding(),
                    },
                ],
            }));
        }
        Ok(Self {
            target,
            vertices,
            indices,
            pipeline,
            groups,
        })
    }
    fn draw(
        &self,
        encoder: &mut gpu::CommandEncoder,
        selection: &GpuLodVisibility,
    ) -> Result<(), RenderError> {
        let view = self.target.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&gpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(gpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: gpu::Operations {
                    load: gpu::LoadOp::Clear(gpu::Color::BLACK),
                    store: gpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), gpu::IndexFormat::Uint32);
        for (level, group) in self.groups.iter().enumerate() {
            pass.set_bind_group(0, group, &[]);
            pass.draw_indexed_indirect(
                selection.draw_arguments(),
                selection.arguments_offset(level)?,
            );
        }
        Ok(())
    }
}
fn geometry(device: &gpu::Device) -> (gpu::Buffer, gpu::Buffer) {
    let mut vertices = Vec::<f32>::new();
    let mut indices = Vec::<u32>::new();
    for y in 0..3 {
        for x in 0..3 {
            vertices.extend([x as f32 * 0.3 - 0.3, y as f32 * 0.3 - 0.3]);
        }
    }
    for y in 0..2 {
        for x in 0..2 {
            let a = y * 3 + x;
            indices.extend([a, a + 1, a + 4, a, a + 4, a + 3]);
        }
    }
    vertices.extend([0., 0.3, 0.3, 0., 0., -0.3, -0.3, 0.]);
    indices.extend([0, 1, 2, 0, 2, 3]);
    vertices.extend([-0.3, -0.3, 0.3, -0.3, 0., 0.3]);
    indices.extend([0, 1, 2]);
    let vertices: Vec<_> = vertices.into_iter().flat_map(f32::to_le_bytes).collect();
    let indices: Vec<_> = indices.into_iter().flat_map(u32::to_le_bytes).collect();
    (
        device.create_buffer_init(&gpu::util::BufferInitDescriptor {
            label: None,
            contents: &vertices,
            usage: gpu::BufferUsages::VERTEX,
        }),
        device.create_buffer_init(&gpu::util::BufferInitDescriptor {
            label: None,
            contents: &indices,
            usage: gpu::BufferUsages::INDEX,
        }),
    )
}
fn pipeline(device: &gpu::Device) -> gpu::RenderPipeline {
    let shader = device.create_shader_module(gpu::ShaderModuleDescriptor {
        label: None,
        source: gpu::ShaderSource::Wgsl(
            r#"
struct Out { @builtin(position) position:vec4<f32>, @location(0) color:vec4<f32> }
@group(0) @binding(0) var<storage,read> visible:array<u32>;
@group(0) @binding(1) var<storage,read> centers:array<vec4<f32>>;
@group(0) @binding(2) var<uniform> tint:vec4<f32>;
@vertex fn vertex(@location(0) local:vec2<f32>,@builtin(instance_index) instance_id:u32)->Out {
    let center=centers[visible[instance_id]];
    let world=vec3(local+center.xy,center.z);
    var out:Out;out.position=vec4(world.xy,(100.0/(-99.9))*world.z+10.0/(-99.9),-world.z);
    out.color=tint;return out;
}
@fragment fn fragment(in:Out)->@location(0) vec4<f32>{return in.color;}
"#
            .into(),
        ),
    });
    device.create_render_pipeline(&gpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: gpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[gpu::VertexBufferLayout {
                array_stride: 8,
                step_mode: gpu::VertexStepMode::Vertex,
                attributes: &[gpu::VertexAttribute {
                    format: gpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            }],
        },
        fragment: Some(gpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment"),
            compilation_options: Default::default(),
            targets: &[Some(gpu::ColorTargetState {
                format: gpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: gpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    })
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
fn export(pixels: &[u8], resident: bool) -> Result<(), std::io::Error> {
    let mut image = b"P6\n512 512\n255\n".to_vec();
    for pixel in pixels.chunks_exact(4) {
        image.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(
        std::env::temp_dir().join(if resident {
            "e3dn-lod-resident.ppm"
        } else {
            "e3dn-lod-degraded.ppm"
        }),
        image,
    )
}
