// =============================================================================
//        #######
//     ###       ###     F: raster_options.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 12:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Explicit per-export raster working-surface limits.

use std::io::Write;

use crate::{
    ErrorCode, ExportContext, ExportFormat, ExportOutcome, ExportRequest, FileMakerError,
    OperationControl, ResolvedScene, Result,
};

/// Limits one RGBA working strip, not codec/assets/output or whole-process memory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RasterOptions {
    max_strip_bytes: usize,
    max_strip_rows: u32,
}

impl Default for RasterOptions {
    fn default() -> Self {
        Self {
            max_strip_bytes: 4 * 1024 * 1024,
            max_strip_rows: 256,
        }
    }
}

impl RasterOptions {
    /// Creates limits from 1 byte through 64 MiB and from 1 through 4096 rows.
    ///
    /// The independent 4 MiB scanline ceiling still applies. A scanline that
    /// cannot fit the selected strip-byte limit is rejected, never rounded up.
    pub fn new(max_strip_bytes: usize, max_strip_rows: u32) -> Result<Self> {
        if !(1..=64 * 1024 * 1024).contains(&max_strip_bytes)
            || !(1..=4096).contains(&max_strip_rows)
        {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "raster strip limits must be 1..=64 MiB and 1..=4096 rows",
            ));
        }
        Ok(Self {
            max_strip_bytes,
            max_strip_rows,
        })
    }

    /// Maximum RGBA bytes in one working strip.
    #[must_use]
    pub const fn max_strip_bytes(self) -> usize {
        self.max_strip_bytes
    }

    /// Maximum rows in one working strip.
    #[must_use]
    pub const fn max_strip_rows(self) -> u32 {
        self.max_strip_rows
    }
}

/// Streams PNG/JPEG using explicit working-strip limits and cooperative control.
///
/// Shares validation, paint overrides and loss reporting with `export_controlled`.
/// Other formats are rejected before writing. Strip size may change encoding
/// chunks, not resolved layout; callers own any partial output after failure.
pub fn export_raster_controlled(
    scene: &ResolvedScene,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    options: RasterOptions,
    control: &OperationControl,
    writer: &mut dyn Write,
) -> Result<ExportOutcome> {
    if !matches!(request.format, ExportFormat::Png | ExportFormat::Jpeg) {
        return Err(FileMakerError::new(
            ErrorCode::ExportUnsupported,
            "raster strip options require PNG or JPEG",
        ));
    }
    super::core::export_with_control(
        scene,
        request,
        context,
        Some(control),
        Some(options),
        writer,
    )
}
