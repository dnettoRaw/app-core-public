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

pub(crate) fn selected_bounds(
    bounds: BoundsSet,
    view: MaskView,
) -> impl ExactSizeIterator<Item = Rect> {
    // Views contain one or four entries; no per-element heap buffer is needed.
    let (selected, count) = match view {
        MaskView::CollisionMask => ([bounds.collision; 4], 1),
        MaskView::LayoutBounds => ([bounds.layout; 4], 1),
        MaskView::VisualBounds => ([bounds.visual; 4], 1),
        MaskView::Combined => (
            [
                bounds.intrinsic,
                bounds.layout,
                bounds.collision,
                bounds.visual,
            ],
            4,
        ),
    };
    selected.into_iter().take(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_views_preserve_exact_bounds_order_and_duplicates() {
        let rectangles = [1, 2, 3, 4].map(|x| {
            Rect::new(
                Unit::from_raw(x),
                Unit::ZERO,
                Unit::from_raw(10),
                Unit::from_raw(10),
            )
            .unwrap()
        });
        let mut bounds = BoundsSet {
            intrinsic: rectangles[0],
            layout: rectangles[1],
            collision: rectangles[2],
            visual: rectangles[3],
            clip: None,
        };
        for (view, expected) in [
            (MaskView::CollisionMask, rectangles[2]),
            (MaskView::LayoutBounds, rectangles[1]),
            (MaskView::VisualBounds, rectangles[3]),
        ] {
            let mut selected = selected_bounds(bounds, view);
            assert_eq!(selected.len(), 1);
            assert_eq!(selected.next(), Some(expected));
            assert_eq!(selected.len(), 0);
            assert_eq!(selected.next(), None);
        }
        assert!(selected_bounds(bounds, MaskView::Combined).eq(rectangles));
        bounds.visual = bounds.intrinsic;
        // The mask deduplicates; overlays still receive every selected bounds class.
        assert!(selected_bounds(bounds, MaskView::Combined).eq([
            rectangles[0],
            rectangles[1],
            rectangles[2],
            rectangles[0]
        ]));
    }
}
