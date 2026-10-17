// =============================================================================
//        #######
//     ###       ###     F: visibility.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{frustum::validate_sphere, BoundingSphere, Frustum, RenderError, RenderLimits};
/// Explicit visibility inputs; supplied mask must come from the same frame/camera and queue.
pub struct VisibilityInput<'a> {
    /// Source world spheres, within the configured capacity.
    pub spheres: &'a [BoundingSphere],
    /// Normalized current-camera planes.
    pub frustum: Frustum,
    /// Triangle mesh index count for indirect arguments.
    pub index_count: u32,
    /// None selects frustum-only; Some consumes current-frame GPU flags in order.
    pub occlusion: Option<&'a wgpu::Buffer>,
}
/// Bounded GPU frustum selection and compacted indirect indexed-draw arguments.
/// Optional current-frame GPU occlusion flags are applied before compaction. Owner admits submissions.
pub struct GpuVisibility {
    capacity: u32,
    spheres: wgpu::Buffer,
    visible: wgpu::Buffer,
    arguments: wgpu::Buffer,
    controls: wgpu::Buffer,
    group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
}
impl GpuVisibility {
    /// Account fixed buffers before allocation; capacity is in 1..65536.
    pub fn buffer_bytes(capacity: u32) -> Result<u64, RenderError> {
        if !(1..=65536).contains(&capacity) {
            return Err(RenderError::InvalidLimits);
        }
        Ok(u64::from(capacity) * 20 + 136)
    }
    /// Construct fixed buffers. Caller passes its remaining managed GPU byte budget.
    pub fn new(
        device: &wgpu::Device,
        capacity: u32,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        limits.validate()?;
        if Self::buffer_bytes(capacity)? > limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        let storage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let make = |size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("E3DN visibility"),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let spheres = make(u64::from(capacity) * 16, storage);
        let visible = make(
            u64::from(capacity) * 4,
            storage | wgpu::BufferUsages::COPY_SRC,
        );
        let arguments = make(
            20,
            storage | wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_SRC,
        );
        let controls = make(
            112,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let unmasked = make(4, wgpu::BufferUsages::STORAGE);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("E3DN frustum culling"),
            source: wgpu::ShaderSource::Wgsl(include_str!("visibility.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("E3DN frustum culling"),
            layout: None,
            module: &shader,
            entry_point: Some("cull"),
            compilation_options: Default::default(),
            cache: None,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("E3DN visibility"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: spheres.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: visible.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: arguments.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: controls.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: unmasked.as_entire_binding(),
                },
            ],
        });
        Ok(Self {
            capacity,
            spheres,
            visible,
            arguments,
            controls,
            group,
            pipeline,
        })
    }
    /// Submit selection for one triangle-mesh batch; validate every sphere before any queue write.
    /// Arguments reset each submission, including an empty frame. Results need GPU completion
    /// before diagnostic readback; normal rendering consumes them directly on the same queue.
    pub fn submit(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        spheres: &[BoundingSphere],
        frustum: Frustum,
        index_count: u32,
    ) -> Result<wgpu::SubmissionIndex, RenderError> {
        let mut encoder = device.create_command_encoder(&Default::default());
        self.encode(
            device,
            queue,
            &mut encoder,
            VisibilityInput {
                spheres,
                frustum,
                index_count,
                occlusion: None,
            },
        )?;
        Ok(queue.submit([encoder.finish()]))
    }
    /// Encode after any current-frame occlusion producer in this encoder. No GPU readback.
    /// Inputs are validated before queue writes; owner submits and retains all frame resources.
    /// Encode this instance once between queue.submit calls: queued writes are shared
    /// by every encoder submitted in that batch. Use distinct visibility instances for prepass and final selection.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        input: VisibilityInput<'_>,
    ) -> Result<(), RenderError> {
        let VisibilityInput {
            spheres,
            frustum,
            index_count,
            occlusion,
        } = input;
        if spheres.len() > self.capacity as usize
            || index_count == 0
            || index_count > 196608
            || !index_count.is_multiple_of(3)
        {
            return Err(RenderError::InvalidLimits);
        }
        for sphere in spheres {
            validate_sphere(*sphere)?;
        }
        if let Some(mask) = occlusion {
            if mask.size() < (spheres.len().max(1) * 4) as u64
                || !mask.usage().contains(wgpu::BufferUsages::STORAGE)
            {
                return Err(RenderError::InvalidLimits);
            }
        }
        let masked = occlusion.map(|mask| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("E3DN masked visibility"),
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.spheres.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.visible.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.arguments.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.controls.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: mask.as_entire_binding(),
                    },
                ],
            })
        });
        let mut payload = Vec::new();
        payload
            .try_reserve_exact(spheres.len() * 16)
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        for sphere in spheres {
            for value in [
                sphere.center[0],
                sphere.center[1],
                sphere.center[2],
                sphere.radius,
            ] {
                payload.extend_from_slice(&value.to_le_bytes());
            }
        }
        let mut controls = Vec::with_capacity(112);
        for plane in frustum.planes() {
            for value in plane {
                controls.extend_from_slice(&value.to_le_bytes());
            }
        }
        for value in [spheres.len() as u32, u32::from(occlusion.is_some()), 0, 0] {
            controls.extend_from_slice(&value.to_le_bytes());
        }
        let arguments = [index_count, 0, 0, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        if !payload.is_empty() {
            queue.write_buffer(&self.spheres, 0, &payload);
        }
        queue.write_buffer(&self.controls, 0, &controls);
        queue.write_buffer(&self.arguments, 0, &arguments);
        if !spheres.is_empty() {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("E3DN frustum culling"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, masked.as_ref().unwrap_or(&self.group), &[]);
            pass.dispatch_workgroups((spheres.len() as u32).div_ceil(64), 1, 1);
        }
        Ok(())
    }
    /// Compacted source-instance indices; order is GPU scheduling-dependent.
    pub fn visible_indices(&self) -> &wgpu::Buffer {
        &self.visible
    }
    /// DrawIndexedIndirect arguments; first_instance and base_vertex are zero.
    pub fn draw_arguments(&self) -> &wgpu::Buffer {
        &self.arguments
    }
}
