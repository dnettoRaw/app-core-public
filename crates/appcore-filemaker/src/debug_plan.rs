// =============================================================================
//        #######
//     ###       ###     F: debug_plan.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{DebugOverlayOptions, ErrorCode, FileMakerError, ResolvedPage, ResourceLimits, Result};

pub(crate) fn validate_overlay(
    page: &ResolvedPage,
    options: &DebugOverlayOptions,
    limits: &ResourceLimits,
) -> Result<()> {
    let mut count = 0_usize;
    if let Some(spacing) = options.grid {
        add(&mut count, axis_marks(page.size.width, spacing)?)?;
        add(&mut count, axis_marks(page.size.height, spacing)?)?;
    }
    if options.safe_area && page.page_template.is_some() {
        add(&mut count, 2)?;
    }
    if options.regions {
        add(
            &mut count,
            page.regions
                .len()
                .checked_mul(2)
                .ok_or_else(count_overflow)?,
        )?;
    }
    if options.collision {
        let collidable = page
            .elements
            .iter()
            .filter(|element| element.collidable)
            .count();
        let named = collidable
            .checked_add(page.exclusions.len())
            .and_then(|value| value.checked_mul(2))
            .ok_or_else(count_overflow)?;
        add(&mut count, named)?;
    }
    for element in &page.elements {
        if options.bounds {
            add(
                &mut count,
                if options.view == crate::MaskView::Combined {
                    4
                } else {
                    1
                },
            )?;
        }
        add(&mut count, usize::from(options.ids))?;
        add(&mut count, usize::from(options.coordinates))?;
        if options.anchors {
            add(&mut count, element.layout_trace.geometry.anchors.len())?;
        }
        if options.crosshair {
            add(&mut count, 2)?;
        }
    }
    if options.ruler {
        let spacing = options.grid.unwrap_or(crate::Unit::points(10)?);
        add(&mut count, axis_marks(page.size.width, spacing)?)?;
        add(&mut count, axis_marks(page.size.height, spacing)?)?;
    }
    if count > limits.max_preflight_comparisons {
        return Err(FileMakerError::new(
            ErrorCode::LimitExceeded,
            "debug overlay exceeds the diagnostic geometry budget",
        ));
    }
    Ok(())
}

fn axis_marks(size: crate::Unit, spacing: crate::Unit) -> Result<usize> {
    if size < crate::Unit::ZERO || spacing <= crate::Unit::ZERO {
        return Err(FileMakerError::new(
            ErrorCode::GeometryInvalid,
            "debug grid dimensions are invalid",
        ));
    }
    let marks = size
        .raw()
        .checked_div(spacing.raw())
        .and_then(|value| value.checked_add(1))
        .ok_or_else(count_overflow)?;
    usize::try_from(marks).map_err(|_| count_overflow())
}

fn add(count: &mut usize, value: usize) -> Result<()> {
    *count = count.checked_add(value).ok_or_else(count_overflow)?;
    Ok(())
}

fn count_overflow() -> FileMakerError {
    FileMakerError::new(
        ErrorCode::LimitExceeded,
        "debug overlay primitive count overflow",
    )
}
