// =============================================================================
//        #######
//     ###       ###     F: source_page_build.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded source page build contracts and behavior for this crate.

use crate::source::{EdgeSource, ElementSource, PageSource};
use crate::{
    CollisionPolicy, CollisionResolution, ElementIr, ErrorCode, FileMakerError, Orientation,
    PresetRegistry, ResourceLimits, Result, Size, Unit,
};

type ElementConverter = fn(&ElementSource, &str, &ResourceLimits) -> Result<ElementIr>;

pub(crate) fn validate_page(page: Option<&PageSource>) -> Result<()> {
    let Some(page) = page else {
        return Ok(());
    };
    if page.preset.is_some() && (page.width.is_some() || page.height.is_some()) {
        return Err(schema_error(
            "page preset cannot be combined with explicit width/height",
        ));
    }
    if page.preset.is_none() && (page.width.is_some() != page.height.is_some()) {
        return Err(schema_error(
            "explicit page width and height must be supplied together",
        ));
    }
    Ok(())
}

pub(crate) fn validate_page_layer_elements(elements: &[ElementSource]) -> Result<()> {
    let mut stack: Vec<&ElementSource> = elements.iter().rev().collect();
    while let Some(element) = stack.pop() {
        if element.collision.is_some() || element.repeat.is_some() {
            return Err(
                schema_error("page-layer elements cannot declare collision or repeat")
                    .at(element.id.clone()),
            );
        }
        if element.element_type == "table" {
            return Err(
                schema_error("page-layer elements cannot contain tables").at(element.id.clone())
            );
        }
        if element
            .anchors
            .values()
            .any(|target| !target.starts_with("guide:"))
        {
            return Err(
                schema_error("page-layer anchors may target only named guides")
                    .at(element.id.clone()),
            );
        }
        stack.extend(element.children.iter().rev());
    }
    Ok(())
}

pub(crate) fn append_page_layer_elements(
    page: &PageSource,
    limits: &ResourceLimits,
    target: &mut Vec<ElementIr>,
    convert: ElementConverter,
) -> Result<()> {
    for (placement, sources) in page.placed_element_lists() {
        for source in sources {
            let mut element = convert(source, "page", limits)?;
            element.page_placement = Some(placement);
            apply_page_contract(&mut element, placement);
            target.push(element);
        }
    }
    Ok(())
}

fn apply_page_contract(element: &mut ElementIr, placement: crate::PagePlacement) {
    element.collision = Some(CollisionPolicy {
        enabled: false,
        resolution: CollisionResolution::Overlay,
        ..CollisionPolicy::default()
    });
    element.layer = match placement.band {
        crate::PageBand::Background => "!page-background".to_owned(),
        crate::PageBand::Header | crate::PageBand::Footer => "~page-content".to_owned(),
    };
    for child in &mut element.children {
        apply_page_contract(child, placement);
    }
}

pub(crate) fn resolve_page(
    template_id: &str,
    page: Option<&PageSource>,
    presets: &PresetRegistry,
) -> Result<(Option<Size>, Orientation, Option<crate::PageTemplate>)> {
    let Some(page) = page else {
        return Ok((None, Orientation::Portrait, None));
    };
    let size = resolve_size(page, presets)?;
    let page_template = size
        .map(|size| {
            Ok(crate::PageTemplate {
                name: template_id.to_owned(),
                role: crate::PageRole::Continuation,
                size,
                margin: resolve_edges(&page.margin)?,
                bleed: resolve_edges(&page.bleed)?,
                safe: resolve_edges(&page.safe)?,
                crop_marks: page.crop_marks,
            })
        })
        .transpose()?;
    if let Some(template) = &page_template {
        template.content_bounds()?;
        template.safe_bounds()?;
    }
    Ok((size, page.orientation, page_template))
}

fn resolve_size(page: &PageSource, presets: &PresetRegistry) -> Result<Option<Size>> {
    if let Some(name) = &page.preset {
        return Ok(Some(presets.get(name)?.oriented_size(page.orientation)));
    }
    let (Some(width), Some(height)) = (page.width, page.height) else {
        return Ok(None);
    };
    let width = width
        .resolve(Unit::ZERO, Unit::ZERO)?
        .ok_or_else(|| schema_error("page width cannot be auto"))?;
    let height = height
        .resolve(Unit::ZERO, Unit::ZERO)?
        .ok_or_else(|| schema_error("page height cannot be auto"))?;
    Size::new(width, height).map(Some)
}

fn resolve_edges(source: &EdgeSource) -> Result<crate::Insets> {
    let edge = |value: Option<crate::Length>| -> Result<Unit> {
        value.map_or(Ok(Unit::ZERO), |value| {
            value
                .resolve(Unit::ZERO, Unit::ZERO)?
                .ok_or_else(|| schema_error("page edge cannot be auto"))
        })
    };
    Ok(crate::Insets {
        top: edge(source.top)?,
        right: edge(source.right)?,
        bottom: edge(source.bottom)?,
        left: edge(source.left)?,
    })
}

fn schema_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::SchemaField, message)
}
