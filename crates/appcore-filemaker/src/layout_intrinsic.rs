// =============================================================================
//        #######
//     ###       ###     F: layout_intrinsic.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Opt-in intrinsic height planning before ordinary collision placement.

use crate::layout_geometry::layout_error;
use crate::{
    Distribution, DocumentIr, ElementIr, ElementKind, LayoutEngine, LayoutMode, Length,
    ProgressPhase, Rect, ResolvedScene, Result, Size, TextOverflow, Unit, WritingMode,
};

impl LayoutEngine<'_> {
    /// Resolves explicit `height: auto` text and flow groups before placement.
    ///
    /// Uses the same shaping, fonts, padding and line advances as export.
    /// Horizontal groups take the largest child height; vertical groups sum
    /// child heights and gaps. `min_height` preserves a design's short form.
    /// A maximum smaller than measured content returns an error, never a cut.
    ///
    /// This additive opt-in leaves [`Self::resolve`] unchanged. Auto heights
    /// require acyclic, start-distributed body flows with definite widths.
    /// Height-relative padding/children, transformed or positioned children,
    /// page layers, vertical writing and capped text are rejected explicitly.
    /// In roots containing auto heights, descendants explicitly ignore their
    /// containing group IDs, preserving sibling/external collision rules.
    /// Fixed-height elements retain their existing overflow policies. Perform
    /// layout preflight on the returned scene before export. Do not share a
    /// cache fingerprint with ordinary `resolve` for the same document.
    pub fn resolve_intrinsic(&self, document: &DocumentIr) -> Result<ResolvedScene> {
        self.resolve(&self.prepare_intrinsic(document)?)
    }

    pub(crate) fn prepare_intrinsic(&self, document: &DocumentIr) -> Result<DocumentIr> {
        let page = document
            .page_size
            .ok_or_else(|| layout_error("explicit page size required"))?;
        let container = document.page_template.as_ref().map_or_else(
            || Rect::new(Unit::ZERO, Unit::ZERO, page.width, page.height),
            crate::PageTemplate::content_bounds,
        )?;
        let mut count = 0;
        self.check_intrinsic_input(&document.elements, 0, &mut count)?;
        let mut planned = document.clone();
        for element in &mut planned.elements {
            if !contains_auto(element) {
                continue;
            }
            let region = crate::layout_region::resolve_region(
                element,
                document,
                container,
                self.options.logical_unit,
            )?;
            self.plan_intrinsic(element, region.size)?;
            let inherited = document
                .page_collision
                .as_ref()
                .or(document.collision.as_ref())
                .unwrap_or(&self.options.collision);
            exclude_containing_groups(element, document, inherited, &mut Vec::new());
        }
        Ok(planned)
    }

    fn check_intrinsic_input(
        &self,
        elements: &[ElementIr],
        depth: usize,
        count: &mut usize,
    ) -> Result<()> {
        crate::ResourceLimits::check("intrinsic nesting", depth, 64)?;
        for element in elements {
            *count = count.saturating_add(1);
            crate::ResourceLimits::check("intrinsic elements", *count, self.limits.max_elements)?;
            self.control
                .checkpoint(ProgressPhase::Layout, *count as u64, None)?;
            if let Some(text) = &element.text {
                crate::ResourceLimits::check(
                    "intrinsic text bytes",
                    text.len(),
                    self.limits.max_text_bytes,
                )?;
            }
            self.check_intrinsic_input(&element.children, depth + 1, count)?;
        }
        Ok(())
    }

    fn plan_intrinsic(&self, element: &mut ElementIr, container: Size) -> Result<Size> {
        if element.page_placement.is_some() && contains_auto(element) {
            return Err(layout_error(
                "intrinsic sizing is limited to body elements; page bands retain explicit geometry",
            )
            .at(element.id.as_str()));
        }
        self.control.checkpoint(ProgressPhase::Layout, 0, None)?;
        let automatic = element.geometry.height == Some(Length::Auto) && !element.hidden;
        let fallback_height = element
            .style
            .font_size
            .unwrap_or(Unit::points(12)?)
            .checked_scale(1_200_000)?;
        let mut size = crate::constraints::resolve_constrained_size(
            element.geometry.width,
            element.geometry.height,
            element.geometry.constraints,
            container,
            Size::new(container.width, fallback_height)?,
            self.options.logical_unit,
        )?;
        if automatic {
            validate_auto(element)?;
            let height = match element.kind {
                ElementKind::Text => self.intrinsic_text_height(element, size)?,
                ElementKind::Group => self.intrinsic_group_height(element, size)?,
                _ => {
                    return Err(
                        layout_error("intrinsic height supports text and flow groups only")
                            .at(element.id.as_str()),
                    )
                }
            };
            let constraints = element.geometry.constraints;
            let minimum = constraints.min_height.map_or(Ok(None), |v| {
                v.resolve(container.height, self.options.logical_unit)
            })?;
            size.height = minimum.map_or(height, |v| height.max(v));
            let maximum = constraints.max_height.map_or(Ok(None), |v| {
                v.resolve(container.height, self.options.logical_unit)
            })?;
            if maximum.is_some_and(|v| size.height > v) {
                return Err(layout_error("intrinsic content exceeds max_height; use a larger region or explicit fixed-height overflow policy").at(element.id.as_str()));
            }
            if element.kind == ElementKind::Group && size.height > container.height {
                return Err(layout_error("intrinsic group exceeds one useful page; use explicit pagination outside this indivisible group").at(element.id.as_str()));
            }
            element.geometry.height = Some(Length::Absolute(size.height));
        } else {
            for child in &mut element.children {
                // Named regions are resolved during placement, not against this
                // preliminary containing box. Never measure an auto subtree there.
                if child.geometry.region.is_some() && contains_auto(child) {
                    return Err(layout_error(
                        "intrinsic descendants cannot select a nested region",
                    )
                    .at(child.id.as_str()));
                }
                self.plan_intrinsic(child, size)?;
            }
        }
        Ok(size)
    }

    fn intrinsic_text_height(&self, element: &ElementIr, size: Size) -> Result<Unit> {
        if element.text_options.max_lines.is_some()
            || element.text_options.writing_mode != WritingMode::Horizontal
            || !matches!(
                element.text_options.overflow,
                TextOverflow::Expand | TextOverflow::Wrap | TextOverflow::Error
            )
            || matches!(
                element.text_options.padding.top,
                Length::Percent(_) | Length::Auto
            )
            || matches!(
                element.text_options.padding.bottom,
                Length::Percent(_) | Length::Auto
            )
            || !element.children.is_empty()
        {
            return Err(layout_error("intrinsic text requires uncapped horizontal wrap/expand/error and definite block padding").at(element.id.as_str()));
        }
        let mut measured = element.clone();
        measured.text_options.overflow = TextOverflow::Expand;
        let padding = measured.text_options.padding.resolve(
            size.width,
            size.height,
            self.options.logical_unit,
        )?;
        let height = size.height.max(
            padding
                .top
                .checked_add(padding.bottom)?
                .checked_add(Unit::points(1)?)?,
        );
        let bounds = Rect::new(Unit::ZERO, Unit::ZERO, size.width, height)?;
        let (_, text, intrinsic) = crate::layout_measure::measure_content(
            &measured,
            bounds,
            self.fonts,
            self.options.logical_unit,
        )?;
        if intrinsic.size.width > size.width
            || text.as_ref().is_some_and(|t| !t.diagnostics.is_empty())
        {
            return Err(
                layout_error("intrinsic text exceeds its definite width").at(element.id.as_str())
            );
        }
        Ok(intrinsic.size.height.max(Unit::points(1)?))
    }

    fn intrinsic_group_height(&self, element: &mut ElementIr, size: Size) -> Result<Unit> {
        if element.layout == LayoutMode::Absolute || element.distribute != Distribution::Start {
            return Err(layout_error(
                "intrinsic groups require a start-distributed horizontal or vertical flow",
            )
            .at(element.id.as_str()));
        }
        let gap = element
            .gap
            .resolve(size.width, self.options.logical_unit)?
            .ok_or_else(|| layout_error("intrinsic group gap cannot be auto"))?;
        let mut height = Unit::ZERO;
        let mut width = Unit::ZERO;
        for (count, child) in element
            .children
            .iter_mut()
            .filter(|c| !c.hidden)
            .enumerate()
        {
            validate_flow_child(child)?;
            let child_size = self.plan_intrinsic(child, size)?;
            if count > 0 {
                if element.layout == LayoutMode::FlowVertical {
                    height = height.checked_add(gap)?;
                } else {
                    width = width.checked_add(gap)?;
                }
            }
            if element.layout == LayoutMode::FlowVertical {
                height = height.checked_add(child_size.height)?;
                width = width.max(child_size.width);
            } else {
                height = height.max(child_size.height);
                width = width.checked_add(child_size.width)?;
            }
        }
        if width > size.width {
            return Err(
                layout_error("intrinsic flow children exceed container width")
                    .at(element.id.as_str()),
            );
        }
        Ok(height.max(Unit::points(1)?))
    }
}

