// =============================================================================
//        #######
//     ###       ###     F: layout_collision.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded layout collision contracts and behavior for this crate.

use std::collections::BTreeSet;

use crate::layout_context::LayoutContext;
use crate::layout_geometry::{layout_error, non_convergent};
use crate::{
    CollisionPolicy, CollisionResolution, CollisionRule, ElementIr, LayoutOptions,
    OperationControl, ProgressPhase, Rect, ResourceLimits, Result, Size, Unit,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_candidate(
    element: &ElementIr,
    mut page: usize,
    mut bounds: Rect,
    policy: &CollisionPolicy,
    context: &mut LayoutContext,
    limits: &ResourceLimits,
    options: &LayoutOptions,
    control: &OperationControl,
) -> Result<(usize, Rect)> {
    let mut seen = BTreeSet::new();
    for iteration in 0..limits.max_reflows {
        control.checkpoint(
            ProgressPhase::Reflow,
            u64::try_from(iteration).unwrap_or(u64::MAX),
            u64::try_from(limits.max_reflows).ok(),
        )?;
        context.ensure_page(page)?;
        let state = (
            page,
            bounds.origin.x.raw(),
            bounds.origin.y.raw(),
            bounds.size.width.raw(),
            bounds.size.height.raw(),
        );
        if !seen.insert(state) {
            return Err(non_convergent(element, "collision reflow cycle detected"));
        }
        if bounds.bottom()? > context.page_size.height {
            page = page
                .checked_add(1)
                .ok_or_else(|| non_convergent(element, "page index overflow"))?;
            bounds.origin.y = Unit::ZERO;
            continue;
        }
        if !policy.enabled || policy.resolution == CollisionResolution::Overlay {
            return Ok((page, bounds));
        }
        let candidate = CollisionRule {
            id: element.id.as_str().to_owned(),
            bounds,
            policy: policy.clone(),
            sequence: context.sequence,
        };
        let overlap =
            context.first_collision(page, &candidate, limits.max_collision_comparisons)?;
        let Some(overlap) = overlap else {
            return Ok((page, bounds));
        };
        match policy.resolution {
            CollisionResolution::Error => {
                return Err(layout_error(format!(
                    "element `{}` collides with `{}`",
                    element.id.as_str(),
                    overlap.id
                )))
            }
            CollisionResolution::Push => {
                if !policy.movable || policy.priority > overlap.policy.priority {
                    return Err(layout_error(
                        "collision push cannot move effective candidate",
                    ));
                }
                bounds.origin.y = overlap
                    .bounds
                    .bottom()?
                    .checked_add(options.collision_gap)?;
            }
            CollisionResolution::NextPage => {
                page = page
                    .checked_add(1)
                    .ok_or_else(|| non_convergent(element, "page index overflow"))?;
                bounds.origin.y = Unit::ZERO;
            }
            CollisionResolution::Shrink => {
                let width = bounds.size.width.checked_scale(900_000)?;
                let height = bounds.size.height.checked_scale(900_000)?;
                if width < options.minimum_size || height < options.minimum_size {
                    return Err(non_convergent(
                        element,
                        "collision shrink reached minimum size",
                    ));
                }
                bounds.size = Size::new(width, height)?;
            }
            CollisionResolution::Overlay => return Ok((page, bounds)),
        }
    }
    Err(non_convergent(
        element,
        "collision reflow iteration limit exceeded",
    ))
}
