// =============================================================================
//        #######
//     ###       ###     F: mesh_scene.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    scene_mesh::SceneMesh, BoundingSphere, Frustum, GpuLodMeshBatch, GpuMeshBatch, GpuOcclusion,
    HiZPyramid, LodView, MeshLodDescriptor, OcclusionInput, PreparedInstance, RenderError,
    RenderLimits,
};
/// Fixed shared geometry and maximum instance count for one scene batch.
pub struct MeshBatchDescriptor<'a> {
    /// Resolved seed retaining mesh ownership; material must be opaque.
    pub seed: &'a PreparedInstance,
    /// Reserved instance capacity, including currently absent instances.
    pub capacity: u32,
}
/// Fixed packed geometry levels and instance capacity for one LOD scene batch.
pub struct LodSceneBatchDescriptor<'a> {
    /// Finest-to-coarsest resolved assets; producer guarantees geometric errors.
    pub levels: &'a [MeshLodDescriptor<'a>],
    /// Reserved instance count, including currently absent instances.
    pub capacity: u32,
}
/// Same-frame centered-perspective inputs for the LOD scene graph.
pub struct LodSceneFrame<'a> {
    /// Ordered slices referencing each batch's finest seed identity.
    pub groups: &'a [&'a [PreparedInstance]],
    /// Matching world-to-clip matrix.
    pub matrix: [f32; 16],
    /// Physical pixel error budget and matching camera coordinates.
    pub view: LodView,
    /// Caller-owned target matching the scene dimensions/format.
    pub target: &'a wgpu::TextureView,
}
struct Batch {
    prepass: SceneMesh,
    final_pass: SceneMesh,
    query: GpuOcclusion,
}
/// Bounded multi-mesh frame graph: frustum prepass, fresh Hi-Z, queries, indirect draws.
/// Supports up to 64 fixed batches and 65536 reserved instances in total.
/// Each batch has separate prepass/final buffers, avoiding queued parameter aliasing.
/// Caller owns frame admission, target format/size and completion before resize/drop.
pub struct GpuMeshScene {
    batches: Vec<Batch>,
    pre_depth: wgpu::Texture,
    final_depth: wgpu::Texture,
    pyramid: HiZPyramid,
    fixed_bytes: u64,
    bytes: u64,
    limits: RenderLimits,
    lod: bool,
}
impl GpuMeshScene {
    /// Reserve all buffers, both depth targets and padded Hi-Z before GPU allocation.
    /// Caller-owned color/surface targets and driver metadata are excluded.
    pub fn new(
        device: &wgpu::Device,
        descriptors: &[MeshBatchDescriptor<'_>],
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        limits.validate()?;
        if descriptors.is_empty() || descriptors.len() > 64 {
            return Err(RenderError::InvalidLimits);
        }
        let mut fixed_bytes = 0;
        let mut instances = 0_u64;
        for descriptor in descriptors {
            fixed_bytes += GpuMeshBatch::buffer_bytes(descriptor.seed, descriptor.capacity)? * 2
                + GpuOcclusion::buffer_bytes(descriptor.capacity)?;
            instances += u64::from(descriptor.capacity);
            if descriptor.seed.color()[3] != 1.0 {
                return Err(RenderError::InvalidResource);
            }
        }
        if instances > 65536 {
            return Err(RenderError::InvalidLimits);
        }
        let bytes = reservation(fixed_bytes, width, height, limits)?;
        let mut batches = Vec::new();
        batches
            .try_reserve_exact(descriptors.len())
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        for descriptor in descriptors {
            let make =
                || GpuMeshBatch::new(device, descriptor.seed, descriptor.capacity, format, limits);
            batches.push(Batch {
                prepass: SceneMesh::Static(make()?),
                final_pass: SceneMesh::Static(make()?),
                query: GpuOcclusion::new(device, descriptor.capacity, limits)?,
            });
        }
        Ok(Self {
            batches,
            pre_depth: depth(device, width, height),
            final_depth: depth(device, width, height),
            pyramid: HiZPyramid::new(device, width, height, limits)?,
            fixed_bytes,
            bytes,
            limits,
            lod: false,
        })
    }
    /// Admit the complete LOD graph before allocation, including separate prepass/final buffers.
    /// Geometry is currently packed separately per pass and counted twice; no eviction is implied.
    pub fn new_lod(
        device: &wgpu::Device,
        descriptors: &[LodSceneBatchDescriptor<'_>],
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        limits.validate()?;
        if descriptors.is_empty() || descriptors.len() > 64 {
            return Err(RenderError::InvalidLimits);
        }
        let mut fixed_bytes = 0;
        let mut instances = 0u64;
        for descriptor in descriptors {
            fixed_bytes += GpuLodMeshBatch::buffer_bytes(descriptor.levels, descriptor.capacity)?
                * 2
                + GpuOcclusion::buffer_bytes(descriptor.capacity)?;
            instances += u64::from(descriptor.capacity);
        }
        if instances > 65536 {
            return Err(RenderError::InvalidLimits);
        }
        let bytes = reservation(fixed_bytes, width, height, limits)?;
        let mut batches = Vec::new();
        batches
            .try_reserve_exact(descriptors.len())
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        for descriptor in descriptors {
            let make = || {
                GpuLodMeshBatch::new(
                    device,
                    descriptor.levels,
                    descriptor.capacity,
                    format,
                    limits,
                )
            };
            batches.push(Batch {
                prepass: SceneMesh::Lod(make()?),
                final_pass: SceneMesh::Lod(make()?),
                query: GpuOcclusion::new(device, descriptor.capacity, limits)?,
            });
        }
        Ok(Self {
            batches,
            pre_depth: depth(device, width, height),
            final_depth: depth(device, width, height),
            pyramid: HiZPyramid::new(device, width, height, limits)?,
            fixed_bytes,
            bytes,
            limits,
            lod: true,
        })
    }
    /// Change logical level availability in both passes after the caller drains pending frames.
    /// Does not free packed GPU geometry. Invalid batch/level is rejected without mutation.
    pub fn set_lod_resident(
        &mut self,
        batch: usize,
        level: usize,
        resident: bool,
    ) -> Result<(), RenderError> {
        let batch = self
            .batches
            .get_mut(batch)
            .ok_or(RenderError::InvalidResource)?;
        batch.prepass.set_resident(level, resident)?;
        batch.final_pass.set_resident(level, resident)
    }
    /// Accounted buffers/depth/mips, excluding caller-owned color target.
    pub fn managed_bytes(&self) -> u64 {
        self.bytes
    }
    /// Indirect arguments for bounded diagnostic readback after scene submission.
    /// Static batches have one 20-byte record; LOD batches have one per descriptor level.
    /// This is not a synchronous per-frame statistics API; invalid batch indices are rejected.
    pub fn draw_arguments(&self, batch: usize) -> Result<&wgpu::Buffer, RenderError> {
        self.batches
            .get(batch)
            .map(|b| b.final_pass.arguments())
            .ok_or(RenderError::InvalidResource)
    }
    /// Return final reservation after checking old+new texture peak without allocating.
    /// Owner must drain old submissions first; identical dimensions do not allocate.
    pub fn resized_bytes(&self, width: u32, height: u32) -> Result<u64, RenderError> {
        self.resize_peak_bytes(width, height)?;
        reservation(self.fixed_bytes, width, height, self.limits)
    }
    pub(crate) fn resize_peak_bytes(&self, width: u32, height: u32) -> Result<u64, RenderError> {
        let bytes = reservation(self.fixed_bytes, width, height, self.limits)?;
        let peak = if (width, height) == (self.pre_depth.width(), self.pre_depth.height()) {
            bytes
        } else {
            bytes + self.bytes - self.fixed_bytes
        };
        if peak > self.limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        Ok(peak)
    }
    /// Recreate attachments only after the caller has observed GPU completion.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        let bytes = self.resized_bytes(width, height)?;
        if (width, height) == (self.pre_depth.width(), self.pre_depth.height()) {
            return Ok(());
        }
        let pyramid = HiZPyramid::new(device, width, height, self.limits)?;
        self.pre_depth = depth(device, width, height);
        self.final_depth = depth(device, width, height);
        self.pyramid = pyramid;
        self.bytes = bytes;
        Ok(())
    }
    /// Encode each scene once between queue.submit calls. Groups match descriptor order;
    /// each slice contains only that batch's mesh. Empty groups are valid and reset counts.
    /// All groups are validated before writes; target must match configured size and format.
    /// No temporal history or CPU visibility readback is used.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        groups: &[&[PreparedInstance]],
        matrix: [f32; 16],
        target: &wgpu::TextureView,
    ) -> Result<(), RenderError> {
        if self.lod {
            return Err(RenderError::InvalidLimits);
        }
        self.encode_view(device, queue, encoder, groups, (matrix, None), target)
    }
    /// Encode the LOD frame graph using the same camera/error budget in prepass and final draw.
    /// Groups follow new_lod descriptors; no temporal history or CPU selection readback is used.
    pub fn encode_lod(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: LodSceneFrame<'_>,
    ) -> Result<(), RenderError> {
        if !self.lod {
            return Err(RenderError::InvalidLimits);
        }
        let view = frame.view.validate()?;
        self.encode_view(
            device,
            queue,
            encoder,
            frame.groups,
            (frame.matrix, Some(view)),
            frame.target,
        )
    }
    fn encode_view(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        groups: &[&[PreparedInstance]],
        camera: ([f32; 16], Option<LodView>),
        target: &wgpu::TextureView,
    ) -> Result<(), RenderError> {
        let matrix = camera.0;
        if groups.len() != self.batches.len()
            || !matrix.iter().all(|v| v.is_finite() && v.abs() <= 1e6)
        {
            return Err(RenderError::InvalidLimits);
        }
        Frustum::from_matrix(matrix)?;
        let mut bounds = Vec::new();
        bounds
            .try_reserve_exact(groups.len())
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        for (batch, group) in self.batches.iter().zip(groups) {
            batch.prepass.validate(group)?;
            let mut spheres = Vec::<BoundingSphere>::new();
            spheres
                .try_reserve_exact(group.len())
                .map_err(|_| RenderError::CpuAllocationFailed)?;
            for instance in *group {
                spheres.push(batch.prepass.bounds(instance)?);
            }
            bounds.push(spheres);
        }
        let pre_depth = self.pre_depth.create_view(&Default::default());
        for (index, (batch, group)) in self.batches.iter().zip(groups).enumerate() {
            batch
                .prepass
                .prepare(device, queue, encoder, group, camera, None)?;
            batch
                .prepass
                .draw(encoder, target, &pre_depth, index == 0)?;
        }
        self.pyramid.encode(device, encoder, &self.pre_depth)?;
        for ((batch, group), spheres) in self.batches.iter().zip(groups).zip(&bounds) {
            batch.query.encode_query(
                device,
                queue,
                encoder,
                OcclusionInput {
                    pyramid: &self.pyramid,
                    depth: &self.pre_depth,
                    spheres,
                    matrix,
                },
                false,
            )?;
            batch.final_pass.prepare(
                device,
                queue,
                encoder,
                group,
                camera,
                Some(batch.query.flags()),
            )?;
        }
        let final_depth = self.final_depth.create_view(&Default::default());
        for (index, batch) in self.batches.iter().enumerate() {
            batch
                .final_pass
                .draw(encoder, target, &final_depth, index == 0)?;
        }
        Ok(())
    }
}
fn reservation(
    fixed: u64,
    width: u32,
    height: u32,
    limits: RenderLimits,
) -> Result<u64, RenderError> {
    let depths = limits.attachment_bytes(width, height)?;
    let (_, _, _, mips) = HiZPyramid::dimensions(width, height, limits)?;
    let bytes = fixed + depths + mips;
    if bytes > limits.managed_bytes {
        return Err(RenderError::BudgetExceeded);
    }
    Ok(bytes)
}
fn depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("E3DN scene depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