fn contains_auto(element: &ElementIr) -> bool {
    element.geometry.height == Some(Length::Auto) || element.children.iter().any(contains_auto)
}

fn validate_auto(element: &ElementIr) -> Result<()> {
    if element.page_placement.is_some()
        || element.geometry.constraints.aspect_ratio.is_some()
        || element.geometry.constraints.preferred_height.is_some()
        || element.geometry.width.is_none_or(|v| v == Length::Auto)
    {
        return Err(layout_error("intrinsic height requires definite width, body placement, and no aspect ratio/preferred_height").at(element.id.as_str()));
    }
    Ok(())
}

fn validate_flow_child(child: &ElementIr) -> Result<()> {
    let geometry = &child.geometry;
    let constraints = geometry.constraints;
    let height_relative = [
        geometry.height,
        constraints.min_height,
        constraints.max_height,
        constraints.preferred_height,
    ]
    .into_iter()
    .flatten()
    .any(|v| matches!(v, Length::Percent(_)));
    if geometry.x.is_some()
        || geometry.y.is_some()
        || !geometry.anchors.is_empty()
        || geometry
            .align_x
            .is_some_and(|v| v != crate::Alignment::Start)
        || geometry.region.is_some()
        || geometry
            .align_y
            .is_some_and(|v| v != crate::Alignment::Start)
        || (child.transform.rotate != 0
            || child.transform.scale_x != 1_000_000
            || child.transform.scale_y != 1_000_000
            || child.transform.translate_x != Length::Absolute(Unit::ZERO)
            || child.transform.translate_y != Length::Absolute(Unit::ZERO))
        || height_relative
        || (geometry.height != Some(Length::Auto)
            && child.kind == ElementKind::Text
            && child.text_options.overflow == TextOverflow::Expand)
        || child.page_placement.is_some()
        || child.keep_with_next
    {
        return Err(layout_error("intrinsic flow child requires untransformed, unpositioned, height-independent geometry; expanded text must use height: auto").at(child.id.as_str()));
    }
    Ok(())
}

// Containment is structural, not an accidental overlap. Materialize the
// existing ignore contract only in roots participating in intrinsic planning.
fn exclude_containing_groups(
    element: &mut ElementIr,
    document: &DocumentIr,
    inherited: &crate::CollisionPolicy,
    ancestors: &mut Vec<String>,
) {
    let mut policy = crate::layout_policy::effective_collision_policy(element, document, inherited);
    if !ancestors.is_empty() {
        policy.ignore.extend(ancestors.iter().cloned());
        element.collision = Some(policy.clone());
    }
    let group = element.kind == ElementKind::Group;
    if group {
        ancestors.push(element.id.as_str().to_owned());
    }
    for child in &mut element.children {
        exclude_containing_groups(child, document, &policy, ancestors);
    }
    if group {
        ancestors.pop();
    }
}
