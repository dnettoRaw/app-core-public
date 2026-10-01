// =============================================================================
//        #######
//     ###       ###     F: layout.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded layout contracts and behavior for this crate.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::layout_context::LayoutContext;
use crate::layout_geometry::{layout_error, validate_anchor_graph};

use crate::{
    AssetResolver, CollisionPolicy, DocumentFingerprint, DocumentIr, ElementIr, FontManager,
    LayoutMode, OperationControl, ProgressPhase, Rect, ResolvedScene, ResourceLimits, Result,
    SceneCache, Transform, Unit, ENGINE_VERSION,
};

/// Explicit layout and reflow policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LayoutOptions {
    /// Default inherited collision policy.
    pub collision: CollisionPolicy,
    /// Logical unit used by `lu` lengths.
    pub logical_unit: Unit,
    /// Smallest dimension accepted by collision shrinking.
    pub minimum_size: Unit,
    /// Gap introduced by push collision resolution.
    pub collision_gap: Unit,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            collision: CollisionPolicy::default(),
            logical_unit: Unit::from_raw(Unit::PER_POINT),
            minimum_size: Unit::from_raw(Unit::PER_POINT),
            collision_gap: Unit::ZERO,
        }
    }
}

/// Deterministic measure → propose → query → resolve → commit engine.
pub struct LayoutEngine<'a> {
    pub(crate) limits: &'a ResourceLimits,
    pub(crate) fonts: &'a FontManager,
    pub(crate) options: LayoutOptions,
    pub(crate) control: OperationControl,
    pub(crate) assets: Option<&'a dyn AssetResolver>,
}

#[derive(Clone)]
pub(crate) struct ElementPlacement {
    pub(crate) initial_page: usize,
    pub(crate) page_index: usize,
    pub(crate) proposed: Rect,
    pub(crate) rect: Rect,
    pub(crate) policy: CollisionPolicy,
    pub(crate) transform: Transform,
}

impl<'a> LayoutEngine<'a> {
    /// Creates an engine over explicit limits and fonts.
    pub fn new(
        limits: &'a ResourceLimits,
        fonts: &'a FontManager,
        options: LayoutOptions,
    ) -> Result<Self> {
        Self::new_controlled(limits, fonts, options, OperationControl::default())
    }

    /// Creates an engine with cooperative cancellation and progress controls.
    pub fn new_controlled(
        limits: &'a ResourceLimits,
        fonts: &'a FontManager,
        options: LayoutOptions,
        control: OperationControl,
    ) -> Result<Self> {
        limits.validate()?;
        if options.logical_unit <= Unit::ZERO
            || options.minimum_size <= Unit::ZERO
            || options.collision_gap < Unit::ZERO
        {
            return Err(layout_error("layout options contain invalid dimensions"));
        }
        Ok(Self {
            limits,
            fonts,
            options,
            control,
            assets: None,
        })
    }

    /// Supplies the explicit resolver needed to resolve image paint geometry.
    #[must_use]
    pub fn with_assets(mut self, assets: &'a dyn AssetResolver) -> Self {
        self.assets = Some(assets);
        self
    }

