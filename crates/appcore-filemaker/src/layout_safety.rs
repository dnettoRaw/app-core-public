//! Public bounded anti-overflow and layout-safety audit helpers.

use serde::{Deserialize, Serialize};

use crate::{
    ErrorCode, OperationControl, ResolvedScene, ResourceLimits, Result, ValidationCode,
    ValidationReport,
};

/// Options for a deterministic resolved-scene safety audit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayoutSafetyOptions {
    /// Maximum retained issues.
    pub max_issues: usize,
    /// Treat warnings such as collisions and clipping as failures.
    pub strict: bool,
}

impl Default for LayoutSafetyOptions {
    fn default() -> Self {
        Self {
            max_issues: 256,
            strict: false,
        }
    }
}

/// Stable summary of overflow, collision and text-safety findings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LayoutSafetyReport {
    /// Number of physical pages in the resolved scene.
    pub pages: usize,
    /// Number of resolved elements in the scene.
    pub elements: usize,
    /// Number of retained overflow findings.
    pub overflow_count: usize,
    /// Number of retained collision findings.
    pub collision_count: usize,
    /// Number of retained text-related findings.
    pub text_issue_count: usize,
    /// Whether the underlying report was truncated by its bound.
    pub truncated: bool,
    /// Full bounded validation evidence.
    pub validation: ValidationReport,
}

impl LayoutSafetyReport {
    /// Returns true when the scene has no retained issues or truncation.
    #[must_use]
    pub fn is_safe(&self) -> bool {
        !self.truncated && self.validation.issues.is_empty()
    }

    /// Serializes the bounded report for golden or support fixtures.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|error| crate::FileMakerError::new(ErrorCode::Validation, error.to_string()))
    }
}

/// Audits a resolved scene using the crate's existing bounded layout checks.
pub fn audit_layout(
    scene: &ResolvedScene,
    limits: &ResourceLimits,
    options: &LayoutSafetyOptions,
    control: &OperationControl,
) -> Result<LayoutSafetyReport> {
    if options.max_issues == 0 {
        return Err(crate::FileMakerError::new(
            ErrorCode::Validation,
            "layout safety requires a non-zero issue bound",
        ));
    }
    let validation = crate::validate_layout(scene, limits, options.max_issues, control)?;
    let report = summarize(scene, validation);
    if options.strict {
        report.validation.enforce(true)?;
    }
    Ok(report)
}

fn summarize(scene: &ResolvedScene, validation: ValidationReport) -> LayoutSafetyReport {
    let mut overflow_count = 0;
    let mut collision_count = 0;
    let mut text_issue_count = 0;
    for issue in &validation.issues {
        match issue.code {
            ValidationCode::Overflow => overflow_count += 1,
            ValidationCode::Collision => collision_count += 1,
            ValidationCode::Glyph => text_issue_count += 1,
            _ => {}
        }
    }
    LayoutSafetyReport {
        pages: scene.pages.len(),
        elements: scene.pages.iter().map(|page| page.elements.len()).sum(),
        overflow_count,
        collision_count,
        text_issue_count,
        truncated: validation.truncated,
        validation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ValidationCode, ValidationIssue, ValidationSeverity};

    #[test]
    fn report_is_stable_and_counts_safety_categories() {
        let report = ValidationReport {
            issues: vec![
                ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    code: ValidationCode::Overflow,
                    page: Some(0),
                    element: Some("title".to_owned()),
                    message: "overflow".to_owned(),
                },
                ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    code: ValidationCode::Collision,
                    page: Some(0),
                    element: Some("body".to_owned()),
                    message: "collision".to_owned(),
                },
                ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    code: ValidationCode::Glyph,
                    page: Some(0),
                    element: Some("body".to_owned()),
                    message: "glyph".to_owned(),
                },
            ],
            truncated: false,
        };
        let scene = ResolvedScene {
            template_id: "golden".to_owned(),
            pages: Vec::new(),
            engine_version: crate::ENGINE_VERSION.to_owned(),
        };
        let safety = summarize(&scene, report);
        assert_eq!(
            (
                safety.overflow_count,
                safety.collision_count,
                safety.text_issue_count
            ),
            (1, 1, 1)
        );
        assert!(!safety.is_safe());
        assert!(safety.to_json().unwrap().contains("overflow_count"));
    }
}
