// =============================================================================
//        #######
//     ###       ###     F: layout_page.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded layout page contracts and behavior for this crate.

use crate::layout_context::LayoutContext;
use crate::{
    CollisionPolicy, DocumentIr, ElementIr, ErrorCode, FileMakerError, LayoutEngine, LayoutMode,
    PageRole, PageTemplateSet, Rect, Result, Transform, Unit,
};

pub(crate) fn resolve_page_layers(
    engine: &LayoutEngine<'_>,
    document: &DocumentIr,
    inherited_collision: &CollisionPolicy,
    context: &mut LayoutContext,
) -> Result<()> {
    validate_page_layer_ir(document)?;
    context.ensure_page(0)?;
    let total = context.pages.len();
    assign_page_roles(context, document, total);
    let page_rect = Rect::new(
        Unit::ZERO,
        Unit::ZERO,
        context.page_size.width,
        context.page_size.height,
    )?;
    for index in 0..total {
        for element in active_page_elements(document, index, total) {
            if context.sequence.saturating_add(tree_len(element)) > engine.limits.max_elements {
                return Err(FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    "resolved page-layer elements exceed configured element limit",
                ));
            }
            let mut clone = element.clone();
            clone.page_placement = None;
            substitute_page_numbers(&mut clone, index, total, engine.limits.max_text_bytes)?;
            engine.layout_list(
                std::slice::from_ref(&clone),
                document,
                page_rect,
                index,
                LayoutMode::Absolute,
                crate::Distribution::Start,
                Unit::ZERO,
                inherited_collision,
                Transform::IDENTITY,
                context,
            )?;
            if context.pages.len() != total {
                return Err(FileMakerError::new(
                    ErrorCode::LayoutInvalid,
                    "page-layer element attempted to change physical page count",
                ));
            }
        }
    }
    Ok(())
}

fn assign_page_roles(context: &mut LayoutContext, document: &DocumentIr, total: usize) {
    let templates = document
        .page_template
        .as_ref()
        .map(PageTemplateSet::from_base);
    for (index, page) in context.pages.iter_mut().enumerate() {
        page.role = physical_role(index, total);
        page.page_template = templates
            .as_ref()
            .and_then(|templates| templates.select(index, total))
            .cloned();
    }
}

fn physical_role(index: usize, total: usize) -> PageRole {
    if index == 0 {
        PageRole::First
    } else if index + 1 == total {
        PageRole::Last
    } else {
        PageRole::Continuation
    }
}

fn active_page_elements(
    document: &DocumentIr,
    index: usize,
    total: usize,
) -> impl Iterator<Item = &ElementIr> {
    document.elements.iter().filter(move |element| {
        element
            .page_placement
            .is_some_and(|placement| PageTemplateSet::role_is_active(placement.role, index, total))
    })
}

fn substitute_page_numbers(
    element: &mut ElementIr,
    index: usize,
    total: usize,
    max_text_bytes: usize,
) -> Result<()> {
    if let Some(text) = &mut element.text {
        *text = PageTemplateSet::number_text(text, index, total);
        crate::ResourceLimits::check("page-layer text bytes", text.len(), max_text_bytes)?;
    }
    for child in &mut element.children {
        substitute_page_numbers(child, index, total, max_text_bytes)?;
    }
    Ok(())
}

fn tree_len(element: &ElementIr) -> usize {
    element.children.iter().fold(1_usize, |total, child| {
        total.saturating_add(tree_len(child))
    })
}

pub(crate) fn validate_page_layer_ir(document: &DocumentIr) -> Result<()> {
    for element in &document.elements {
        let Some(placement) = element.page_placement else {
            reject_nested_page_placements(&element.children)?;
            continue;
        };
        if document.model != crate::source::ModelKind::Document {
            return Err(page_error(
                "page layers are valid only for the document model",
            ));
        }
        validate_layer_tree(element, placement, true)?;
    }
    Ok(())
}

fn reject_nested_page_placements(elements: &[ElementIr]) -> Result<()> {
    for element in elements {
        if element.page_placement.is_some() {
            return Err(
                page_error("page-layer placement is valid only on root elements")
                    .at(element.id.as_str()),
            );
        }
        reject_nested_page_placements(&element.children)?;
    }
    Ok(())
}

fn validate_layer_tree(
    element: &ElementIr,
    placement: crate::PagePlacement,
    root: bool,
) -> Result<()> {
    if (!root && element.page_placement.is_some())
        || element.kind == crate::ElementKind::Table
        || element.repeat.is_some()
        || element
            .geometry
            .anchors
            .values()
            .any(|target| !target.starts_with("guide:"))
    {
        return Err(page_error("invalid element in resolved page layer").at(element.id.as_str()));
    }
    let expected_layer = match placement.band {
        crate::PageBand::Background => "!page-background",
        crate::PageBand::Header | crate::PageBand::Footer => "~page-content",
    };
    if element.layer != expected_layer
        || element.collision.as_ref().is_none_or(|policy| {
            policy.enabled || policy.resolution != crate::CollisionResolution::Overlay
        })
    {
        return Err(
            page_error("page-layer paint or collision contract was changed")
                .at(element.id.as_str()),
        );
    }
    for child in &element.children {
        validate_layer_tree(child, placement, false)?;
    }
    Ok(())
}

fn page_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}