    /// Resolves all geometry before any exporter is selected.
    pub fn resolve(&self, document: &DocumentIr) -> Result<ResolvedScene> {
        self.control.checkpoint(ProgressPhase::Layout, 0, None)?;
        let page_size = document
            .page_size
            .ok_or_else(|| layout_error("document/canvas requires an explicit page size"))?;
        validate_anchor_graph(&document.elements)?;
        let exclusions = crate::layout_exclusion::resolve_exclusions(
            document,
            page_size,
            self.options.logical_unit,
        )?;
        let mut context = LayoutContext::new(
            page_size,
            document.page_template.clone(),
            exclusions,
            self.limits.max_pages,
            self.limits.max_elements,
        );
        let page_rect = document.page_template.as_ref().map_or_else(
            || Rect::new(Unit::ZERO, Unit::ZERO, page_size.width, page_size.height),
            crate::PageTemplate::content_bounds,
        )?;
        let document_collision = document
            .collision
            .as_ref()
            .unwrap_or(&self.options.collision);
        let page_collision = document
            .page_collision
            .as_ref()
            .unwrap_or(document_collision);
        let regions =
            crate::layout_region::resolve_regions(document, page_rect, self.options.logical_unit)?;
        context.regions = regions;
        self.layout_list(
            &document.elements,
            document,
            page_rect,
            0,
            LayoutMode::Absolute,
            crate::Distribution::Start,
            Unit::ZERO,
            page_collision,
            Transform::IDENTITY,
            &mut context,
        )?;
        context.remove_leading_empty_pages();
        crate::layout_page::resolve_page_layers(self, document, page_collision, &mut context)?;
        for page in &mut context.pages {
            page.elements.sort_by(|left, right| {
                (&left.layer, left.z_index, left.sequence).cmp(&(
                    &right.layer,
                    right.z_index,
                    right.sequence,
                ))
            });
        }
        self.control.checkpoint(
            ProgressPhase::Layout,
            u64::try_from(context.sequence).unwrap_or(u64::MAX),
            Some(u64::try_from(self.limits.max_elements).unwrap_or(u64::MAX)),
        )?;
        Ok(ResolvedScene {
            template_id: document.template_id.clone(),
            pages: context.pages,
            engine_version: ENGINE_VERSION.to_owned(),
        })
    }

