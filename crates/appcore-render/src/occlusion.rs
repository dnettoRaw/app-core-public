// =============================================================================
//        #######
//     ###       ###     F: occlusion.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    frustum::validate_sphere, BoundingSphere, Frustum, HiZPyramid, RenderError, RenderLimits,
};
/// Explicit same-frame inputs for occlusion. Depth and matrix provenance is caller-owned.
pub struct OcclusionInput<'a> {
    /// Admitted pyramid with matching viewport size.
    pub pyramid: &'a HiZPyramid,
    /// Current-frame, single-sampled depth attachment produced by the same camera.
    pub depth: &'a wgpu::Texture,
    /// Conservative world-space bounds to query.
    pub spheres: &'a [BoundingSphere],
    /// Column-major world-to-clip matrix, with WebGPU 0..1 depth.
    pub matrix: [f32; 16],
}
/// Conservative current-frame Hi-Z queries. One flag per source sphere: 1 visible, 0 occluded.
/// Rebuilds Hi-Z in the same encoder; no temporal history is reused or inferred.
pub struct GpuOcclusion {
    capacity: u32,
    spheres: wgpu::Buffer,
    flags: wgpu::Buffer,
    controls: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}
impl GpuOcclusion {
    /// Fixed query-buffer payload, excluding the separately admitted Hi-Z texture.
    pub fn buffer_bytes(capacity: u32) -> Result<u64, RenderError> {
        if !(1..=65536).contains(&capacity) {
            return Err(RenderError::InvalidLimits);
        }
        Ok(u64::from(capacity) * 20 + 80)
    }
    /// Reserve fixed query storage under the caller's remaining managed GPU budget.
    pub fn new(
        device: &wgpu::Device,
        capacity: u32,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        limits.validate()?;
        if Self::buffer_bytes(capacity)? > limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        let make = |size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("E3DN occlusion"),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let spheres = make(
            u64::from(capacity) * 16,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let flags = make(
            u64::from(capacity) * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let controls = make(
            80,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("E3DN occlusion"),
            entries: &[
                storage(0, true),
                storage(1, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("E3DN occlusion"),
            source: wgpu::ShaderSource::Wgsl(include_str!("occlusion.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("E3DN occlusion"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("query"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Self {
            capacity,
            spheres,
            flags,
            controls,
            pipeline,
            layout,
        })
    }
    /// Encode fresh Hi-Z and conservative queries after current occluder depth rendering.
    /// Depth must belong to this frame/camera and the same device. Caller owns that provenance,
    /// encoder submission and frame admission. Camera/near-plane crossings stay visible.
    pub fn encode_current(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        input: OcclusionInput<'_>,
    ) -> Result<(), RenderError> {
        self.encode_query(device, queue, encoder, input, true)
    }
    /// Internal graph boundary: the scene has already rebuilt this frame's pyramid.
    pub(crate) fn encode_query(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        input: OcclusionInput<'_>,
        rebuild: bool,
    ) -> Result<(), RenderError> {
        let OcclusionInput {
            pyramid,
            depth,
            spheres,
            matrix,
        } = input;
        if spheres.len() > self.capacity as usize
            || !matrix.iter().all(|v| v.is_finite() && v.abs() <= 1e6)
        {
            return Err(RenderError::InvalidLimits);
        }
        Frustum::from_matrix(matrix)?;
        for sphere in spheres {
            validate_sphere(*sphere)?;
        }
        let mut data = Vec::new();
        data.try_reserve_exact(spheres.len() * 16)
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        for sphere in spheres {
            for value in [
                sphere.center[0],
                sphere.center[1],
                sphere.center[2],
                sphere.radius,
            ] {
                data.extend_from_slice(&value.to_le_bytes());
            }
        }
        let mut controls = matrix
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        for value in [
            spheres.len() as u32,
            depth.width(),
            depth.height(),
            pyramid.levels(),
        ] {
            controls.extend_from_slice(&value.to_le_bytes());
        }
        if rebuild {
            pyramid.encode(device, encoder, depth)?;
        }
        if !data.is_empty() {
            queue.write_buffer(&self.spheres, 0, &data);
        }
        queue.write_buffer(&self.controls, 0, &controls);
        if spheres.is_empty() {
            return Ok(());
        }
        let view = pyramid.view();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("E3DN occlusion"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.spheres.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.flags.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.controls.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups((spheres.len() as u32).div_ceil(64), 1, 1);
        Ok(())
    }
    /// GPU flags valid for the last encoded count after the owning encoder executes.
    pub fn flags(&self) -> &wgpu::Buffer {
        &self.flags
    }
}
