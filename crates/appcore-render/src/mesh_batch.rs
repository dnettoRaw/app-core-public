// =============================================================================
//        #######
//     ###       ###     F: mesh_batch.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    asset_data::MeshAsset, mesh_pipeline, Frustum, GpuVisibility, PreparedInstance, RenderError,
    RenderLimits, ResourceId, VisibilityInput,
};
use std::sync::Arc;
use wgpu::util::DeviceExt;
/// One immutable shared opaque static mesh, with per-instance pose/color and GPU indirect visibility.
/// Mixed-mesh slices are rejected; GpuMeshScene composes multiple batches.
pub struct GpuMeshBatch {
    mesh_id: ResourceId,
    _mesh: Arc<MeshAsset>,
    capacity: u32,
    index_count: u32,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    camera: wgpu::Buffer,
    visibility: GpuVisibility,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    bytes: u64,
}
impl GpuMeshBatch {
    /// Required fixed GPU buffer payload, excluding targets, pipelines and driver overhead.
    pub fn buffer_bytes(instance: &PreparedInstance, capacity: u32) -> Result<u64, RenderError> {
        let visibility = GpuVisibility::buffer_bytes(capacity)?;
        Ok(visibility
            + u64::from(capacity) * 64
            + 64
            + instance.mesh().vertices().len() as u64 * 24
            + instance.mesh().indices().len() as u64 * 4)
    }
    /// Upload shared geometry once and reserve bounded pose/visibility buffers.
    pub fn new(
        device: &wgpu::Device,
        instance: &PreparedInstance,
        capacity: u32,
        format: wgpu::TextureFormat,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        limits.validate()?;
        if instance.color()[3] != 1.0
            || !matches!(
                format,
                wgpu::TextureFormat::Rgba8Unorm
                    | wgpu::TextureFormat::Rgba8UnormSrgb
                    | wgpu::TextureFormat::Bgra8Unorm
                    | wgpu::TextureFormat::Bgra8UnormSrgb
            )
        {
            return Err(RenderError::InvalidResource);
        }
        let bytes = Self::buffer_bytes(instance, capacity)?;
        if bytes > limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        let mut vertex_bytes = Vec::new();
        vertex_bytes
            .try_reserve_exact(instance.mesh().vertices().len() * 24)
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        for vertex in instance.mesh().vertices() {
            for value in vertex.position().into_iter().chain(vertex.normal()) {
                vertex_bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let mut index_bytes = Vec::new();
        index_bytes
            .try_reserve_exact(instance.mesh().indices().len() * 4)
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        for value in instance.mesh().indices() {
            index_bytes.extend_from_slice(&value.to_le_bytes());
        }
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("E3DN shared mesh"),
            contents: &vertex_bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("E3DN shared indices"),
            contents: &index_bytes,
            usage: wgpu::BufferUsages::INDEX,
        });
        let make = |size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("E3DN mesh frame"),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let instances = make(
            u64::from(capacity) * 64,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let camera = make(
            64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let visibility = GpuVisibility::new(device, capacity, limits)?;
        let pipeline = mesh_pipeline::pipeline(device, format);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("E3DN mesh frame"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: instances.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: visibility.visible_indices().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: camera.as_entire_binding(),
                },
            ],
        });
        Ok(Self {
            mesh_id: instance.mesh_id(),
            _mesh: instance.mesh.clone(),
            capacity,
            index_count: instance.mesh().indices().len() as u32,
            vertices,
            indices,
            instances,
            camera,
            visibility,
            pipeline,
            group,
            bytes,
        })
    }
    pub(crate) fn draw_arguments(&self) -> &wgpu::Buffer {
        self.visibility.draw_arguments()
    }
    /// Fixed buffer reservation managed by this batch.
    pub fn managed_bytes(&self) -> u64 {
        self.bytes
    }
    /// Validate the entire frame before writes, upload poses, then submit GPU frustum selection.
    /// Owner's frame permit must cover this submission and the later draw on the same queue.
    pub fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &[PreparedInstance],
        matrix: [f32; 16],
    ) -> Result<(), RenderError> {
        let mut encoder = device.create_command_encoder(&Default::default());
        self.encode_prepare(device, queue, &mut encoder, frame, matrix, None)?;
        queue.submit([encoder.finish()]);
        Ok(())
    }
    /// Encode visibility selection after an optional GPU-produced occlusion mask.
    /// Each flag corresponds to one input instance; zero excludes it from the draw.
    /// Encode each batch once between queue.submit calls. Prepass and final selection
    /// require distinct batches because queue writes precede encoder execution.
    pub fn encode_prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &[PreparedInstance],
        matrix: [f32; 16],
        occlusion: Option<&wgpu::Buffer>,
    ) -> Result<(), RenderError> {
        self.validate_frame(frame)?;
        let frustum = Frustum::from_matrix(matrix)?;
        if occlusion.is_some_and(|mask| {
            mask.size() < (frame.len().max(1) as u64) * 4
                || !mask.usage().contains(wgpu::BufferUsages::STORAGE)
        }) {
            return Err(RenderError::InvalidResource);
        }
        let mut data = Vec::new();
        data.try_reserve_exact(frame.len() * 64)
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        let mut spheres = Vec::new();
        spheres
            .try_reserve_exact(frame.len())
            .map_err(|_| RenderError::CpuAllocationFailed)?;
        for instance in frame {
            let transform = instance.transform();
            let position = transform.position();
            let scale = transform.scale();
            for value in [position[0], position[1], position[2], 0.0]
                .into_iter()
                .chain(transform.rotation())
                .chain([scale[0], scale[1], scale[2], 0.0])
                .chain(instance.color())
            {
                data.extend_from_slice(&value.to_le_bytes());
            }
            spheres.push(instance.world_bounds());
        }
        let camera = matrix
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        if !data.is_empty() {
            queue.write_buffer(&self.instances, 0, &data);
        }
        queue.write_buffer(&self.camera, 0, &camera);
        self.visibility.encode(
            device,
            queue,
            encoder,
            VisibilityInput {
                spheres: &spheres,
                frustum,
                index_count: self.index_count,
                occlusion,
            },
        )?;
        Ok(())
    }
    pub(crate) fn validate_frame(&self, frame: &[PreparedInstance]) -> Result<(), RenderError> {
        if frame.len() > self.capacity as usize
            || frame
                .iter()
                .any(|i| i.mesh_id() != self.mesh_id || i.color()[3] != 1.0)
        {
            return Err(RenderError::InvalidResource);
        }
        Ok(())
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
    ) {
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
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed_indirect(self.visibility.draw_arguments(), 0);
    }
}
