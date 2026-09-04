// =============================================================================
//        #######
//     ###       ###     F: layout_region.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{DocumentIr, ElementIr, Rect, RegionIr, ResolvedRegion, Result, Unit};

pub(crate) fn resolve_region(
    element: &ElementIr,
    document: &DocumentIr,
    fallback: Rect,
    logical_unit: Unit,
) -> Result<Rect> {
    let Some(name) = &element.geometry.region else {
        return Ok(fallback);
    };
    let region = document
        .regions
        .get(name)
        .ok_or_else(|| region_error(format!("region `{name}` was not found")))?;
    resolve_bounds(region, fallback, logical_unit)
}

pub(crate) fn resolve_regions(
    document: &DocumentIr,
    page: Rect,
    logical_unit: Unit,
) -> Result<Vec<ResolvedRegion>> {
    document
        .regions
        .iter()
        .map(|(name, region)| {
            Ok(ResolvedRegion {
                name: name.clone(),
                bounds: resolve_bounds(region, page, logical_unit)?,
            })
        })
        .collect()
}

fn resolve_bounds(region: &RegionIr, fallback: Rect, logical_unit: Unit) -> Result<Rect> {
    let x = explicit(region.x.resolve(fallback.size.width, logical_unit)?, "x")?;
    let y = explicit(region.y.resolve(fallback.size.height, logical_unit)?, "y")?;
    let width = explicit(
        region.width.resolve(fallback.size.width, logical_unit)?,
        "width",
    )?;
    let height = explicit(
        region.height.resolve(fallback.size.height, logical_unit)?,
        "height",
    )?;
    Rect::new(
        fallback.origin.x.checked_add(x)?,
        fallback.origin.y.checked_add(y)?,
        width,
        height,
    )
}

fn explicit(value: Option<Unit>, axis: &str) -> Result<Unit> {
    value.ok_or_else(|| region_error(format!("region {axis} cannot be auto")))
}

fn region_error(message: impl Into<String>) -> crate::FileMakerError {
    crate::FileMakerError::new(crate::ErrorCode::LayoutInvalid, message)
}
