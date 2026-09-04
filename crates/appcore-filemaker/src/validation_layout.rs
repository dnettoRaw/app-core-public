// =============================================================================
//        #######
//     ###       ###     F: validation_layout.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded validation layout contracts and behavior for this crate.

use crate::{
    ElementKind, ErrorCode, FileMakerError, OperationControl, PreflightOptions, ProgressPhase,
    Rect, ResolvedScene, ResourceLimits, Result, TextDiagnostic, Unit, ValidationCode,
    ValidationReport, ValidationSeverity,
};

pub(crate) fn inspect(
    scene: &ResolvedScene,
    limits: &ResourceLimits,
    options: &PreflightOptions,
    control: &OperationControl,
) -> Result<ValidationReport> {
    let mut report = ValidationReport::default();
    inspect_scene_contract(scene, limits, options, &mut report);
    let mut comparisons = 0_usize;
    for (expected_index, page) in scene.pages.iter().enumerate() {
        control.checkpoint(
            ProgressPhase::Preflight,
            u64::try_from(page.index).unwrap_or(u64::MAX),
            u64::try_from(scene.pages.len()).ok(),
        )?;
        crate::validation_page::inspect(
            page,
            expected_index,
            scene.pages.len(),
            limits,
            options,
            &mut report,
        )?;
        let page_bounds = Rect::new(Unit::ZERO, Unit::ZERO, page.size.width, page.size.height)?;
        for element in &page.elements {
            inspect_element(page.index, page_bounds, element, options, &mut report)?;
        }
        inspect_collisions(page, limits, options, &mut comparisons, &mut report)?;
    }
    control.checkpoint(
        ProgressPhase::Preflight,
        u64::try_from(scene.pages.len()).unwrap_or(u64::MAX),
        u64::try_from(scene.pages.len()).ok(),
    )?;
    Ok(report)
}

fn inspect_scene_contract(
    scene: &ResolvedScene,
    limits: &ResourceLimits,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) {
    if scene.engine_version != crate::ENGINE_VERSION {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Contract,
            None,
            None,
            "resolved scene engine version does not match the active engine",
            options.max_issues,
        );
    }
    let elements = scene.pages.iter().try_fold(0_usize, |total, page| {
        total.checked_add(page.elements.len())
    });
    if scene.pages.len() > limits.max_pages
        || elements.is_none_or(|count| count > limits.max_elements)
    {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Budget,
            None,
            None,
            "resolved scene exceeds configured page or element budget",
            options.max_issues,
        );
    }
}

fn inspect_element(
    page: usize,
    page_bounds: Rect,
    element: &crate::ResolvedElement,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) -> Result<()> {
    if !contains(page_bounds, element.bounds.visual)? {
        report.push(
            ValidationSeverity::Warning,
            ValidationCode::Overflow,
            Some(page),
            Some(element.id.as_str()),
            "visual bounds leave the page",
            options.max_issues,
        );
    }
    if element.kind == ElementKind::Text {
        inspect_text(page, element, options, report);
    }
    if element.kind == ElementKind::Table {
        crate::validation_table::inspect_layout(page, element, options, report)?;
    }
    Ok(())
}

fn inspect_text(
    page: usize,
    element: &crate::ResolvedElement,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) {
    let Some(layout) = &element.text_layout else {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Glyph,
            Some(page),
            Some(element.id.as_str()),
            "text has no shaped glyph layout",
            options.max_issues,
        );
        return;
    };
    inspect_diagnostics(
        page,
        element.id.as_str(),
        &layout.diagnostics,
        options,
        report,
    );
}

pub(crate) fn inspect_diagnostics(
    page: usize,
    element: &str,
    diagnostics: &[TextDiagnostic],
    options: &PreflightOptions,
    report: &mut ValidationReport,
) {
    for diagnostic in diagnostics {
        let code = match diagnostic {
            TextDiagnostic::Clipped | TextDiagnostic::Ellipsized => ValidationCode::Overflow,
            TextDiagnostic::ColorEmojiRequiresExporter
            | TextDiagnostic::VerticalWritingUnavailable => ValidationCode::Capability,
            TextDiagnostic::Shrunk => continue,
        };
        report.push(
            ValidationSeverity::Warning,
            code,
            Some(page),
            Some(element),
            format!("text diagnostic: {diagnostic:?}"),
            options.max_issues,
        );
    }
}

fn inspect_collisions(
    page: &crate::ResolvedPage,
    limits: &ResourceLimits,
    options: &PreflightOptions,
    comparisons: &mut usize,
    report: &mut ValidationReport,
) -> Result<()> {
    for (index, left) in page.elements.iter().enumerate() {
        if !left.collidable {
            continue;
        }
        for right in &page.elements[index + 1..] {
            if !right.collidable {
                continue;
            }
            *comparisons = comparisons.checked_add(1).ok_or_else(|| {
                FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    "preflight comparison count overflow",
                )
            })?;
            if *comparisons > limits.max_preflight_comparisons {
                report.push(
                    ValidationSeverity::Warning,
                    ValidationCode::Budget,
                    Some(page.index),
                    None,
                    "collision preflight comparison budget exhausted",
                    options.max_issues,
                );
                return Ok(());
            }
            if left.bounds.collision.intersects(right.bounds.collision)? {
                report.push(
                    ValidationSeverity::Warning,
                    ValidationCode::Collision,
                    Some(page.index),
                    Some(left.id.as_str()),
                    format!("collision with `{}`", right.id.as_str()),
                    options.max_issues,
                );
            }
        }
    }
    Ok(())
}

fn contains(outer: Rect, inner: Rect) -> Result<bool> {
    Ok(inner.origin.x >= outer.origin.x
        && inner.origin.y >= outer.origin.y
        && inner.right()? <= outer.right()?
        && inner.bottom()? <= outer.bottom()?)
}
