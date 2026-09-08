// =============================================================================
//        #######
//     ###       ###     F: page_query.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Serializes page inspection directly from resolved scene storage.

use appcore_filemaker::{
    ErrorCode, FileMakerError, Rect, ResolvedElement, ResolvedExclusion, ResolvedRegion, Unit,
};
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::{error::json_error, BridgeError, BridgeResult, FileMakerAiSession};

#[derive(Serialize)]
struct PageInspectionView<'a> {
    page: usize,
    role: appcore_filemaker::PageRole,
    exclusions: ExclusionNames<'a>,
    regions: RegionNames<'a>,
    safe: Option<Rect>,
    elements: usize,
    occupied: Option<Rect>,
    overflow: OverflowIds<'a>,
}

struct ExclusionNames<'a>(&'a [ResolvedExclusion]);
struct RegionNames<'a>(&'a [ResolvedRegion]);

struct OverflowIds<'a> {
    elements: &'a [ResolvedElement],
    page_bounds: Rect,
    page_right: Unit,
    page_bottom: Unit,
    count: usize,
}

impl Serialize for ExclusionNames<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for exclusion in self.0 {
            sequence.serialize_element(&exclusion.name)?;
        }
        sequence.end()
    }
}

impl Serialize for RegionNames<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for region in self.0 {
            sequence.serialize_element(&region.name)?;
        }
        sequence.end()
    }
}

impl Serialize for OverflowIds<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.count))?;
        for element in self.elements {
            if is_outside(
                self.page_bounds,
                self.page_right,
                self.page_bottom,
                element.bounds.visual,
            ) {
                sequence.serialize_element(&element.id)?;
            }
        }
        sequence.end()
    }
}

pub(crate) fn inspect(session: &FileMakerAiSession, page: usize) -> BridgeResult<Value> {
    let scene = session.resolve()?;
    let page_ref = scene.pages.get(page).ok_or_else(|| {
        BridgeError::Core(FileMakerError::new(
            ErrorCode::LayoutInvalid,
            format!("page {page} was not found"),
        ))
    })?;
    let page_bounds = Rect::new(
        Unit::ZERO,
        Unit::ZERO,
        page_ref.size.width,
        page_ref.size.height,
    )?;
    let page_right = page_bounds.right()?;
    let page_bottom = page_bounds.bottom()?;
    let mut occupied: Option<Rect> = None;
    let mut overflow = 0_usize;
    for element in &page_ref.elements {
        occupied = Some(match occupied {
            Some(bounds) => bounds.union(element.bounds.layout)?,
            None => element.bounds.layout,
        });
        if !contains_rect(page_bounds, element.bounds.visual)? {
            overflow += 1;
        }
    }
    let view = PageInspectionView {
        page,
        role: page_ref.role,
        exclusions: ExclusionNames(&page_ref.exclusions),
        regions: RegionNames(&page_ref.regions),
        safe: page_ref
            .page_template
            .as_ref()
            .map(appcore_filemaker::PageTemplate::safe_bounds)
            .transpose()?,
        elements: page_ref.elements.len(),
        occupied,
        overflow: OverflowIds {
            elements: &page_ref.elements,
            page_bounds,
            page_right,
            page_bottom,
            count: overflow,
        },
    };
    crate::session::enforce_result_limit(&view, session.result_limit())?;
    serde_json::to_value(view).map_err(json_error)
}

fn contains_rect(outer: Rect, inner: Rect) -> appcore_filemaker::Result<bool> {
    Ok(inner.origin.x >= outer.origin.x
        && inner.origin.y >= outer.origin.y
        && inner.right()? <= outer.right()?
        && inner.bottom()? <= outer.bottom()?)
}

fn is_outside(outer: Rect, outer_right: Unit, outer_bottom: Unit, inner: Rect) -> bool {
    inner.origin.x < outer.origin.x
        || inner.origin.y < outer.origin.y
        || inner.right().map_or(true, |right| right > outer_right)
        || inner.bottom().map_or(true, |bottom| bottom > outer_bottom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore_filemaker::{Compiler, DataValue, FontManager, ResourceLimits, SceneInspector};

    #[test]
    fn borrowed_page_view_matches_core_inspection_exactly() {
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler
            .compile_template_yaml(
                br"filemaker: '1.0'
model: canvas
id: borrowed-page
page: { width: 100pt, height: 80pt }
elements:
  - { id: inside, type: rect, x: 10pt, y: 10pt, width: 20pt, height: 20pt }
  - { id: outside, type: rect, x: 95pt, y: 10pt, width: 20pt, height: 20pt, collision: false }
",
            )
            .unwrap();
        let document = compiler
            .bind(&template, &DataValue::Object(Default::default()), &[])
            .unwrap();
        let session = FileMakerAiSession::new(
            document,
            ResourceLimits::default(),
            FontManager::default(),
            None,
            crate::AiBridgePolicy::default(),
        )
        .unwrap();
        let scene = session.resolve().unwrap();
        let expected =
            serde_json::to_value(SceneInspector::new(&scene).inspect_page(0).unwrap()).unwrap();
        assert_eq!(inspect(&session, 0).unwrap(), expected);
    }
}
