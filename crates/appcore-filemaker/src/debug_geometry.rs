// =============================================================================
//        #######
//     ###       ###     F: debug_geometry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{BoundsSet, ElementId, MaskView, Rect, Result, Size, Unit};

pub(crate) fn mask_free_regions(
    size: Size,
    occupied: &[(ElementId, Rect)],
    budget: &mut crate::diagnostic_budget::DiagnosticBudget,
) -> Result<Vec<Rect>> {
    let mut free = vec![Rect::new(Unit::ZERO, Unit::ZERO, size.width, size.height)?];
    for (_, bounds) in occupied {
        let mut next = Vec::new();
        for region in free {
            budget.operation()?;
            let pieces = crate::inspect::subtract(region, *bounds)?;
            budget.retained(next.len().saturating_add(pieces.len()))?;
            next.extend(pieces);
        }
        free = next;
    }
    let minimum = Unit::points(1)?;
    free.retain(|region| region.size.width >= minimum && region.size.height >= minimum);
    free.sort_by_key(|region| {
        (
            region.origin.y,
            region.origin.x,
            region.size.height,
            region.size.width,
        )
    });
    Ok(free)
}

pub(crate) fn selected_bounds(bounds: BoundsSet, view: MaskView) -> Vec<Rect> {
    match view {
        MaskView::CollisionMask => vec![bounds.collision],
        MaskView::LayoutBounds => vec![bounds.layout],
        MaskView::VisualBounds => vec![bounds.visual],
        MaskView::Combined => vec![
            bounds.intrinsic,
            bounds.layout,
            bounds.collision,
            bounds.visual,
        ],
    }
}
