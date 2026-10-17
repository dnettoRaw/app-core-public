// =============================================================================
//        #######
//     ###       ###     F: lod_visibility.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    lod::validate_levels, Frustum, LodLevel, LodObject, LodView, RenderError, RenderLimits,
};
/// Same-frame perspective LOD selection inputs; masks follow source-object order.
pub struct LodVisibilityInput<'a> {
    /// Admitted conservative bounds/scale.
    pub objects: &'a [LodObject],
    /// Matching current-camera planes.
    pub frustum: Frustum,
    /// Perspective projection/error budget matching that camera.
    pub view: LodView,
    /// Optional current-frame GPU occlusion flags; caller owns frame/device provenance.
    pub occlusion: Option<&'a wgpu::Buffer>,
}
/// GPU frustum/mask selection, perspective LOD and per-level compacted indirect draw arguments.
/// Supports 1–8 fixed packed geometry levels and 1–65536 instances. No CPU visibility readback.
pub struct GpuLodVisibility {
    capacity: u32,
    stride: u32,
    levels: Vec<LodLevel>,
    objects: wgpu::Buffer,
    visible: wgpu::Buffer,
    arguments: wgpu::Buffer,
    descriptors: wgpu::Buffer,
    controls: wgpu::Buffer,
    unmasked: wgpu::Buffer,
    choices: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
}
impl GpuLodVisibility {
    /// Fixed payload; partitions are aligned for vertex-stage storage-buffer bindings.
    pub fn buffer_bytes(capacity: u32, levels: u32) -> Result<u64, RenderError> {
        if !(1..=65536).contains(&capacity) || !(1..=8).contains(&levels) {
            return Err(RenderError::InvalidLimits);
        }
        Ok(u64::from(capacity) * 36
            + u64::from(capacity.div_ceil(64) * 64) * 4 * u64::from(levels)
            + u64::from(levels) * 52
            + 164)
    }
    /// Reserve before allocation. Packed geometry spans/error bounds remain asset-producer owned.
    /// Coarsest geometry is always resident; unavailable desired detail is explicitly degraded.
    pub fn new(
        device: &wgpu::Device,
        capacity: u32,
        levels: &[LodLevel],
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        limits.validate()?;
        validate_levels(levels)?;
        if Self::buffer_bytes(capacity, levels.len() as u32)? > limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        validate_device(&device.limits(), capacity, levels.len() as u32)?;
        let mut admitted = Vec::new();
        admitted
            .try_reserve_exact(levels.len())
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        admitted.extend_from_slice(levels);
        let stride = capacity.div_ceil(64) * 64;
        let storage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let make = |size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("E3DN LOD visibility"),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("E3DN LOD selection"),
            source: wgpu::ShaderSource::Wgsl(include_str!("lod_visibility.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("E3DN LOD selection"),
            layout: None,
            module: &shader,
            entry_point: Some("select_lod"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Self {
            capacity,
            stride,
            levels: admitted,
            objects: make(u64::from(capacity) * 32, storage),
            visible: make(
                u64::from(stride) * 4 * levels.len() as u64,
                storage | wgpu::BufferUsages::COPY_SRC,
            ),
            arguments: make(
                levels.len() as u64 * 20,
                storage | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::INDIRECT,
            ),
            descriptors: make(levels.len() as u64 * 32, storage),
            controls: make(
                160,
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            ),
            unmasked: make(4, wgpu::BufferUsages::STORAGE),
            choices: make(
                u64::from(capacity) * 4,
                storage | wgpu::BufferUsages::COPY_SRC,
            ),
            pipeline,
        })
    }
    /// Change future selection residency without any GPU writes; never evict the coarsest level.
    /// Keep geometry alive through prior submissions; this flag does not retire resources.
    pub fn set_resident(&mut self, level: usize, resident: bool) -> Result<(), RenderError> {
        if level >= self.levels.len() || (!resident && level == self.levels.len() - 1) {
            return Err(RenderError::InvalidResource);
        }
        self.levels[level].resident = resident;
        Ok(())
    }
    /// Encode once between queue.submit calls, including when multiple encoders share one submit.
    /// Owner admits/submits and retains resources through their completion.
    /// Invalid input is rejected before writes; empty input resets every indirect count.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        mut input: LodVisibilityInput<'_>,
    ) -> Result<(), RenderError> {
        input.view = input.view.validate()?;
        if input.objects.len() > self.capacity as usize {
            return Err(RenderError::InvalidLimits);
        }
        for object in input.objects {
            object.validate()?;
        }
        if input.occlusion.is_some_and(|m| {
            m.size() < input.objects.len().max(1) as u64 * 4
                || !m.usage().contains(wgpu::BufferUsages::STORAGE)
        }) {
            return Err(RenderError::InvalidResource);
        }
        let payload = pack_objects(input.objects)?;
        let (levels, arguments) = pack_levels(&self.levels);
        let mut controls = Vec::with_capacity(160);
        for value in input
            .frustum
            .planes()
            .iter()
            .flatten()
            .copied()
            .chain(input.view.eye)
            .chain([input.view.near_distance])
            .chain(input.view.forward)
            .chain([0.])
            .chain([
                input.view.focal_pixels,
                input.view.viewport_half_diagonal,
                input.view.maximum_error_pixels,
                0.,
            ])
        {
            controls.extend_from_slice(&value.to_le_bytes());
        }
        for value in [
            input.objects.len() as u32,
            self.levels.len() as u32,
            self.stride,
            u32::from(input.occlusion.is_some()),
        ] {
            controls.extend_from_slice(&value.to_le_bytes());
        }
        let group = self.group(device, input.occlusion.unwrap_or(&self.unmasked));
        if !payload.is_empty() {
            queue.write_buffer(&self.objects, 0, &payload);
        }
        queue.write_buffer(&self.descriptors, 0, &levels);
        queue.write_buffer(&self.arguments, 0, &arguments);
        queue.write_buffer(&self.controls, 0, &controls);
        if input.objects.is_empty() {
            return Ok(());
        }
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups((input.objects.len() as u32).div_ceil(64), 1, 1);
        Ok(())
    }
    fn group(&self, device: &wgpu::Device, mask: &wgpu::Buffer) -> wgpu::BindGroup {
        let buffers = [
            &self.objects,
            &self.visible,
            &self.arguments,
            &self.descriptors,
            &self.controls,
            mask,
            &self.choices,
        ];
        let entries: Vec<_> = buffers
            .into_iter()
            .enumerate()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: buffer.as_entire_binding(),
            })
            .collect();
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("E3DN LOD frame"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &entries,
        })
    }
    /// Aligned compacted source-index partition for one level's mesh draw bind group.
    pub fn visible_binding(&self, level: usize) -> Result<wgpu::BufferBinding<'_>, RenderError> {
        self.arguments_offset(level)?;
        Ok(wgpu::BufferBinding {
            buffer: &self.visible,
            offset: level as u64 * u64::from(self.stride) * 4,
            size: std::num::NonZeroU64::new(u64::from(self.stride) * 4),
        })
    }
    /// Indirect buffer; one 20-byte DrawIndexedIndirect record per level.
    pub fn draw_arguments(&self) -> &wgpu::Buffer {
        &self.arguments
    }
    /// Validated byte offset for one draw record.
    pub fn arguments_offset(&self, level: usize) -> Result<u64, RenderError> {
        if level >= self.levels.len() {
            return Err(RenderError::InvalidResource);
        }
        Ok(level as u64 * 20)
    }
    /// Encoded choice per input: low bits level, bit 31 residency degradation, MAX culled.
    /// Only the last submitted input count is valid. Diagnostic readback waits for completion.
    pub fn choices(&self) -> &wgpu::Buffer {
        &self.choices
    }
}
fn pack_objects(objects: &[LodObject]) -> Result<Vec<u8>, RenderError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(objects.len() * 32)
        .map_err(|_| RenderError::CpuAllocationFailed)?;
    for object in objects {
        for value in object.bounds.center.into_iter().chain([
            object.bounds.radius,
            object.max_scale,
            0.,
            0.,
            0.,
        ]) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    Ok(bytes)
}
fn pack_levels(levels: &[LodLevel]) -> (Vec<u8>, Vec<u8>) {
    let mut descriptors = Vec::with_capacity(levels.len() * 32);
    let mut arguments = Vec::with_capacity(levels.len() * 20);
    for level in levels {
        for value in [
            level.index_count,
            level.first_index,
            level.base_vertex,
            u32::from(level.resident),
        ] {
            descriptors.extend_from_slice(&value.to_le_bytes());
        }
        for value in [level.geometric_error, 0., 0., 0.] {
            descriptors.extend_from_slice(&value.to_le_bytes());
        }
        for value in [
            level.index_count,
            0,
            level.first_index,
            level.base_vertex,
            0,
        ] {
            arguments.extend_from_slice(&value.to_le_bytes());
        }
    }
    (descriptors, arguments)
}

