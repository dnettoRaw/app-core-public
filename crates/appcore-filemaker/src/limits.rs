// =============================================================================
//        #######
//     ###       ###     F: limits.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use serde::{Deserialize, Serialize};

use crate::{ErrorCode, FileMakerError, Result};

/// Explicit limits applied throughout parsing, compilation, and export.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResourceLimits {
    /// Maximum YAML/template bytes.
    pub max_template_bytes: usize,
    /// Maximum total bytes loaded through include resolvers.
    pub max_include_bytes: usize,
    /// Maximum include nesting depth.
    pub max_include_depth: usize,
    /// Maximum resolved elements across all pages.
    pub max_elements: usize,
    /// Maximum vector path commands across one source template.
    pub max_path_commands: usize,
    /// Maximum generated pages.
    pub max_pages: usize,
    /// Maximum Unicode scalar bytes in one text node.
    pub max_text_bytes: usize,
    /// Maximum bytes accepted for one explicitly resolved asset or font.
    pub max_asset_bytes: usize,
    /// Maximum raster output pixels.
    pub max_pixels: u64,
    /// Maximum collision/reflow iterations.
    pub max_reflows: usize,
    /// Maximum spatial-rule comparisons during one layout.
    pub max_collision_comparisons: usize,
    /// Maximum expression operations per evaluation.
    pub max_expression_steps: usize,
    /// Maximum runtime patch operations in one transaction or complete bind batch.
    pub max_patch_operations: usize,
    /// Maximum geometry comparisons or retained diagnostics during preflight,
    /// masks, overlays, and free-region inspection.
    pub max_preflight_comparisons: usize,
    /// Maximum streamed dataset rows.
    pub max_rows: u64,
    /// Maximum total output bytes written by in-memory helpers.
    pub max_output_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_template_bytes: 4 * 1024 * 1024,
            max_include_bytes: 16 * 1024 * 1024,
            max_include_depth: 16,
            max_elements: 100_000,
            max_path_commands: 1_000_000,
            max_pages: 10_000,
            max_text_bytes: 4 * 1024 * 1024,
            max_asset_bytes: 256 * 1024 * 1024,
            max_pixels: 100_000_000,
            max_reflows: 128,
            max_collision_comparisons: 1_000_000,
            max_expression_steps: 10_000,
            max_patch_operations: 10_000,
            max_preflight_comparisons: 1_000_000,
            max_rows: 10_000_000,
            max_output_bytes: 512 * 1024 * 1024,
        }
    }
}

impl ResourceLimits {
    /// Rejects zero or internally inconsistent bounds.
    pub fn validate(&self) -> Result<()> {
        let all_nonzero = self.max_template_bytes > 0
            && self.max_include_bytes > 0
            && self.max_include_depth > 0
            && self.max_elements > 0
            && self.max_path_commands > 0
            && self.max_pages > 0
            && self.max_text_bytes > 0
            && self.max_asset_bytes > 0
            && self.max_pixels > 0
            && self.max_reflows > 0
            && self.max_collision_comparisons > 0
            && self.max_expression_steps > 0
            && self.max_patch_operations > 0
            && self.max_preflight_comparisons > 0
            && self.max_rows > 0
            && self.max_output_bytes > 0;
        if !all_nonzero {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "resource limits must be non-zero",
            ));
        }
        Ok(())
    }

    /// Checks a named observed value against its limit.
    #[allow(dead_code)]
    pub(crate) fn check(name: &'static str, observed: usize, limit: usize) -> Result<()> {
        if observed > limit {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                format!("{name} is {observed}; limit is {limit}"),
            ));
        }
        Ok(())
    }
}
