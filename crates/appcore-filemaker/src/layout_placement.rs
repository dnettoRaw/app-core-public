// =============================================================================
//        #######
//     ###       ###     F: layout_placement.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Proposes element geometry and resolves its collision placement.

use crate::layout::{ElementPlacement, LayoutEngine};
use crate::layout_context::LayoutContext;
use crate::layout_geometry::{
    layout_error, propose_rect, resolve_layout_rect, resolve_transform, select_collision_bounds,
    visual_bounds,
};
use crate::layout_measure::measure_content;
use crate::{
    CollisionPolicy, DocumentIr, ElementIr, LayoutMode, Rect, Result, TextOverflow, Transform,
};

impl LayoutEngine<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn place_element(
        &self,
        element: &ElementIr,
        document: &DocumentIr,
        container: Rect,
        page_index: usize,
        parent_layout: LayoutMode,
        flow_origin: crate::Point,
        inherited_collision: &CollisionPolicy,
        parent_transform: Transform,
        context: &mut LayoutContext,
    ) -> Result<ElementPlacement> {
        let initial_page = page_index;
        let effective_container = crate::layout_region::resolve_region(
            element,
            document,
            container,
            self.options.logical_unit,
        )?;
        let mut proposed = propose_rect(
            element,
            effective_container,
            parent_layout,
            flow_origin,
            &context.positions,
            &document.guides,
            self.options.logical_unit,
        )?;
        let policy = crate::layout_policy::effective_collision_policy(
            element,
            document,
            inherited_collision,
        );
        crate::layout_policy::validate_shrink_policy(&policy)?;
        let (mut initial_style, initial_text_layout, mut initial_intrinsic) =
            measure_content(element, proposed, self.fonts, self.options.logical_unit)?;
        if element.kind == crate::ElementKind::Text
            && element.text_options.overflow == TextOverflow::Expand
        {
            let measured = initial_text_layout
                .as_ref()
                .ok_or_else(|| layout_error("expanded text has no measurement"))?
                .outer_measured()?;
            proposed.size.width = proposed.size.width.max(measured.width);
            proposed.size.height = proposed.size.height.max(measured.height);
            (initial_style, _, initial_intrinsic) =
                measure_content(element, proposed, self.fonts, self.options.logical_unit)?;
        }
        resolve_placement(
            self,
            element,
            proposed,
            initial_page,
            initial_style,
            initial_intrinsic,
            parent_transform,
            policy,
            context,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_placement(
    engine: &LayoutEngine<'_>,
    element: &ElementIr,
    proposed: Rect,
    initial_page: usize,
    initial_style: crate::ComputedStyle,
    initial_intrinsic: Rect,
    parent_transform: Transform,
    policy: CollisionPolicy,
    context: &mut LayoutContext,
) -> Result<ElementPlacement> {
    let proposed_layout = proposed;
    let proposed_transform = resolve_transform(element, proposed, engine.options.logical_unit)?
        .then(parent_transform)?;
    crate::layout_policy::validate_shrink_transform(&policy, proposed_transform)?;
    let initial_visual =
        proposed_transform.bounds(visual_bounds(proposed, initial_style.stroke_width)?)?;
    let collision_candidate = select_collision_bounds(
        policy.bounds,
        proposed_transform.bounds(proposed)?,
        proposed_transform.bounds(initial_intrinsic)?,
        initial_visual,
    );
    let (page_index, resolved_collision) = crate::layout_collision::resolve_candidate(
        element,
        initial_page,
        collision_candidate,
        &policy,
        context,
        engine.limits,
        &engine.options,
        &engine.control,
    )?;
    let rect = resolve_layout_rect(
        policy.bounds,
        proposed,
        collision_candidate,
        resolved_collision,
        parent_transform,
    )?;
    let transform =
        resolve_transform(element, rect, engine.options.logical_unit)?.then(parent_transform)?;
    Ok(ElementPlacement {
        initial_page,
        page_index,
        proposed: proposed_layout,
        rect,
        policy,
        transform,
    })
}
