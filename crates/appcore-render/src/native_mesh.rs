// =============================================================================
//        #######
//     ###       ###     F: native_mesh.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    frame_admission::FramePermit, GpuMeshScene, LodSceneBatchDescriptor, LodSceneFrame, LodView,
    MeshBatchDescriptor, NativeRenderer, PreparedInstance, PresentationState, RenderError,
};
use std::sync::atomic::Ordering;
impl NativeRenderer<'_> {
    /// Initialize fixed opaque mesh batches after all frames drain.
    /// Lorenz and mesh scenes are mutually exclusive in this initial presentation profile.
    /// Whole-scene admission includes both depths, Hi-Z, buffers and the surface color target.
    pub fn enable_meshes(
        &mut self,
        descriptors: &[MeshBatchDescriptor<'_>],
    ) -> Result<(), RenderError> {
        self.poll()?;
        if self.meshes.is_some() {
            return Err(RenderError::InvalidLimits);
        }
        if self.inflight() != 0 {
            return Err(RenderError::FramesPending);
        }
        // Existing Lorenz is rejected by the budget contract before creating new resources.
        self.ensure_mesh_profile()?;
        let color_bytes = u64::from(self.config.width) * u64::from(self.config.height) * 4;
        let mut limits = self.limits;
        limits.managed_bytes = limits
            .managed_bytes
            .checked_sub(color_bytes)
            .filter(|b| *b > 0)
            .ok_or(RenderError::BudgetExceeded)?;
        self.meshes = Some(GpuMeshScene::new(
            &self.device,
            descriptors,
            self.config.format,
            self.config.width,
            self.config.height,
            limits,
        )?);
        self.poll()
    }
    /// Initialize LOD batches with whole-scene admission; resources remain owned by this generation.
    pub fn enable_mesh_lods(
        &mut self,
        descriptors: &[LodSceneBatchDescriptor<'_>],
    ) -> Result<(), RenderError> {
        self.poll()?;
        if self.meshes.is_some() {
            return Err(RenderError::InvalidLimits);
        }
        if self.inflight() != 0 {
            return Err(RenderError::FramesPending);
        }
        self.ensure_mesh_profile()?;
        let mut limits = self.limits;
        limits.managed_bytes = limits
            .managed_bytes
            .checked_sub(u64::from(self.config.width) * u64::from(self.config.height) * 4)
            .filter(|b| *b > 0)
            .ok_or(RenderError::BudgetExceeded)?;
        self.meshes = Some(GpuMeshScene::new_lod(
            &self.device,
            descriptors,
            self.config.format,
            self.config.width,
            self.config.height,
            limits,
        )?);
        self.poll()
    }
    /// Logical LOD availability changes require drained frames; no geometry is evicted.
    pub fn set_mesh_lod_resident(
        &mut self,
        batch: usize,
        level: usize,
        resident: bool,
    ) -> Result<(), RenderError> {
        self.poll()?;
        if self.inflight() != 0 {
            return Err(RenderError::FramesPending);
        }
        self.meshes
            .as_mut()
            .ok_or(RenderError::InvalidLimits)?
            .set_lod_resident(batch, level, resident)
    }
    /// Present matching centered-perspective LOD inputs through current-frame Hi-Z.
    pub fn present_mesh_lods(
        &mut self,
        groups: &[&[PreparedInstance]],
        matrix: [f32; 16],
        view: LodView,
    ) -> Result<bool, RenderError> {
        view.validate()?;
        self.present_mesh_frame(groups, matrix, Some(view))
    }
    /// Present fixed groups through current-frame Hi-Z and indexed indirect drawing.
    /// Groups match enable_meshes descriptor order. Caller supplies WebGPU world-to-clip matrix.
    /// Completion admission covers the entire prepass/query/draw graph; no readback is performed.
    pub fn present_meshes(
        &mut self,
        groups: &[&[PreparedInstance]],
        matrix: [f32; 16],
    ) -> Result<bool, RenderError> {
        self.present_mesh_frame(groups, matrix, None)
    }
    fn present_mesh_frame(
        &mut self,
        groups: &[&[PreparedInstance]],
        matrix: [f32; 16],
        view: Option<LodView>,
    ) -> Result<bool, RenderError> {
        self.poll()?;
        if self.meshes.is_none() {
            return Err(RenderError::InvalidLimits);
        }
        if self.state == PresentationState::Suspended {
            return Ok(false);
        }
        let permit = FramePermit::acquire(&self.inflight)?;
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Timeout) => return Ok(false),
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.state = PresentationState::Suspended;
                return Err(RenderError::SurfaceUnavailable);
            }
            Err(_) => {
                self.failed.store(true, Ordering::Release);
                return Err(RenderError::DeviceUnavailable);
            }
        };
        let frame_id = self.next_frame_id()?;
        let scene = self.meshes.as_ref().ok_or(RenderError::InvalidLimits)?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let target = frame.texture.create_view(&Default::default());
        if let Some(view) = view {
            scene.encode_lod(
                &self.device,
                &self.queue,
                &mut encoder,
                LodSceneFrame {
                    groups,
                    matrix,
                    view,
                    target: &target,
                },
            )?;
        } else {
            scene.encode(
                &self.device,
                &self.queue,
                &mut encoder,
                groups,
                matrix,
                &target,
            )?;
        }
        self.queue.submit([encoder.finish()]);
        self.track_frame(frame_id, permit);
        frame.present();
        Ok(true)
    }
}
