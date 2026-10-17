// =============================================================================
//        #######
//     ###       ###     F: lorenz.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{LorenzParameters, RenderError, RenderLimits};
use wgpu::util::DeviceExt;
/// Six fixed-step Lorenz trajectories, double-buffered heads and bounded GPU ring trails.
/// The owner must admit the step within its frame budget before calling submit_steps.
pub struct LorenzCompute {
    heads: [wgpu::Buffer; 2],
    trails: wgpu::Buffer,
    parameters: wgpu::Buffer,
    groups: [wgpu::BindGroup; 2],
    pipeline: wgpu::ComputePipeline,
    settings: LorenzParameters,
    current: usize,
    cursor: u32,
    count: u32,
}
impl LorenzCompute {
    /// Total fixed GPU buffer bytes, excluding pipeline/driver overhead.
    pub const BUFFER_BYTES: u64 = 6 * 4096 * 16 + 2 * 6 * 16 + 32;
    /// Create after admission against the caller's remaining GPU memory budget.
    /// Caller accounts these bytes together with every other resource on this device.
    pub fn new(device: &wgpu::Device, limits: RenderLimits) -> Result<Self, RenderError> {
        limits.validate()?;
        if limits.managed_bytes < Self::BUFFER_BYTES {
            return Err(RenderError::BudgetExceeded);
        }
        let seeds = seeds();
        let head = || {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("E3DN Lorenz heads"),
                contents: &seeds,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
            })
        };
        let heads = [head(), head()];
        let trails = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("E3DN Lorenz ring"),
            size: 6 * 4096 * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("E3DN Lorenz controls"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("E3DN Lorenz RK4"),
            source: wgpu::ShaderSource::Wgsl(include_str!("lorenz.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("E3DN Lorenz RK4"),
            layout: None,
            module: &shader,
            entry_point: Some("integrate"),
            compilation_options: Default::default(),
            cache: None,
        });
        let layout = pipeline.get_bind_group_layout(0);
        let group = |input: usize, output: usize| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("E3DN Lorenz step"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: heads[input].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: heads[output].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: trails.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: parameters.as_entire_binding(),
                    },
                ],
            })
        };
        let groups = [group(0, 1), group(1, 0)];
        Ok(Self {
            heads,
            trails,
            parameters,
            groups,
            pipeline,
            settings: LorenzParameters::default(),
            current: 0,
            cursor: 0,
            count: 0,
        })
    }
    /// Submit one bounded dispatch, then advance the CPU ring state.
    /// Zero steps perform no queue writes or submission. The caller's frame permit must
    /// cover this submission and subsequent rendering on the same queue.
    pub fn submit_steps(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        steps: u32,
    ) -> Result<Option<wgpu::SubmissionIndex>, RenderError> {
        if steps > 8 {
            return Err(RenderError::InvalidLimits);
        }
        if steps == 0 {
            return Ok(None);
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(&self.cursor.to_le_bytes());
        bytes.extend_from_slice(&steps.to_le_bytes());
        bytes.extend_from_slice(&0.005f32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        for value in [
            self.settings.sigma,
            self.settings.rho,
            self.settings.beta,
            0.0,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        queue.write_buffer(&self.parameters, 0, &bytes);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("E3DN Lorenz"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.groups[self.current], &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        let submission = queue.submit([encoder.finish()]);
        self.current ^= 1;
        self.cursor = (self.cursor + steps) % 4096;
        self.count = (self.count + steps).min(4096);
        Ok(Some(submission))
    }
    /// Current coefficients; reset() preserves them.
    pub fn parameters(&self) -> LorenzParameters {
        self.settings
    }
    /// Validate coefficients and restart trajectories, discarding previous trail history.
    /// Caller must first observe completion of all pending work on this simulation.
    pub fn reset_with_parameters(
        &mut self,
        queue: &wgpu::Queue,
        parameters: LorenzParameters,
    ) -> Result<(), RenderError> {
        parameters.validate()?;
        self.settings = parameters;
        self.reset(queue);
        Ok(())
    }
    /// Retained samples per trajectory, at most 4096.
    pub fn count(&self) -> u32 {
        self.count
    }
    /// Next ring sample index, in 0..4096.
    pub fn cursor(&self) -> u32 {
        self.cursor
    }
    /// GPU trail storage for rendering. Only count() samples per trajectory are initialized.
    pub fn trails(&self) -> &wgpu::Buffer {
        &self.trails
    }
    /// Current GPU head storage, available for explicit diagnostic readback.
    pub fn heads(&self) -> &wgpu::Buffer {
        &self.heads[self.current]
    }
    /// Reset buffers after pending work has drained. Caller owns admission and GPU completion.
    pub fn reset(&mut self, queue: &wgpu::Queue) {
        let seeds = seeds();
        for head in &self.heads {
            queue.write_buffer(head, 0, &seeds);
        }
        self.current = 0;
        self.cursor = 0;
        self.count = 0;
    }
}
fn seeds() -> Vec<u8> {
    (0..6)
        .flat_map(|i| {
            [1.0 + i as f32 * 0.001, 1.0, 1.0, 1.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
        })
        .collect()
}
