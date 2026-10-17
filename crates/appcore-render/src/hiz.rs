// =============================================================================
//        #######
//     ###       ###     F: hiz.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{RenderError, RenderLimits};
/// Standard-depth Hi-Z pyramid: max reduction for conservative occlusion.
/// Pads physical dimensions to powers of two with far depth 1, preserving odd edges.
/// This builds depth bounds only; camera/history validity and occlusion queries are separate.
pub struct HiZPyramid {
    texture: wgpu::Texture,
    views: Vec<wgpu::TextureView>,
    groups: Vec<wgpu::BindGroup>,
    copy: wgpu::ComputePipeline,
    reduce: wgpu::ComputePipeline,
    width: u32,
    height: u32,
    bytes: u64,
    viewport: (u32, u32),
}
impl HiZPyramid {
    /// Account padded R32Float mip payload before allocation, against the remaining GPU budget.
    pub fn dimensions(
        width: u32,
        height: u32,
        limits: RenderLimits,
    ) -> Result<(u32, u32, u32, u64), RenderError> {
        limits.validate()?;
        if width == 0
            || height == 0
            || width > limits.max_dimension
            || height > limits.max_dimension
        {
            return Err(RenderError::InvalidLimits);
        }
        let (width, height) = (width.next_power_of_two(), height.next_power_of_two());
        if width > limits.max_dimension || height > limits.max_dimension {
            return Err(RenderError::InvalidLimits);
        }
        let levels = width.max(height).ilog2() + 1;
        let mut bytes = 0;
        for level in 0..levels {
            bytes += u64::from((width >> level).max(1)) * u64::from((height >> level).max(1)) * 4;
        }
        if bytes > limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        Ok((width, height, levels, bytes))
    }
    /// Allocate validated mip views/pipelines without retaining the source depth attachment.
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        let viewport = (width, height);
        let (width, height, levels, bytes) = Self::dimensions(width, height, limits)?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("E3DN Hi-Z"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let copy_layout = layout(device, true);
        let reduce_layout = layout(device, false);
        let pipeline = |source: &'static str, entry, bindings: &wgpu::BindGroupLayout| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("E3DN Hi-Z"),
                bind_group_layouts: &[bindings],
                push_constant_ranges: &[],
            });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("E3DN Hi-Z"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("E3DN Hi-Z"),
                layout: Some(&layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let copy = pipeline(include_str!("hiz_copy.wgsl"), "copy_depth", &copy_layout);
        let reduce = pipeline(include_str!("hiz_reduce.wgsl"), "reduce", &reduce_layout);
        let views = (0..levels)
            .map(|level| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: level,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>();
        let groups = (1..levels)
            .map(|level| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("E3DN Hi-Z reduce"),
                    layout: &reduce.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &views[level as usize - 1],
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&views[level as usize]),
                        },
                    ],
                })
            })
            .collect();
        Ok(Self {
            texture,
            views,
            groups,
            copy,
            reduce,
            width,
            height,
            bytes,
            viewport,
        })
    }
    /// Encode fresh depth copy and every reduction level; caller submits under its frame permit.
    /// Source must be single-sampled depth matching the original viewport dimensions.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::Texture,
    ) -> Result<(), RenderError> {
        if (depth.width(), depth.height()) != self.viewport
            || depth.sample_count() != 1
            || depth.format() != wgpu::TextureFormat::Depth32Float
            || !depth.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(RenderError::InvalidLimits);
        }
        let depth = depth.create_view(&wgpu::TextureViewDescriptor {
            mip_level_count: Some(1),
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("E3DN Hi-Z copy"),
            layout: &self.copy.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.views[0]),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.copy);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
        }
        for (index, group) in self.groups.iter().enumerate() {
            let level = index as u32 + 1;
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.reduce);
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(
                (self.width >> level).max(1).div_ceil(8),
                (self.height >> level).max(1).div_ceil(8),
                1,
            );
        }
        Ok(())
    }
    /// Immutable whole-mip view for later occlusion queries after completion/order dependency.
    pub fn view(&self) -> wgpu::TextureView {
        self.texture.create_view(&Default::default())
    }
    /// Texture for explicit diagnostic mip readback.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
    /// Number of reduction levels, including copied base.
    pub fn levels(&self) -> u32 {
        self.views.len() as u32
    }
    /// Padded physical dimensions.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    /// Accounted mip payload, excluding driver metadata.
    pub fn managed_bytes(&self) -> u64 {
        self.bytes
    }
}

fn layout(device: &wgpu::Device, depth: bool) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("E3DN Hi-Z"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: if depth {
                        wgpu::TextureSampleType::Depth
                    } else {
                        wgpu::TextureSampleType::Float { filterable: false }
                    },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::R32Float,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    })
}