    /// Resolves only on a fingerprint miss and returns an immutable shared scene.
    ///
    /// The fingerprint must be computed from the same template, data, patches,
    /// assets, and fonts used to produce `document` and this engine.
    pub fn resolve_cached(
        &self,
        document: &DocumentIr,
        fingerprint: DocumentFingerprint,
        cache: &mut SceneCache,
    ) -> Result<Arc<ResolvedScene>> {
        cache.get_or_try_insert_with(fingerprint, self.limits, || self.resolve(document))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout_list(
        &self,
        elements: &[ElementIr],
        document: &DocumentIr,
        container: Rect,
        mut page_index: usize,
        parent_layout: LayoutMode,
        distribution: crate::Distribution,
        gap: Unit,
        inherited_collision: &CollisionPolicy,
        parent_transform: Transform,
        context: &mut LayoutContext,
    ) -> Result<()> {
        let flow = crate::layout_flow::plan_flow(
            elements,
            container,
            parent_layout,
            distribution,
            gap,
            self.options.logical_unit,
        )?;
        let mut flow_x = flow.x;
        let mut flow_y = flow.y;
        let effective_gap = flow.gap;
        let visible: Vec<_> = elements
            .iter()
            .filter(|element| element.page_placement.is_none())
            .filter(|element| !element.hidden)
            .collect();
        let mut index = 0;
        while index < visible.len() {
            let element = visible[index];
            context.checkpoint(&self.control, self.limits.max_elements)?;
            crate::layout_keep::validate_flow(parent_layout, element)?;
            if element.keep_with_next {
                (page_index, flow_y) = self.defer_kept_block(
                    &visible,
                    index,
                    container,
                    flow_x,
                    flow_y,
                    effective_gap,
                    page_index,
                    document,
                    context,
                )?;
            }
            page_index = self.layout_element_at_flow(
                element,
                document,
                container,
                page_index,
                parent_layout,
                &mut flow_x,
                &mut flow_y,
                effective_gap,
                inherited_collision,
                parent_transform,
                context,
            )?;
            index += 1;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn defer_kept_block(
        &self,
        visible: &[&ElementIr],
        index: usize,
        container: Rect,
        flow_x: Unit,
        flow_y: Unit,
        gap: Unit,
        page_index: usize,
        document: &DocumentIr,
        context: &LayoutContext,
    ) -> Result<(usize, Unit)> {
        let mut end = index + 1;
        while end < visible.len() && visible[end - 1].keep_with_next {
            end += 1;
        }
        if end == index + 1 || visible[end - 1].keep_with_next {
            return Err(
                layout_error("keep_with_next must be followed by another visible sibling")
                    .at(visible[index].id.as_str()),
            );
        }
        let height = crate::layout_keep::block_height(
            &visible[index..end],
            container,
            flow_x,
            flow_y,
            gap,
            self.options.logical_unit,
            &context.positions,
            &document.guides,
            self.fonts,
        )?;
        if crate::layout_keep::should_defer(height, flow_y, container)? {
            Ok((
                crate::layout_keep::next_page(page_index)?,
                container.origin.y,
            ))
        } else {
            Ok((page_index, flow_y))
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_element_at_flow(
        &self,
        element: &ElementIr,
        document: &DocumentIr,
        container: Rect,
        page_index: usize,
        parent_layout: LayoutMode,
        flow_x: &mut Unit,
        flow_y: &mut Unit,
        gap: Unit,
        inherited_collision: &CollisionPolicy,
        parent_transform: Transform,
        context: &mut LayoutContext,
    ) -> Result<usize> {
        let flow_origin = crate::Point {
            x: *flow_x,
            y: *flow_y,
        };
        if let Some(last) = self.commit_expanded_text_fragments(
            element,
            document,
            container,
            page_index,
            parent_layout,
            flow_origin,
            inherited_collision,
            parent_transform,
            context,
        )? {
            self.record_flow_end(element, &last, parent_layout, gap, flow_x, flow_y, context)?;
            return Ok(last.page_index);
        }
        let placement_page = crate::layout_geometry::anchor_page(
            element,
            &context.positions,
            self.options.logical_unit,
        )?
        .unwrap_or(page_index);
        let placement = self.place_element(
            element,
            document,
            container,
            placement_page,
            parent_layout,
            flow_origin,
            inherited_collision,
            parent_transform,
            context,
        )?;
        if element.kind == crate::ElementKind::Table {
            let last = self.commit_table_fragments(
                element,
                document,
                container,
                parent_layout,
                flow_origin,
                inherited_collision,
                parent_transform,
                placement,
                context,
            )?;
            self.record_flow_end(element, &last, parent_layout, gap, flow_x, flow_y, context)?;
            return Ok(last.page_index);
        }
        self.commit_regular_element(
            element,
            document,
            placement,
            parent_layout,
            gap,
            flow_x,
            flow_y,
            context,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn record_flow_end(
        &self,
        element: &ElementIr,
        last: &ElementPlacement,
        parent_layout: LayoutMode,
        gap: Unit,
        flow_x: &mut Unit,
        flow_y: &mut Unit,
        context: &mut LayoutContext,
    ) -> Result<()> {
        context
            .positions
            .insert(element.id.as_str().to_owned(), (last.page_index, last.rect));
        advance_flow(parent_layout, last.rect, gap, flow_x, flow_y)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_regular_element(
        &self,
        element: &ElementIr,
        document: &DocumentIr,
        placement: ElementPlacement,
        parent_layout: LayoutMode,
        gap: Unit,
        flow_x: &mut Unit,
        flow_y: &mut Unit,
        context: &mut LayoutContext,
    ) -> Result<usize> {
        let resolved = self.build_resolved_element(element, &placement, context)?;
        context.commit(placement.page_index, resolved, placement.policy.clone())?;
        context.positions.insert(
            element.id.as_str().to_owned(),
            (placement.page_index, placement.rect),
        );
        advance_flow(parent_layout, placement.rect, gap, flow_x, flow_y)?;
        let child_gap = element
            .gap
            .resolve(placement.rect.size.width, self.options.logical_unit)?
            .unwrap_or(Unit::ZERO);
        self.layout_list(
            &element.children,
            document,
            placement.rect,
            placement.page_index,
            element.layout,
            element.distribute,
            child_gap,
            &placement.policy,
            placement.transform,
            context,
        )?;
        Ok(placement.page_index)
    }
}

fn advance_flow(
    layout: LayoutMode,
    rect: Rect,
    gap: Unit,
    flow_x: &mut Unit,
    flow_y: &mut Unit,
) -> Result<()> {
    match layout {
        LayoutMode::FlowVertical => *flow_y = rect.bottom()?.checked_add(gap)?,
        LayoutMode::FlowHorizontal => *flow_x = rect.right()?.checked_add(gap)?,
        LayoutMode::Absolute => {}
    }
    Ok(())
}
