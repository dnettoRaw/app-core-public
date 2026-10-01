// =============================================================================
//        #######
//     ###       ###     F: layout_keep.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Estimates bounded contiguous keep-with-next blocks in vertical flows.

use crate::{
    ElementIr, ErrorCode, FileMakerError, FontManager, LayoutMode, Point, Rect, Result, Unit,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn block_height(
    elements: &[&ElementIr],
    container: Rect,
    flow_x: Unit,
    flow_y: Unit,
    gap: Unit,
    logical_unit: Unit,
    positions: &std::collections::BTreeMap<String, (usize, Rect)>,
    guides: &std::collections::BTreeMap<String, crate::Length>,
    fonts: &FontManager,
) -> Result<Unit> {
    let mut cursor = flow_y;
    for element in elements {
        if element.geometry.y.is_some()
            || !element.geometry.anchors.is_empty()
            || element.kind == crate::ElementKind::Table
            || !element.children.is_empty()
        {
            return Err(keep_error(
                "keep_with_next blocks require sibling elements without y anchors, tables, or children",
            ));
        }
        let proposed = crate::layout_geometry::propose_rect(
            element,
            container,
            LayoutMode::FlowVertical,
            Point {
                x: flow_x,
                y: cursor,
            },
            positions,
            guides,
            logical_unit,
        )?;
        let (style, layout, _) =
            crate::layout_measure::measure_content(element, proposed, fonts, logical_unit)?;
        let height = if element.kind == crate::ElementKind::Text
            && element.text_options.overflow == crate::TextOverflow::Expand
        {
            layout
                .ok_or_else(|| keep_error("expanded keep block has no measured text"))?
                .measured
                .height
                .max(proposed.size.height)
        } else {
            let _ = style;
            proposed.size.height
        };
        cursor = cursor.checked_add(height)?.checked_add(gap)?;
    }
    cursor.checked_sub(flow_y)?.checked_sub(gap)
}

pub(crate) fn should_defer(height: Unit, flow_y: Unit, container: Rect) -> Result<bool> {
    if height > container.size.height || flow_y <= container.origin.y {
        return Ok(false);
    }
    Ok(flow_y.checked_add(height)? > container.bottom()?)
}

pub(crate) fn validate_flow(mode: LayoutMode, element: &ElementIr) -> Result<()> {
    if element.keep_with_next && mode != LayoutMode::FlowVertical {
        return Err(
            keep_error("keep_with_next is supported only inside a vertical flow")
                .at(element.id.as_str()),
        );
    }
    Ok(())
}

pub(crate) fn next_page(page: usize) -> Result<usize> {
    page.checked_add(1)
        .ok_or_else(|| keep_error("keep block page index overflow"))
}

fn keep_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}
