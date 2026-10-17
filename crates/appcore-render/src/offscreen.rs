// =============================================================================
//        #######
//     ###       ###     F: offscreen.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{RenderError, RenderLimits};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

/// Observed device capabilities. This first implementation explicitly requires Metal.
#[derive(Debug, Clone)]
pub struct RenderCapabilities {
    /// Backend actually selected; never an implicit software fallback.
    pub backend: wgpu::Backend,
    /// Physical texture size accepted by the selected device.
    pub max_texture_dimension_2d: u32,
}
/// A single fixed-size offscreen color/depth target with bounded readback staging.
/// Capture is an explicit blocking diagnostic boundary, not an interactive frame loop.
pub struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    color: wgpu::Texture,
    depth: wgpu::Texture,
    staging: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    width: u32,
    height: u32,
    padded_row: u32,
    managed_bytes: u64,
    capabilities: RenderCapabilities,
}
impl OffscreenRenderer {
    /// Request a native Metal device and allocate only after dimension/byte admission.
    pub async fn new(limits: RenderLimits, width: u32, height: u32) -> Result<Self, RenderError> {
        let bytes = limits.attachment_bytes(width, height)?;
        let padded_row = (width * 4).div_ceil(256) * 256;
        let managed_bytes = bytes + u64::from(padded_row) * u64::from(height);
        if managed_bytes > limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        if !cfg!(target_os = "macos") {
            return Err(RenderError::DeviceUnavailable);
        }
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .map_err(|_| RenderError::DeviceUnavailable)?;
        let capabilities = RenderCapabilities {
            backend: adapter.get_info().backend,
            max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d,
        };
        if width > capabilities.max_texture_dimension_2d
            || height > capabilities.max_texture_dimension_2d
        {
            return Err(RenderError::InvalidLimits);
        }
        let required_limits = wgpu::Limits {
            max_texture_dimension_2d: width.max(height).max(2048),
            ..Default::default()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("E3DN offscreen"),
                required_limits,
                ..Default::default()
            })
            .await
            .map_err(|_| RenderError::DeviceUnavailable)?;
        device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = |format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("E3DN target"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = texture(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("E3DN capture"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pipeline = create_pipeline(&device);
        let validation = device.pop_error_scope().await;
        let allocation = device.pop_error_scope().await;
        if validation.is_some() || allocation.is_some() {
            return Err(RenderError::GpuAllocationFailed);
        }
        Ok(Self {
            device,
            queue,
            color,
            depth,
            staging,
            pipeline,
            width,
            height,
            padded_row,
            managed_bytes,
            capabilities,
        })
    }
    /// Actual negotiated capabilities.
    pub fn capabilities(&self) -> &RenderCapabilities {
        &self.capabilities
    }
    /// Color, depth and staging bytes managed by this renderer, excluding driver overhead.
    pub fn managed_bytes(&self) -> u64 {
        self.managed_bytes
    }
    /// Draw a depth-tested triangle, then explicitly wait up to five seconds for RGBA capture.
    /// CPU output is at most width * height * 4 bytes; never call this in an interactive pump.
    pub fn capture_triangle(&mut self) -> Result<Vec<u8>, RenderError> {
        let color = self.color.create_view(&Default::default());
        let depth = self.depth.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("E3DN triangle"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.draw(0..3, 0..2);
        }
        encoder.copy_texture_to_buffer(
            self.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::sync_channel(1);
        self.staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.device.poll(wgpu::PollType::Poll).is_err() {
                self.staging.unmap();
                return Err(RenderError::DeviceUnavailable);
            }
            match receiver.try_recv() {
                Ok(Ok(())) => break,
                Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.staging.unmap();
                    return Err(RenderError::CaptureFailed);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                self.staging.unmap();
                return Err(RenderError::CaptureTimeout);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let mapped = self.staging.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in mapped.chunks_exact(self.padded_row as usize) {
            pixels.extend_from_slice(&row[..self.width as usize * 4]);
        }
        drop(mapped);
        self.staging.unmap();
        Ok(pixels)
    }
}

fn create_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("E3DN triangle"),
        source: wgpu::ShaderSource::Wgsl(include_str!("triangle.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("E3DN triangle"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview: None,
        cache: None,
    })
}
