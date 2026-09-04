// =============================================================================
//        #######
//     ###       ###     F: layout_exclusion.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeSet;

use crate::{
    CollisionBounds, CollisionPolicy, CollisionResolution, DocumentIr, ErrorCode, FileMakerError,
    Rect, ResolvedExclusion, Result, Size, Unit,
};

pub(crate) fn resolve_exclusions(
    document: &DocumentIr,
    page_size: Size,
    logical_unit: Unit,
) -> Result<Vec<(ResolvedExclusion, CollisionPolicy)>> {
    let page = Rect::new(Unit::ZERO, Unit::ZERO, page_size.width, page_size.height)?;
    document
        .exclusions
        .iter()
        .map(|(name, source)| {
            let bounds = Rect::new(
                required(source.x.resolve(page_size.width, logical_unit)?, "x")?,
                required(source.y.resolve(page_size.height, logical_unit)?, "y")?,
                required(
                    source.width.resolve(page_size.width, logical_unit)?,
                    "width",
                )?,
                required(
                    source.height.resolve(page_size.height, logical_unit)?,
                    "height",
                )?,
            )?;
            if !contains(page, bounds)? {
                return Err(exclusion_error(format!(
                    "exclusion `{name}` must remain inside the page trim box"
                )));
            }
            Ok((
                ResolvedExclusion {
                    name: name.clone(),
                    bounds,
                    group: source.group.clone(),
                    collides_with: source.collides_with.clone(),
                },
                CollisionPolicy {
                    enabled: true,
                    group: source.group.clone(),
                    collides_with: source.collides_with.clone(),
                    ignore: BTreeSet::new(),
                    priority: i32::MAX,
                    movable: false,
                    bounds: CollisionBounds::Layout,
                    resolution: CollisionResolution::Error,
                },
            ))
        })
        .collect()
}

fn required(value: Option<Unit>, axis: &str) -> Result<Unit> {
    value.ok_or_else(|| exclusion_error(format!("exclusion {axis} cannot be auto")))
}

fn contains(outer: Rect, inner: Rect) -> Result<bool> {
    Ok(inner.origin.x >= outer.origin.x
        && inner.origin.y >= outer.origin.y
        && inner.right()? <= outer.right()?
        && inner.bottom()? <= outer.bottom()?)
}

fn exclusion_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}
