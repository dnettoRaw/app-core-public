// =============================================================================
//        #######
//     ###       ###     F: scene_mesh.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    BoundingSphere, GpuLodMeshBatch, GpuMeshBatch, LodMeshFrame, LodView, PreparedInstance,
    RenderError,
};
pub(crate) enum SceneMesh {
    Static(GpuMeshBatch),
    Lod(GpuLodMeshBatch),
}
impl SceneMesh {
    pub(crate) fn validate(&self, frame: &[PreparedInstance]) -> Result<(), RenderError> {
        match self {
            Self::Static(b) => b.validate_frame(frame),
            Self::Lod(b) => b.validate_frame(frame),
        }
    }
    pub(crate) fn bounds(
        &self,
        instance: &PreparedInstance,
    ) -> Result<BoundingSphere, RenderError> {
        match self {
            Self::Static(_) => Ok(instance.world_bounds()),
            Self::Lod(b) => b.world_bounds(instance),
        }
    }
    pub(crate) fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &[PreparedInstance],
        camera: ([f32; 16], Option<LodView>),
        mask: Option<&wgpu::Buffer>,
    ) -> Result<(), RenderError> {
        match self {
            Self::Static(b) => b.encode_prepare(device, queue, encoder, frame, camera.0, mask),
            Self::Lod(b) => b.encode_prepare(
                device,
                queue,
                encoder,
                LodMeshFrame {
                    instances: frame,
                    matrix: camera.0,
                    view: camera.1.ok_or(RenderError::InvalidLimits)?,
                    occlusion: mask,
                },
            ),
        }
    }
    pub(crate) fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        clear: bool,
    ) -> Result<(), RenderError> {
        match self {
            Self::Static(b) => {
                b.draw(encoder, target, depth, clear);
                Ok(())
            }
            Self::Lod(b) => b.draw(encoder, target, depth, clear),
        }
    }
    pub(crate) fn arguments(&self) -> &wgpu::Buffer {
        match self {
            Self::Static(b) => b.draw_arguments(),
            Self::Lod(b) => b.draw_arguments(),
        }
    }
    pub(crate) fn set_resident(&mut self, level: usize, resident: bool) -> Result<(), RenderError> {
        match self {
            Self::Lod(b) => b.set_resident(level, resident),
            Self::Static(_) => Err(RenderError::InvalidResource),
        }
    }
}
