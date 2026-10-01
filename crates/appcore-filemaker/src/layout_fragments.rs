// =============================================================================
//        #######
//     ###       ###     F: layout_fragments.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Commits elements whose resolved layout spans physical pages.

use crate::layout::ElementPlacement;
use crate::layout_context::LayoutContext;
use crate::layout_geometry::{
    layout_error, propose_rect, resolve_transform, select_collision_bounds, shape_for,
    visual_bounds,
};
use crate::layout_measure::{measure_content, resolve_image};
use crate::{
    BoundsSet, CollisionPolicy, DataValue, DocumentIr, ElementIr, LayoutEngine, LayoutMode, Rect,
    ResolvedElement, Result, Transform, Unit,
};

impl LayoutEngine<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_expanded_text_fragments(
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
    ) -> Result<Option<ElementPlacement>> {
        if parent_layout != LayoutMode::FlowVertical
            || element.kind != crate::ElementKind::Text
            || element.text_options.overflow != crate::TextOverflow::Expand
            || element.text_options.writing_mode != crate::WritingMode::Horizontal
            || element.geometry.y.is_some()
            || !element.geometry.anchors.is_empty()
            || element.geometry.region.is_some()
            || element.geometry.constraints != crate::LayoutConstraints::default()
            || element.geometry.align_y.is_some()
            || !element.children.is_empty()
        {
            return Ok(None);
        }
        let effective_container = crate::layout_region::resolve_region(
            element,
            document,
            container,
            self.options.logical_unit,
        )?;
        let proposed = propose_rect(
            element,
            effective_container,
            parent_layout,
            flow_origin,
            &context.positions,
            &document.guides,
            self.options.logical_unit,
        )?;
        let transform = resolve_transform(element, proposed, self.options.logical_unit)?;
        if transform != Transform::IDENTITY {
            return Ok(None);
        }
        let (_, initial_layout, _) =
            measure_content(element, proposed, self.fonts, self.options.logical_unit)?;
        let Some(initial_layout) = initial_layout else {
            return Ok(None);
        };
        let mut proposed = proposed;
        proposed.size.width = proposed
            .size
            .width
            .max(initial_layout.outer_measured()?.width);
        let (_, layout, _) =
            measure_content(element, proposed, self.fonts, self.options.logical_unit)?;
        let layout = layout.ok_or_else(|| layout_error("expanded text has no measured layout"))?;
        let rendered_height = layout
            .lines
            .iter()
            .try_fold(Unit::ZERO, |height, line| height.checked_add(line.height))?;
        let rendered_height =
            rendered_height.checked_add(layout.padding.top.checked_add(layout.padding.bottom)?)?;
        if rendered_height <= effective_container.size.height {
            return Ok(None);
        }
        let chunks =
            crate::text_pagination::split_lines(&layout, flow_origin.y, effective_container)?;
        if chunks.len() < 2 {
            return Ok(None);
        }
        self.commit_text_chunks(
            element,
            document,
            effective_container,
            page_index,
            parent_layout,
            flow_origin,
            inherited_collision,
            parent_transform,
            context,
            &layout,
            &chunks,
        )
        .map(Some)
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_text_chunks(
        &self,
        element: &ElementIr,
        document: &DocumentIr,
        effective_container: Rect,
        page_index: usize,
        parent_layout: LayoutMode,
        flow_origin: crate::Point,
        inherited_collision: &CollisionPolicy,
        parent_transform: Transform,
        context: &mut LayoutContext,
        layout: &crate::TextLayout,
        chunks: &[crate::text_pagination::TextChunk],
    ) -> Result<ElementPlacement> {
        let policy = crate::layout_policy::effective_collision_policy(
            element,
            document,
            inherited_collision,
        );
        crate::layout_policy::validate_shrink_policy(&policy)?;
        let mut current_page = page_index;
        let mut current_y = flow_origin.y;
        let mut last = None;
        for chunk in chunks {
            if chunk.starts_new_page {
                current_page = current_page
                    .checked_add(1)
                    .ok_or_else(|| layout_error("text continuation page index overflow"))?;
                current_y = effective_container.origin.y;
            }
            let mut fragment_text = String::new();
            for (index, line) in layout.lines[chunk.lines.clone()].iter().enumerate() {
                if index > 0 {
                    fragment_text.push('\n');
                }
                fragment_text.push_str(&line.source_text);
            }
            let mut fragment = element.clone_with_text(fragment_text);
            fragment.text_options.overflow = crate::TextOverflow::Wrap;
            fragment.text_options.max_lines = None;
            fragment.geometry.height = Some(crate::Length::Absolute(chunk.height));
            let placement = self.place_element(
                &fragment,
                document,
                effective_container,
                current_page,
                parent_layout,
                crate::Point {
                    x: flow_origin.x,
                    y: current_y,
                },
                inherited_collision,
                parent_transform,
                context,
            )?;
            if placement.rect.bottom()? > effective_container.bottom()? {
                return Err(layout_error(
                    "text fragment cannot fit its page content region after collision resolution",
                )
                .at(element.id.as_str()));
            }
            let resolved = self.build_resolved_element(&fragment, &placement, context)?;
            context.commit(placement.page_index, resolved, placement.policy.clone())?;
            current_page = placement.page_index;
            current_y = placement.rect.bottom()?;
            last = Some(placement);
        }
        last.ok_or_else(|| layout_error("expanded text pagination produced no fragment"))
    }

    pub(crate) fn build_resolved_element(
        &self,
        element: &ElementIr,
        placement: &ElementPlacement,
        context: &mut LayoutContext,
    ) -> Result<ResolvedElement> {
        let (style, text_layout, intrinsic) = measure_content(
            element,
            placement.rect,
            self.fonts,
            self.options.logical_unit,
        )?;
        let image_placement = resolve_image(element, placement.rect, self.assets, self.limits)?;
        let transformed_layout = placement.transform.bounds(placement.rect)?;
        let transformed_intrinsic = placement.transform.bounds(intrinsic)?;
        let visual = placement
            .transform
            .bounds(visual_bounds(placement.rect, style.stroke_width)?)?;
        let collision = select_collision_bounds(
            placement.policy.bounds,
            transformed_layout,
            transformed_intrinsic,
            visual,
        );
        let clip = text_layout.as_ref().and_then(|layout| {
            layout
                .diagnostics
                .contains(&crate::TextDiagnostic::Clipped)
                .then_some(placement.rect)
        });
        Ok(ResolvedElement {
            id: element.id.clone(),
            kind: element.kind,
            bounds: BoundsSet {
                intrinsic,
                layout: placement.rect,
                collision,
                visual,
                clip,
            },
            collidable: placement.policy.enabled,
            shape: shape_for(element, placement.rect, self.options.logical_unit)?,
            transform: placement.transform,
            style,
            text: element.text.clone(),
            text_layout,
            asset: element.asset.clone(),
            image_placement,
            table: None,
            layer: element.layer.clone(),
            z_index: element.z_index,
            sequence: context.next_sequence(),
            provenance: element.provenance.clone(),
            layout_trace: crate::LayoutTrace {
                geometry: element.geometry.clone(),
                proposed: placement.proposed,
                collision_policy: placement.policy.clone(),
                initial_page: placement.initial_page,
                reflowed: placement.initial_page != placement.page_index
                    || placement.proposed != placement.rect,
            },
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_table_fragments(
        &self,
        element: &ElementIr,
        document: &DocumentIr,
        container: Rect,
        parent_layout: LayoutMode,
        flow_origin: crate::Point,
        inherited_collision: &CollisionPolicy,
        parent_transform: Transform,
        first: ElementPlacement,
        context: &mut LayoutContext,
    ) -> Result<ElementPlacement> {
        let fragments = crate::layout_table::resolve_table_fragments(
            element,
            first.rect,
            self.fonts,
            self.limits,
            self.options.logical_unit,
        )?;
        let mut placement = first.clone();
        for (index, mut fragment) in fragments.into_iter().enumerate() {
            let target_page = first
                .page_index
                .checked_add(fragment.index)
                .ok_or_else(|| layout_error("table continuation page index overflow"))?;
            if index > 0 || target_page != placement.page_index {
                placement = self.place_element(
                    element,
                    document,
                    container,
                    target_page,
                    parent_layout,
                    flow_origin,
                    inherited_collision,
                    parent_transform,
                    context,
                )?;
            }
            let table = element
                .table
                .as_ref()
                .ok_or_else(|| layout_error("table intent is missing"))?;
            let source_bounds = crate::layout_table::page_body_bounds(
                table,
                first.rect,
                fragment.index,
                self.options.logical_unit,
            )?;
            let target_bounds = crate::layout_table::page_body_bounds(
                table,
                placement.rect,
                fragment.index,
                self.options.logical_unit,
            )?;
            crate::layout_table::translate_table_fragment(
                &mut fragment,
                source_bounds.origin,
                target_bounds.origin,
            )?;
            placement.rect = target_bounds;
            let mut resolved = self.build_resolved_element(element, &placement, context)?;
            resolved.table = Some(fragment);
            publish_table_row_anchors(element, placement.page_index, &resolved, context)?;
            context.commit(placement.page_index, resolved, placement.policy.clone())?;
        }
        Ok(placement)
    }
}

fn publish_table_row_anchors(
    element: &ElementIr,
    page_index: usize,
    resolved: &ResolvedElement,
    context: &mut LayoutContext,
) -> Result<()> {
    let Some(field) = element
        .table
        .as_ref()
        .and_then(|table| table.spec.row_anchor_field.as_deref())
    else {
        return Ok(());
    };
    let fragment = resolved
        .table
        .as_ref()
        .ok_or_else(|| layout_error("resolved table fragment is missing"))?;
    let rows = &element
        .table
        .as_ref()
        .ok_or_else(|| layout_error("table intent is missing"))?
        .rows;
    for row in &fragment.rows {
        let source_index = usize::try_from(row.source_index)
            .map_err(|_| layout_error("table row source index exceeds platform bounds"))?;
        let source = rows
            .get(source_index)
            .ok_or_else(|| layout_error("resolved table row has no source row"))?;
        let Some(DataValue::String(name)) = source.get(field) else {
            continue;
        };
        context.positions.insert(
            format!("{}::{name}", element.id.as_str()),
            (page_index, row.bounds),
        );
    }
    Ok(())
}
