// =============================================================================
//        #######
//     ###       ###     F: adaptive_layout.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Explicit bounded adaptive pagination design, independent of template V1.
use crate::{ElementId, ErrorCode, FileMakerError, Length, ResourceLimits, Result, TablePageBody};
use serde::Deserialize;

/// Opt-in YAML layout plan with private fields to preserve Rust construction compatibility.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveLayoutPlan {
    pub(crate) filemaker_layout: String,
    pub(crate) table: ElementId,
    pub(crate) bodies: AdaptiveBodies,
    pub(crate) closing: ClosingBlock,
    #[serde(default)]
    pub(crate) first_after: Option<ClosingBlock>,
    pub(crate) oversized_group: OversizedGroup,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdaptiveBodies {
    pub(crate) first: TablePageBody,
    pub(crate) intermediate: TablePageBody,
    pub(crate) last: TablePageBody,
    pub(crate) single: TablePageBody,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClosingBlock {
    pub(crate) element: ElementId,
    pub(crate) gap: Length,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OversizedGroup {
    Error,
    SplitRows,
}

impl AdaptiveLayoutPlan {
    /// Parses one strict YAML plan, bounded by the template byte limit.
    /// Unknown fields, duplicate fields and versions other than `1.0` fail.
    pub fn parse_yaml(bytes: &[u8], limits: &ResourceLimits) -> Result<Self> {
        limits.validate()?;
        ResourceLimits::check(
            "adaptive plan bytes",
            bytes.len(),
            limits.max_template_bytes,
        )?;
        let options = serde_saphyr::options! { strict_booleans: true, with_snippet: false, budget: serde_saphyr::budget! { max_documents: 1 } };
        let plan: Self = serde_saphyr::from_slice_with_options(bytes, options).map_err(|e| {
            FileMakerError::new(
                ErrorCode::SchemaSyntax,
                format!("invalid adaptive layout YAML: {e}"),
            )
        })?;
        if plan.filemaker_layout != "1.0" {
            return Err(FileMakerError::new(
                ErrorCode::SchemaVersion,
                "unsupported adaptive layout version; expected 1.0",
            ));
        }
        Ok(plan)
    }
}