fn validate_device(limits: &wgpu::Limits, capacity: u32, levels: u32) -> Result<(), RenderError> {
    let largest = (u64::from(capacity) * 32)
        .max(u64::from(capacity.div_ceil(64) * 64) * 4 * u64::from(levels))
        .max(u64::from(levels) * 32);
    let alignment = limits.min_storage_buffer_offset_alignment;
    if limits.max_storage_buffers_per_shader_stage < 6
        || alignment == 0
        || alignment > 256
        || 256 % alignment != 0
        || largest > u64::from(limits.max_storage_buffer_binding_size)
        || largest > limits.max_buffer_size
        || limits.max_uniform_buffer_binding_size < 160
        || limits.max_compute_invocations_per_workgroup < 64
        || limits.max_compute_workgroup_size_x < 64
        || limits.max_compute_workgroups_per_dimension < capacity.div_ceil(64)
    {
        return Err(RenderError::DeviceUnavailable);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_rejects_insufficient_binding_and_dispatch_limits() {
        let base = wgpu::Limits::default();
        assert_eq!(validate_device(&base, 65, 3), Ok(()));
        for limits in [
            wgpu::Limits {
                max_storage_buffers_per_shader_stage: 5,
                ..base.clone()
            },
            wgpu::Limits {
                min_storage_buffer_offset_alignment: 512,
                ..base.clone()
            },
            wgpu::Limits {
                max_storage_buffer_binding_size: 1024,
                ..base.clone()
            },
            wgpu::Limits {
                max_buffer_size: 1024,
                ..base.clone()
            },
            wgpu::Limits {
                max_uniform_buffer_binding_size: 128,
                ..base.clone()
            },
            wgpu::Limits {
                max_compute_workgroups_per_dimension: 1,
                ..base.clone()
            },
            wgpu::Limits {
                max_compute_invocations_per_workgroup: 32,
                ..base.clone()
            },
        ] {
            assert_eq!(
                validate_device(&limits, 65, 3),
                Err(RenderError::DeviceUnavailable)
            );
        }
    }
}
