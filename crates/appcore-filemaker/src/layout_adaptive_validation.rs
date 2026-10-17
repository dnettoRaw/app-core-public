// =============================================================================
//        #######
//     ###       ###     F: layout_adaptive_validation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Validates the bounded adaptive layout contract before returning any scene.
use crate::layout_adaptive::AdaptiveRuntime;
use crate::layout_geometry::layout_error;
use crate::{
    AdaptiveLayoutPlan, DocumentIr, ElementIr, ElementKind, LayoutEngine, Length, Rect,
    ResolvedScene, Result, TablePageBodies, Unit,
};

pub(crate) fn prepare(
    engine: &LayoutEngine<'_>,
    document: &mut DocumentIr,
    plan: &AdaptiveLayoutPlan,
) -> Result<AdaptiveRuntime> {
    let (table_index, closing_index) = locate_roots(document, plan)?;
    let closing = &document.elements[closing_index];
    let (closing_height, gap) = measure_closing(engine, document, closing, plan)?;
    let mut closing_ids = Vec::new();
    collect_ids(closing, &mut closing_ids);
    install_bodies(&mut document.elements[table_index], plan)?;
    Ok(AdaptiveRuntime {
        plan: plan.clone(),
        diagnostics: None,
        closing_height,
        closing_ids,
        reserve: if document.elements[closing_index].hidden {
            Unit::ZERO
        } else {
            closing_height.checked_add(gap)?
        },
        control: engine.control.clone(),
    })
}

fn validate_axis_aligned(element: &ElementIr) -> Result<()> {
    let t = element.transform;
    if t.rotate != 0
        || t.scale_x != 1_000_000
        || t.scale_y != 1_000_000
        || t.translate_x != Length::Absolute(Unit::ZERO)
        || t.translate_y != Length::Absolute(Unit::ZERO)
        || element.geometry.region.is_some()
        || element.keep_with_next
    {
        return Err(layout_error("adaptive table and closing root require untransformed, unregionalized geometry without keep_with_next").at(element.id.as_str()));
    }
    Ok(())
}
fn count_tables(element: &ElementIr) -> usize {
    if element.hidden || element.page_placement.is_some() {
        return 0;
    }
    usize::from(element.kind == ElementKind::Table)
        + element.children.iter().map(count_tables).sum::<usize>()
}
fn collect_ids(element: &ElementIr, ids: &mut Vec<crate::ElementId>) {
    if element.hidden {
        return;
    }
    ids.push(element.id.clone());
    for child in &element.children {
        collect_ids(child, ids);
    }
}
pub(crate) fn within(inner: Rect, outer: Rect) -> Result<bool> {
    Ok(inner.origin.x >= outer.origin.x
        && inner.origin.y >= outer.origin.y
        && inner.right()? <= outer.right()?
        && inner.bottom()? <= outer.bottom()?)
}

pub(crate) fn verify(scene: &ResolvedScene, runtime: &AdaptiveRuntime) -> Result<()> {
    let (last_page, table) = scene
        .pages
        .iter()
        .rev()
        .find_map(|p| {
            p.elements
                .iter()
                .find(|e| e.id == runtime.plan.table)
                .map(|e| (p, e))
        })
        .ok_or_else(|| layout_error("adaptive table produced no fragment"))?;
    if runtime.closing_ids.is_empty() {
        return Ok(());
    }
    let closing = last_page
        .elements
        .iter()
        .find(|e| e.id == runtime.plan.closing.element)
        .ok_or_else(|| {
            layout_error(
                "closing block left the final table page; no partial adaptive scene returned",
            )
        })?;
    if closing.bounds.layout.size.height != runtime.closing_height
        || closing.bounds.layout.origin.y
            != table
                .bounds
                .layout
                .bottom()?
                .checked_add(runtime.reserve.checked_sub(runtime.closing_height)?)?
        || last_page.index + 1 != scene.pages.len()
    {
        return Err(layout_error("closing placement changed after reservation; use intrinsic height and remove conflicting geometry").at(closing.id.as_str()));
    }
    let page_bounds = last_page.page_template.as_ref().map_or_else(
        || {
            Rect::new(
                Unit::ZERO,
                Unit::ZERO,
                last_page.size.width,
                last_page.size.height,
            )
        },
        crate::PageTemplate::content_bounds,
    )?;
    if !within(closing.bounds.layout, page_bounds)? {
        return Err(layout_error("closing block exceeds the useful page region"));
    }
    for id in &runtime.closing_ids {
        let node = last_page
            .elements
            .iter()
            .find(|e| &e.id == id)
            .ok_or_else(|| {
                layout_error("closing descendant moved to another page").at(id.as_str())
            })?;
        if !within(node.bounds.layout, closing.bounds.layout)? {
            return Err(layout_error("closing descendant exceeds reserved block").at(id.as_str()));
        }
    }
    Ok(())
}

