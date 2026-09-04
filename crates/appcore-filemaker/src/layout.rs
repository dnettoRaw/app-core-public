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
use crate::layout_geometry::{
    layout_error, propose_rect, resolve_layout_rect, resolve_transform, select_collision_bounds,
    shape_for, validate_anchor_graph, visual_bounds,
};
use crate::layout_measure::{measure_content, resolve_image};

use crate::{
    AssetResolver, BoundsSet, CollisionPolicy, DocumentFingerprint, DocumentIr, ElementIr,
    FontManager, LayoutMode, OperationControl, ProgressPhase, Rect, ResolvedElement, ResolvedScene,
    ResourceLimits, Result, SceneCache, TextOverflow, Transform, Unit, ENGINE_VERSION,
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
    fonts: &'a FontManager,
    pub(crate) options: LayoutOptions,
    control: OperationControl,
    assets: Option<&'a dyn AssetResolver>,
}

#[derive(Clone)]
struct ElementPlacement {
    initial_page: usize,
    page_index: usize,
    proposed: Rect,
    rect: Rect,
    policy: CollisionPolicy,
    transform: Transform,
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
        for element in elements
            .iter()
            .filter(|element| element.page_placement.is_none())
        {
            context.checkpoint(&self.control, self.limits.max_elements)?;
            if element.hidden {
                continue;
            }
            let placement = self.place_element(
                element,
                document,
                container,
                page_index,
                parent_layout,
                crate::Point {
                    x: flow_x,
                    y: flow_y,
                },
                inherited_collision,
                parent_transform,
                context,
            )?;
            if element.kind == crate::ElementKind::Table {
                let first = placement.clone();
                let last = self.commit_table_fragments(
                    element,
                    document,
                    container,
                    parent_layout,
                    crate::Point {
                        x: flow_x,
                        y: flow_y,
                    },
                    inherited_collision,
                    parent_transform,
                    placement,
                    context,
                )?;
                page_index = last.page_index;
                context.positions.insert(
                    element.id.as_str().to_owned(),
                    (first.page_index, first.rect),
                );
                match parent_layout {
                    LayoutMode::FlowVertical => {
                        flow_y = last.rect.bottom()?.checked_add(effective_gap)?;
                    }
                    LayoutMode::FlowHorizontal => {
                        flow_x = last.rect.right()?.checked_add(effective_gap)?;
                    }
                    LayoutMode::Absolute => {}
                }
                continue;
            }
            page_index = placement.page_index;
            let resolved = self.build_resolved_element(element, &placement, context)?;
            context.commit(placement.page_index, resolved, placement.policy.clone())?;
            context.positions.insert(
                element.id.as_str().to_owned(),
                (placement.page_index, placement.rect),
            );
            match parent_layout {
                LayoutMode::FlowVertical => {
                    flow_y = placement.rect.bottom()?.checked_add(effective_gap)?;
                }
                LayoutMode::FlowHorizontal => {
                    flow_x = placement.rect.right()?.checked_add(effective_gap)?;
                }
                LayoutMode::Absolute => {}
            }
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
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn place_element(
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
            measure_content(element, proposed, self.fonts)?;
        if element.kind == crate::ElementKind::Text
            && element.text_options.overflow == TextOverflow::Expand
        {
            let measured = initial_text_layout
                .as_ref()
                .ok_or_else(|| layout_error("expanded text has no measurement"))?
                .measured;
            proposed.size.width = proposed.size.width.max(measured.width);
            proposed.size.height = proposed.size.height.max(measured.height);
            (initial_style, _, initial_intrinsic) = measure_content(element, proposed, self.fonts)?;
        }
        let proposed_layout = proposed;
        let proposed_transform = resolve_transform(element, proposed, self.options.logical_unit)?
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
            page_index,
            collision_candidate,
            &policy,
            context,
            self.limits,
            &self.options,
            &self.control,
        )?;
        let rect = resolve_layout_rect(
            policy.bounds,
            proposed,
            collision_candidate,
            resolved_collision,
            parent_transform,
        )?;
        let transform =
            resolve_transform(element, rect, self.options.logical_unit)?.then(parent_transform)?;
        Ok(ElementPlacement {
            initial_page,
            page_index,
            proposed: proposed_layout,
            rect,
            policy,
            transform,
        })
    }

    fn build_resolved_element(
        &self,
        element: &ElementIr,
        placement: &ElementPlacement,
        context: &mut LayoutContext,
    ) -> Result<ResolvedElement> {
        let (style, text_layout, intrinsic) = measure_content(element, placement.rect, self.fonts)?;
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
    fn commit_table_fragments(
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
            if index > 0 {
                placement = self.place_element(
                    element,
                    document,
                    container,
                    placement
                        .page_index
                        .checked_add(1)
                        .ok_or_else(|| layout_error("table continuation page index overflow"))?,
                    parent_layout,
                    flow_origin,
                    inherited_collision,
                    parent_transform,
                    context,
                )?;
            }
            crate::layout_table::translate_table_fragment(
                &mut fragment,
                first.rect.origin,
                placement.rect.origin,
            )?;
            let mut resolved = self.build_resolved_element(element, &placement, context)?;
            resolved.table = Some(fragment);
            context.commit(placement.page_index, resolved, placement.policy.clone())?;
        }
        Ok(placement)
    }
}
