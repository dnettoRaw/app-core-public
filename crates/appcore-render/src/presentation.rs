// =============================================================================
//        #######
//     ###       ###     F: presentation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{
    frame_admission::FramePermit, lorenz_scene::LorenzScene, GpuMeshScene, LorenzView, RenderError,
    RenderLimits,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};

/// Explicit native presentation state; zero dimensions suspend submissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationState {
    Suspended,
    Ready,
    Lost,
    Closed,
}
/// One borrowed native surface. The deployment owns the underlying window and pump.
/// At most two submissions may remain pending; completion callbacks release admission.
pub struct NativeRenderer<'window> {
    thread: std::marker::PhantomData<std::rc::Rc<()>>,
    pub(crate) surface: wgpu::Surface<'window>,
    adapter_info: wgpu::AdapterInfo,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) limits: RenderLimits,
    lorenz: Option<LorenzScene>,
    pub(crate) meshes: Option<GpuMeshScene>,
    pub(crate) state: PresentationState,
    aborted: bool,
    pub(crate) failed: Arc<AtomicBool>,
    pub(crate) inflight: Arc<AtomicU32>,
    completion: Arc<crate::frame_progress::FrameCompletion>,
}
impl<'window> NativeRenderer<'window> {
    /// Negotiate a Metal adapter compatible with the supplied surface.
    /// Starts suspended: call resize with validated physical dimensions before presenting.
    pub async fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'window>,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        limits.validate()?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                power_preference: wgpu::PowerPreference::HighPerformance,
            })
            .await
            .map_err(|_| RenderError::DeviceUnavailable)?;
        let adapter_info = adapter.get_info();
        if adapter_info.backend != wgpu::Backend::Metal {
            return Err(RenderError::DeviceUnavailable);
        }
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .ok_or(RenderError::DeviceUnavailable)?;
        if !caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
            return Err(RenderError::DeviceUnavailable);
        }
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .ok_or(RenderError::DeviceUnavailable)?;
        let device_limits = wgpu::Limits {
            max_texture_dimension_2d: limits
                .max_dimension
                .min(adapter.limits().max_texture_dimension_2d),
            ..Default::default()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("E3DN presentation"),
                required_limits: device_limits,
                ..Default::default()
            })
            .await
            .map_err(|_| RenderError::DeviceUnavailable)?;
        let failed = Arc::new(AtomicBool::new(false));
        let errors = failed.clone();
        device.on_uncaptured_error(Box::new(move |_| {
            errors.store(true, Ordering::Release);
        }));
        let losses = failed.clone();
        device.set_device_lost_callback(move |_, _| {
            losses.store(true, Ordering::Release);
        });
        Ok(Self {
            thread: std::marker::PhantomData,
            surface,
            device,
            queue,
            adapter_info,
            limits,
            failed,
            inflight: Arc::new(AtomicU32::new(0)),
            completion: Arc::new(Default::default()),
            lorenz: None,
            meshes: None,
            state: PresentationState::Suspended,
            aborted: false,
            config: wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: 1,
                height: 1,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode,
                view_formats: vec![],
            },
        })
    }
    /// Selected adapter metadata for local qualification; this is not a hardware identity token.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }
    /// Current state, including asynchronous device/error callbacks.
    pub fn state(&self) -> PresentationState {
        if self.state != PresentationState::Closed && self.failed.load(Ordering::Acquire) {
            PresentationState::Lost
        } else {
            self.state
        }
    }
    /// Managed scene payload plus one configured color target, in bytes.
    /// Excludes driver/compositor, extra swapchain images and allocator overhead;
    /// this is budget accounting, not measured VRAM residency.
    pub fn managed_gpu_bytes(&self) -> u64 {
        u64::from(self.config.width) * u64::from(self.config.height) * 4
            + self.meshes.as_ref().map_or(0, GpuMeshScene::managed_bytes)
            + self.lorenz.as_ref().map_or(0, LorenzScene::managed_bytes)
    }
    /// Outstanding submissions observed by queue completion callbacks.
    pub fn inflight(&self) -> u32 {
        self.inflight.load(Ordering::Acquire)
    }
    /// Nonblocking completion observation; IDs are local to this renderer generation.
    /// Call poll to service callbacks. This does not measure display scanout.
    pub fn frame_progress(&self) -> crate::NativeFrameProgress {
        self.completion.snapshot()
    }
    pub(crate) fn next_frame_id(&self) -> Result<u64, RenderError> {
        self.completion.next()
    }
    pub(crate) fn track_frame(&self, id: u64, permit: FramePermit) {
        self.completion.submitted(id);
        let completion = self.completion.clone();
        self.queue.on_submitted_work_done(move || {
            completion.completed(id);
            drop(permit);
        });
    }
    /// Reconfigure physical dimensions only after pending submissions have completed.
    /// Returns backpressure rather than waiting on the UI thread.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        self.poll()?;
        if self.inflight() != 0 {
            return Err(RenderError::FramesPending);
        }
        if width == 0 || height == 0 {
            self.state = PresentationState::Suspended;
            return Ok(());
        }
        self.check_scene_budget(width, height, self.lorenz.is_some())?;
        if width > self.device.limits().max_texture_dimension_2d
            || height > self.device.limits().max_texture_dimension_2d
        {
            return Err(RenderError::InvalidLimits);
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        if let Some(scene) = &mut self.lorenz {
            scene.resize(&self.device, self.limits, width, height)?;
        }
        if let Some(scene) = &mut self.meshes {
            scene.resize(&self.device, width, height)?;
        }
        self.state = PresentationState::Ready;
        self.poll()
    }
    /// Service device callbacks without waiting for GPU completion.
    pub fn poll(&self) -> Result<(), RenderError> {
        if self.state == PresentationState::Closed {
            return Err(RenderError::RendererClosed);
        }
        self.device
            .poll(wgpu::PollType::Poll)
            .map_err(|_| RenderError::DeviceUnavailable)?;
        if self.state() == PresentationState::Lost {
            return Err(RenderError::DeviceUnavailable);
        }
        Ok(())
    }
    /// Submit a clear frame without blocking for readback. Returns whether a frame was presented.
    /// Surface loss requires explicit resize/recreation; timeout skips the current frame.
    pub fn present_clear(&mut self, rgba: [f64; 4]) -> Result<bool, RenderError> {
        if !rgba
            .iter()
            .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
        {
            return Err(RenderError::InvalidLimits);
        }
        self.poll()?;
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
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("E3DN presentation clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: rgba[0],
                            g: rgba[1],
                            b: rgba[2],
                            a: rgba[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit([encoder.finish()]);
        self.track_frame(frame_id, permit);
        frame.present();
        Ok(true)
    }
    pub(crate) fn check_scene_budget(
        &self,
        width: u32,
        height: u32,
        lorenz: bool,
    ) -> Result<(), RenderError> {
        let bytes = if let Some(scene) = &self.meshes {
            if lorenz {
                return Err(RenderError::InvalidLimits);
            }
            scene.resize_peak_bytes(width, height)? + u64::from(width) * u64::from(height) * 4
        } else if lorenz {
            if let Some(scene) = &self.lorenz {
                scene.resize_peak_bytes(self.limits, width, height)?
            } else {
                LorenzScene::admitted_bytes(self.limits, width, height, None)?
            }
        } else {
            self.limits.attachment_bytes(width, height)?
        };
        if bytes > self.limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        Ok(())
    }
    /// Initialize bounded Lorenz resources after pending frames drain; never warms up implicitly.
    pub fn enable_lorenz(&mut self) -> Result<(), RenderError> {
        self.poll()?;
        if self.lorenz.is_some() || self.meshes.is_some() {
            return Err(RenderError::InvalidLimits);
        }
        if self.inflight() != 0 {
            return Err(RenderError::FramesPending);
        }
        self.check_scene_budget(self.config.width, self.config.height, true)?;
        self.lorenz = Some(LorenzScene::new(
            &self.device,
            self.config.format,
            self.limits,
            self.config.width,
            self.config.height,
        )?);
        self.poll()
    }
    pub(crate) fn ensure_mesh_profile(&self) -> Result<(), RenderError> {
        if self.lorenz.is_some() {
            return Err(RenderError::InvalidLimits);
        }
        Ok(())
    }
    /// Retained sample count per trajectory; zero before enabling or after reset.
    pub fn lorenz_samples(&self) -> u32 {
        self.lorenz.as_ref().map_or(0, |s| s.compute.count())
    }
    /// Reset only after pending frames drain. Keep pumping and retry on FramesPending.
    pub fn reset_lorenz(&mut self) -> Result<(), RenderError> {
        self.poll()?;
        if self.inflight() != 0 {
            return Err(RenderError::FramesPending);
        }
        self.lorenz
            .as_mut()
            .ok_or(RenderError::InvalidLimits)?
            .compute
            .reset(&self.queue);
        Ok(())
    }
    /// Change bounded Lorenz coefficients and restart trails after pending frames drain.
    /// Invalid input leaves the simulation untouched; reset_lorenz preserves these coefficients.
    pub fn configure_lorenz(
        &mut self,
        parameters: crate::LorenzParameters,
    ) -> Result<(), RenderError> {
        parameters.validate()?;
        self.poll()?;
        if self.inflight() != 0 {
            return Err(RenderError::FramesPending);
        }
        self.lorenz
            .as_mut()
            .ok_or(RenderError::InvalidLimits)?
            .compute
            .reset_with_parameters(&self.queue, parameters)
    }
    /// Present six GPU-computed trajectories; 0 steps draws a paused scene, maximum 8.
    /// Admission covers both compute and draw submissions; no CPU particle readback occurs.
    pub fn present_lorenz(&mut self, steps: u32, view: LorenzView) -> Result<bool, RenderError> {
        view.validate()?;
        if steps > 8 || self.lorenz.is_none() {
            return Err(RenderError::InvalidLimits);
        }
        self.poll()?;
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
        let scene = self.lorenz.as_mut().ok_or(RenderError::InvalidLimits)?;
        scene
            .compute
            .submit_steps(&self.device, &self.queue, steps)?;
        let target = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        scene.draw(
            &self.queue,
            &mut encoder,
            &target,
            view,
            self.config.width,
            self.config.height,
        )?;
        self.queue.submit([encoder.finish()]);
        self.track_frame(frame_id, permit);
        frame.present();
        Ok(true)
    }
    /// Destroy this generation's real GPU device for explicit fault qualification.
    /// This is not a simulated hardware reset. Recreate through a new renderer/window.
    #[cfg(feature = "qualification")]
    pub fn qualification_destroy_device(&self) {
        self.device.destroy();
    }
    /// Inject the same suspended state used for outdated/lost surfaces.
    /// This tests the retry contract, not physical compositor loss.
    #[cfg(feature = "qualification")]
    pub fn qualification_surface_loss(&mut self) -> Result<(), RenderError> {
        self.poll()?;
        self.state = PresentationState::Suspended;
        Ok(())
    }
    /// Stop accepting submissions, without blocking or implying GPU completion.
    /// Keep the window alive until finish_close succeeds or the owner explicitly destroys the device.
    pub fn begin_close(&mut self) {
        self.state = PresentationState::Closed;
    }
    /// Abandon this generation after an error or the owner's drain deadline.
    /// Destroys the device while the borrowed window is still alive. Does not wait,
    /// report pending work as completed, or permit reuse. Repeated calls are harmless.
    pub fn abort_close(&mut self) {
        self.aborted = true;
        self.state = PresentationState::Closed;
        self.device.destroy();
    }
    /// Nonblocking drain observation after begin_close; pump this under deployment's shutdown deadline.
    /// After abort_close, returns RendererClosed rather than claiming a successful drain.
    pub fn finish_close(&self) -> Result<(), RenderError> {
        if self.aborted {
            return Err(RenderError::RendererClosed);
        }
        if self.state != PresentationState::Closed {
            return Err(RenderError::InvalidLimits);
        }
        self.device
            .poll(wgpu::PollType::Poll)
            .map_err(|_| RenderError::DeviceUnavailable)?;
        if self.inflight() != 0 {
            return Err(RenderError::FramesPending);
        }
        Ok(())
    }
}

impl Drop for NativeRenderer<'_> {
    fn drop(&mut self) {
        // Error returns must invalidate the device before releasing its surface owner.
        self.device.destroy();
    }
}
