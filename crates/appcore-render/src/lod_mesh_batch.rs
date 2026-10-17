// =============================================================================
//        #######
//     ###       ###     F: lod_mesh_batch.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    asset_data::{world_bounds, MeshAsset},
    lod_geometry, mesh_pipeline, BoundingSphere, Frustum, GpuLodVisibility, LodObject, LodView,
    LodVisibilityInput, MeshLodDescriptor, PreparedInstance, RenderError, RenderLimits, ResourceId,
};
use std::sync::Arc;
use wgpu::util::DeviceExt;
/// Explicit same-frame inputs for a packed static mesh LOD batch.
pub struct LodMeshFrame<'a> {
    /// Instances bound to the finest seed mesh, in source-index order.
    pub instances: &'a [PreparedInstance],
    /// Centered perspective world-to-clip matrix matching view.
    pub matrix: [f32; 16],
    /// Matching physical-pixel camera/error budget.
    pub view: LodView,
    /// Optional current-frame GPU occlusion flags in instance order.
    pub occlusion: Option<&'a wgpu::Buffer>,
}
/// Shared packed geometry, instance poses and GPU-selected indexed-indirect draws per LOD.
/// Assets stay owned through registry retirement. Caller owns frame admission and completion.
pub struct GpuLodMeshBatch {
    mesh_id: ResourceId,
    _owners: Vec<Arc<MeshAsset>>,
    bounds: BoundingSphere,
    capacity: u32,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    camera: wgpu::Buffer,
    visibility: GpuLodVisibility,
    pipeline: wgpu::RenderPipeline,
    groups: Vec<wgpu::BindGroup>,
    bytes: u64,
}
impl GpuLodMeshBatch {
    /// Complete fixed GPU buffer reservation, excluding targets/pipelines/driver overhead.
    pub fn buffer_bytes(
        descriptors: &[MeshLodDescriptor<'_>],
        capacity: u32,
    ) -> Result<u64, RenderError> {
        let geometry = lod_geometry::plan(descriptors)?;
        Ok(geometry.bytes
            + GpuLodVisibility::buffer_bytes(capacity, descriptors.len() as u32)?
            + u64::from(capacity) * 64
            + 64)
    }
    /// Admit all packed geometry and frame buffers before uploading. All levels begin available.
    pub fn new(
        device: &wgpu::Device,
        descriptors: &[MeshLodDescriptor<'_>],
        capacity: u32,
        format: wgpu::TextureFormat,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        limits.validate()?;
        let plan = lod_geometry::plan(descriptors)?;
        let bytes = plan.bytes
            + GpuLodVisibility::buffer_bytes(capacity, descriptors.len() as u32)?
            + u64::from(capacity) * 64
            + 64;
        if bytes > limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(RenderError::InvalidResource);
        }
        let caps = device.limits();
        let poses = u64::from(capacity) * 64;
        if poses > u64::from(caps.max_storage_buffer_binding_size)
            || plan.vertex_bytes.max(plan.index_bytes).max(poses) > caps.max_buffer_size
            || caps.max_vertex_buffer_array_stride < 24
            || caps.max_vertex_attributes < 2
        {
            return Err(RenderError::DeviceUnavailable);
        }
        let packed = lod_geometry::pack(descriptors)?;
        let visibility = GpuLodVisibility::new(device, capacity, &plan.levels, limits)?;
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("E3DN packed LOD vertices"),
            contents: &packed.vertices,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("E3DN packed LOD indices"),
            contents: &packed.indices,
            usage: wgpu::BufferUsages::INDEX,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("E3DN LOD poses"),
            size: u64::from(capacity) * 64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("E3DN LOD camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pipeline = mesh_pipeline::pipeline(device, format);
        let mut groups = Vec::new();
        groups
            .try_reserve_exact(descriptors.len())
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        for level in 0..descriptors.len() {
            groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("E3DN LOD draw"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: instances.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(visibility.visible_binding(level)?),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: camera.as_entire_binding(),
                    },
                ],
            }));
        }
        Ok(Self {
            mesh_id: descriptors[0].seed.mesh_id(),
            _owners: packed.owners,
            bounds: plan.bounds,
            capacity,
            vertices,
            indices,
            instances,
            camera,
            visibility,
            pipeline,
            groups,
            bytes,
        })
    }
    pub(crate) fn draw_arguments(&self) -> &wgpu::Buffer {
        self.visibility.draw_arguments()
    }
    /// Fixed payload including packed geometry and every LOD partition.
    pub fn managed_bytes(&self) -> u64 {
        self.bytes
    }
    /// Change availability for future frames. Geometry remains allocated; this is not eviction.
    pub fn set_resident(&mut self, level: usize, resident: bool) -> Result<(), RenderError> {
        self.visibility.set_resident(level, resident)
    }
    /// Conservative sphere enclosing every LOD under the supplied validated transform.
    pub fn world_bounds(&self, instance: &PreparedInstance) -> Result<BoundingSphere, RenderError> {
        if instance.mesh_id() != self.mesh_id {
            return Err(RenderError::InvalidResource);
        }
        Ok(world_bounds(self.bounds, instance.transform()))
    }
    /// Validate identity/opacity/capacity before any writes.
    pub fn validate_frame(&self, frame: &[PreparedInstance]) -> Result<(), RenderError> {
        if frame.len() > self.capacity as usize
            || frame
                .iter()
                .any(|i| i.mesh_id() != self.mesh_id || i.color()[3] != 1.)
        {
            return Err(RenderError::InvalidResource);
        }
        Ok(())
    }
    /// Upload bounded poses and encode GPU selection. Once between queue.submit calls.
    /// Matrix/view/mask must describe this frame; retain resources through draw completion.
    pub fn encode_prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: LodMeshFrame<'_>,
    ) -> Result<(), RenderError> {
        self.validate_frame(frame.instances)?;
        frame.view.validate()?;
        let frustum = Frustum::from_matrix(frame.matrix)?;
        if frame.occlusion.is_some_and(|m| {
            m.size() < frame.instances.len().max(1) as u64 * 4
                || !m.usage().contains(wgpu::BufferUsages::STORAGE)
        }) {
            return Err(RenderError::InvalidResource);
        }
        let mut poses = Vec::new();
        let mut objects = Vec::new();
        poses
            .try_reserve_exact(frame.instances.len() * 64)
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        objects
            .try_reserve_exact(frame.instances.len())
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        for instance in frame.instances {
            let transform = instance.transform();
            let p = transform.position();
            let s = transform.scale();
            for value in [p[0], p[1], p[2], 0.]
                .into_iter()
                .chain(transform.rotation())
                .chain([s[0], s[1], s[2], 0.])
                .chain(instance.color())
            {
                poses.extend_from_slice(&value.to_le_bytes());
            }
            objects.push(LodObject {
                bounds: self.world_bounds(instance)?,
                max_scale: s.into_iter().fold(0., f32::max),
            });
        }
        let camera: Vec<_> = frame
            .matrix
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        if !poses.is_empty() {
            queue.write_buffer(&self.instances, 0, &poses);
        }
        queue.write_buffer(&self.camera, 0, &camera);
        self.visibility.encode(
            device,
            queue,
            encoder,
            LodVisibilityInput {
                objects: &objects,
                frustum,
                view: frame.view,
                occlusion: frame.occlusion,
            },
        )
    }
    /// Set clear=true for the first batch, false to compose later batches.
    /// Encode an actual indexed indirect draw, consuming compacted GPU indices without readback.
    /// Caller owns target/depth attachments and completion before resizing/reusing their resources.
    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        clear: bool,
    ) -> Result<(), RenderError> {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("E3DN indirect mesh"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(1.0)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);

        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for (level, group) in self.groups.iter().enumerate() {
            pass.set_bind_group(0, group, &[]);
            pass.draw_indexed_indirect(
                self.visibility.draw_arguments(),
                self.visibility.arguments_offset(level)?,
            );
        }
        Ok(())
    }
}
