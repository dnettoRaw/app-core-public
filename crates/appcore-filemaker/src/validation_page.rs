// =============================================================================
//        #######
//     ###       ###     F: validation_page.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeSet;

use crate::{
    PageRole, PreflightOptions, Rect, ResolvedPage, ResourceLimits, Result, Unit, ValidationCode,
    ValidationReport, ValidationSeverity,
};

pub(crate) fn inspect(
    page: &ResolvedPage,
    expected_index: usize,
    total: usize,
    limits: &ResourceLimits,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) -> Result<()> {
    if page.index != expected_index || page.role != expected_role(expected_index, total) {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Contract,
            Some(page.index),
            None,
            "resolved page index or semantic role is inconsistent",
            options.max_issues,
        );
    }
    if page
        .page_template
        .as_ref()
        .is_some_and(|template| template.role != page.role || template.size != page.size)
    {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Contract,
            Some(page.index),
            None,
            "resolved page template metadata is inconsistent",
            options.max_issues,
        );
    }
    if page.exclusions.len() > limits.max_elements {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Budget,
            Some(page.index),
            None,
            "resolved exclusion count exceeds configured element limit",
            options.max_issues,
        );
        return Ok(());
    }
    inspect_exclusions(page, options, report)
}

fn inspect_exclusions(
    page: &ResolvedPage,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) -> Result<()> {
    let page_bounds = Rect::new(Unit::ZERO, Unit::ZERO, page.size.width, page.size.height)?;
    let mut names = BTreeSet::new();
    for exclusion in &page.exclusions {
        if !names.insert(&exclusion.name)
            || exclusion.name.is_empty()
            || exclusion.group.is_empty()
            || !contains(page_bounds, exclusion.bounds)?
        {
            report.push(
                ValidationSeverity::Error,
                ValidationCode::Contract,
                Some(page.index),
                None,
                format!("resolved exclusion `{}` is invalid", exclusion.name),
                options.max_issues,
            );
        }
    }
    Ok(())
}

fn expected_role(index: usize, total: usize) -> PageRole {
    if index == 0 {
        PageRole::First
    } else if index + 1 == total {
        PageRole::Last
    } else {
        PageRole::Continuation
    }
}

fn contains(outer: Rect, inner: Rect) -> Result<bool> {
    Ok(inner.origin.x >= outer.origin.x
        && inner.origin.y >= outer.origin.y
        && inner.right()? <= outer.right()?
        && inner.bottom()? <= outer.bottom()?)
}