fn locate_roots(document: &DocumentIr, plan: &AdaptiveLayoutPlan) -> Result<(usize, usize)> {
    if plan.filemaker_layout != "1.0" {
        return Err(crate::FileMakerError::new(
            crate::ErrorCode::SchemaVersion,
            "unsupported adaptive layout version; expected 1.0",
        ));
    }
    let table_index = document
        .elements
        .iter()
        .position(|e| e.id == plan.table && !e.hidden && e.page_placement.is_none())
        .ok_or_else(|| {
            layout_error("adaptive table must be a visible body root").at(plan.table.as_str())
        })?;
    let closing_index = document
        .elements
        .iter()
        .position(|e| e.id == plan.closing.element && e.page_placement.is_none())
        .ok_or_else(|| {
            layout_error("closing block must be a body root").at(plan.closing.element.as_str())
        })?;
    if closing_index <= table_index
        || document
            .elements
            .iter()
            .skip(closing_index + 1)
            .any(|e| !e.hidden && e.page_placement.is_none())
    {
        return Err(layout_error(
            "closing block must follow the table and be the final visible body root",
        ));
    }
    for (index, element) in document.elements.iter().enumerate() {
        if count_tables(element) != usize::from(index == table_index) {
            return Err(layout_error(
                "adaptive layout supports exactly one body table",
            ));
        }
    }
    Ok((table_index, closing_index))
}
fn measure_closing(
    engine: &LayoutEngine<'_>,
    document: &DocumentIr,
    closing: &ElementIr,
    plan: &AdaptiveLayoutPlan,
) -> Result<(Unit, Unit)> {
    validate_axis_aligned(closing)?;
    let gap = match plan.closing.gap {
        Length::Absolute(v) => v,
        Length::Logical(v) => engine.options.logical_unit.checked_scale(v)?,
        _ => return Err(layout_error("closing gap must be absolute or logical")),
    };
    if gap < Unit::ZERO {
        return Err(layout_error("closing gap cannot be negative"));
    }
    let expression = closing
        .geometry
        .anchors
        .get("top")
        .ok_or_else(|| layout_error("closing block requires a top anchor to table.bottom"))?;
    let (target, edge, offset) =
        crate::layout_geometry::parse_anchor(expression, engine.options.logical_unit)?;
    if target != plan.table.as_str()
        || edge != "bottom"
        || offset != gap
        || closing.geometry.anchors.len() != 1
        || closing.geometry.y.is_some()
    {
        return Err(
            layout_error("closing top anchor must match table.bottom plus the plan gap")
                .at(closing.id.as_str()),
        );
    }
    let page = document
        .page_size
        .ok_or_else(|| layout_error("explicit page size required"))?;
    let container = document.page_template.as_ref().map_or_else(
        || crate::Size::new(page.width, page.height),
        |p| Ok(p.content_bounds()?.size),
    )?;
    let closing_height = if closing.hidden {
        Unit::ZERO
    } else {
        let size = crate::constraints::resolve_constrained_size(
            closing.geometry.width,
            closing.geometry.height,
            closing.geometry.constraints,
            container,
            crate::Size::new(container.width, Unit::points(1)?)?,
            engine.options.logical_unit,
        )?;
        if closing.geometry.height.is_none() || closing.geometry.height == Some(Length::Auto) {
            return Err(layout_error(
                "closing block requires explicit or intrinsic height",
            ));
        }
        size.height
    };
    Ok((closing_height, gap))
}
fn install_bodies(table: &mut ElementIr, plan: &AdaptiveLayoutPlan) -> Result<()> {
    validate_axis_aligned(table)?;
    let intent = table
        .table
        .as_mut()
        .ok_or_else(|| layout_error("adaptive table target is not a table"))?;
    if intent.page_bodies.is_some()
        || !intent.spec.total_fields.is_empty()
        || intent
            .spec
            .conditional_styles
            .iter()
            .any(|s| s.reserve_after.is_some())
    {
        return Err(layout_error("adaptive table cannot combine V1 page_bodies, built-in totals or row reserve_after; use one reservation owner").at(table.id.as_str()));
    }
    intent.page_bodies = Some(TablePageBodies {
        first: plan.bodies.first.clone(),
        continuation: plan.bodies.intermediate.clone(),
    });
    Ok(())
}

pub(crate) fn check_table_start(
    first: &crate::layout::ElementPlacement,
    document: &DocumentIr,
    context: &crate::layout_context::LayoutContext,
    fragments: &[crate::ResolvedTableFragment],
) -> Result<()> {
    if first.page_index != 0 {
        return Err(layout_error(
            "adaptive table must start on physical page zero",
        ));
    }
    let useful = document.page_template.as_ref().map_or_else(
        || {
            Rect::new(
                Unit::ZERO,
                Unit::ZERO,
                context.page_size.width,
                context.page_size.height,
            )
        },
        crate::PageTemplate::content_bounds,
    )?;
    if !within(first.rect, useful)? {
        return Err(layout_error(
            "adaptive table bounds must fit the useful page region",
        ));
    }
    if fragments.first().is_some_and(|f| f.index > 0) {
        return Err(layout_error("adaptive group would leave an empty first page of table rows; enlarge its body or choose smaller groups"));
    }
    Ok(())
}
