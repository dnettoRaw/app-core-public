// =============================================================================
//        #######
//     ###       ###     F: lorenz_scene.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{LorenzCompute, LorenzView, RenderError, RenderLimits};
pub(crate) struct LorenzScene {
    pub(crate) compute: LorenzCompute,
    controls: wgpu::Buffer,
    group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    depth: wgpu::Texture,
}
impl LorenzScene {
    pub(crate) fn managed_bytes(&self) -> u64 {
        Self::BYTES + u64::from(self.depth.width()) * u64::from(self.depth.height()) * 4
    }
    pub(crate) const BYTES: u64 = LorenzCompute::BUFFER_BYTES + 96;
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        limits: RenderLimits,
        width: u32,
        height: u32,
    ) -> Result<Self, RenderError> {
        Self::admitted_bytes(limits, width, height, None)?;
        let compute = LorenzCompute::new(device, limits)?;
        let controls = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("E3DN Lorenz camera"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("E3DN native Lorenz"),
            source: wgpu::ShaderSource::Wgsl(include_str!("lorenz_draw.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("E3DN native Lorenz"),
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
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("E3DN native Lorenz"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: compute.trails().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: controls.as_entire_binding(),
                },
            ],
        });
        Ok(Self {
            compute,
            controls,
            group,
            pipeline,
            depth: depth_texture(device, width, height),
        })
    }
    pub(crate) fn admitted_bytes(
        limits: RenderLimits,
        width: u32,
        height: u32,
        previous: Option<(u32, u32)>,
    ) -> Result<u64, RenderError> {
        let mut bytes = limits.attachment_bytes(width, height)? + Self::BYTES;
        if let Some((old_width, old_height)) = previous.filter(|size| *size != (width, height)) {
            bytes += limits.attachment_bytes(old_width, old_height)? / 2;
        }
        if bytes > limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        Ok(bytes)
    }
    pub(crate) fn resize_peak_bytes(
        &self,
        limits: RenderLimits,
        width: u32,
        height: u32,
    ) -> Result<u64, RenderError> {
        Self::admitted_bytes(
            limits,
            width,
            height,
            Some((self.depth.width(), self.depth.height())),
        )
    }
    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        limits: RenderLimits,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        self.resize_peak_bytes(limits, width, height)?;
        if (width, height) != (self.depth.width(), self.depth.height()) {
            self.depth = depth_texture(device, width, height);
        }
        Ok(())
    }
    pub(crate) fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        view: LorenzView,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        let matrix = view.matrix(width as f32 / height as f32)?;
        let mut bytes = Vec::with_capacity(96);
        for value in matrix {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [
            self.compute.count(),
            self.compute.cursor(),
            view.selected,
            u32::from(view.isolate),
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [width as f32, height as f32, 0.0, 0.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        queue.write_buffer(&self.controls, 0, &bytes);
        let depth = self.depth.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("E3DN Lorenz draw"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.045,
                        g: 0.075,
                        b: 0.11,
                        a: 1.0,
                    }),
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
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..6, 0..self.compute.count() * 6);
        Ok(())
    }
}
fn depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("E3DN Lorenz depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_admission_counts_old_depth_in_both_directions_and_reuses_equal_size() {
        let limits = RenderLimits::default();
        for (old, new) in [((64, 32), (128, 64)), ((128, 64), (64, 32))] {
            let final_bytes = LorenzScene::BYTES + u64::from(new.0) * u64::from(new.1) * 8;
            let peak = final_bytes + u64::from(old.0) * u64::from(old.1) * 4;
            let tight = RenderLimits {
                managed_bytes: peak - 1,
                ..limits
            };
            assert_eq!(
                LorenzScene::admitted_bytes(tight, new.0, new.1, Some(old)),
                Err(RenderError::BudgetExceeded)
            );
            let exact = RenderLimits {
                managed_bytes: peak,
                ..limits
            };
            assert_eq!(
                LorenzScene::admitted_bytes(exact, new.0, new.1, Some(old)),
                Ok(peak)
            );
            let steady = RenderLimits {
                managed_bytes: final_bytes,
                ..limits
            };
            assert_eq!(
                LorenzScene::admitted_bytes(steady, new.0, new.1, Some(new)),
                Ok(final_bytes)
            );
            assert_eq!(
                LorenzScene::admitted_bytes(steady, new.0, new.1, None),
                Ok(final_bytes)
            );
        }
        assert_eq!(
            LorenzScene::admitted_bytes(limits, 0, 64, None),
            Err(RenderError::InvalidLimits)
        );
        assert_eq!(
            LorenzScene::admitted_bytes(limits, 4097, 64, None),
            Err(RenderError::InvalidLimits)
        );
    }
}

#[cfg(test)]
#[path = "lorenz_scene_gpu_tests.rs"]
mod gpu_tests;
