// =============================================================================
//        #######
//     ###       ###     F: limits.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

/// Controlled errors at rendering/resource admission boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    /// Outstanding owned scene snapshots reached their configured capacity.
    SnapshotsPending,
    /// A bounded CPU reservation could not be allocated.
    CpuAllocationFailed,
    /// Two presentation submissions are pending; caller must keep pumping.
    FramesPending,
    /// Surface needs explicit resize/recreation before further submission.
    SurfaceUnavailable,
    /// Renderer no longer accepts new operations.
    RendererClosed,
    /// Device rejected bounded target/pipeline allocation or validation.
    GpuAllocationFailed,
    /// Required native GPU adapter/device is unavailable or lost.
    DeviceUnavailable,
    /// Diagnostic capture could not map the submitted result.
    CaptureFailed,
    /// Diagnostic capture did not finish within its fixed deadline.
    CaptureTimeout,
    /// Dimensions or configured limits are outside the supported range.
    InvalidLimits,
    /// Managed bytes or slot capacity would exceed its declared budget.
    BudgetExceeded,
    /// A handle is stale or belongs to another arena.
    InvalidResource,
    /// A monotonically increasing identity cannot advance safely.
    IdentityExhausted,
}
impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RenderError {}

/// Limits for managed resources; excludes driver and operating-system overhead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderLimits {
    /// Maximum physical attachment width/height.
    pub max_dimension: u32,
    /// Total bytes admitted by a resource arena.
    pub managed_bytes: u64,
    /// Maximum live or retired resource slots.
    pub resource_slots: u32,
}
impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_dimension: 4096,
            managed_bytes: 128 * 1024 * 1024,
            resource_slots: 512,
        }
    }
}
impl RenderLimits {
    /// Validate configuration before allocating slots or GPU memory.
    pub fn validate(self) -> Result<Self, RenderError> {
        if self.max_dimension == 0
            || self.max_dimension > 16384
            || self.managed_bytes == 0
            || self.resource_slots == 0
            || self.resource_slots > 65536
        {
            return Err(RenderError::InvalidLimits);
        }
        Ok(self)
    }
    /// Account RGBA8 color and Depth32Float attachments before creation.
    pub fn attachment_bytes(self, width: u32, height: u32) -> Result<u64, RenderError> {
        self.validate()?;
        if width == 0 || height == 0 || width > self.max_dimension || height > self.max_dimension {
            return Err(RenderError::InvalidLimits);
        }
        let bytes = u64::from(width) * u64::from(height) * 8;
        if bytes > self.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        Ok(bytes)
    }
}
