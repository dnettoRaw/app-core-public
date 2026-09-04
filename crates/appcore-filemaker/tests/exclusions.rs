// =============================================================================
//        #######
//     ###       ###     F: exclusions.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeMap;

use appcore_filemaker::{
    preflight, CollisionMask, Compiler, DataValue, ElementId, ErrorCode, ExportContext,
    ExportRequest, FontManager, LayoutEngine, LayoutOptions, MaskView, OperationControl,
    PreflightOptions, ResourceLimits, SceneInspector, Size, Unit,
};

#[test]
fn named_exclusions_seed_every_page_before_geometry_reflow() {
    let yaml = br"filemaker: '1.0'
model: document
id: exclusions
page: { width: 100pt, height: 100pt }
exclusions:
  reserved-header: { x: 0pt, y: 0pt, width: 100%, height: 20pt }
elements:
  - { id: first, type: rect, x: 0pt, y: 0pt, width: 10pt, height: 10pt }
  - { id: overflow, type: rect, x: 0pt, y: 90pt, width: 10pt, height: 20pt }
";
    let scene = compile(yaml).unwrap();
    assert_eq!(scene.pages.len(), 2);
    for (page_index, id) in [(0, "first"), (1, "overflow")] {
        let page = &scene.pages[page_index];
        assert_eq!(page.exclusions.len(), 1);
        assert_eq!(page.exclusions[0].name, "reserved-header");
        assert_eq!(
            page.elements
                .iter()
                .find(|element| element.id.as_str() == id)
                .unwrap()
                .bounds
                .layout
                .origin
                .y,
            Unit::points(20).unwrap()
        );
    }
    let inspection = SceneInspector::new(&scene).inspect_page(0).unwrap();
    assert_eq!(inspection.exclusions, ["reserved-header"]);
    let mask = CollisionMask::derive(&scene, 0, MaskView::CollisionMask).unwrap();
    assert!(mask
        .occupied
        .iter()
        .any(|(id, _)| id.as_str() == "exclusion.reserved-header"));
    assert!(String::from_utf8(mask.to_json().unwrap())
        .unwrap()
        .contains("exclusion.reserved-header"));
    let free = SceneInspector::new(&scene)
        .query_free_regions(
            0,
            Size::new(Unit::points(1).unwrap(), Unit::points(1).unwrap()).unwrap(),
        )
        .unwrap();
    assert!(!free.iter().any(|region| contains(region, 5, 5)));
    let limits = ResourceLimits::default();
    let fonts = FontManager::default();
    assert!(preflight(
        &scene,
        &ExportRequest::default(),
        &ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        },
        &PreflightOptions {
            strict: true,
            ..PreflightOptions::default()
        },
        &OperationControl::default(),
    )
    .unwrap()
    .issues
    .is_empty());
}

#[test]
fn exclusion_collision_groups_are_symmetric_and_explicit() {
    let yaml = br"filemaker: '1.0'
model: document
id: exclusion-groups
page: { width: 100pt, height: 100pt }
exclusions:
  content-header:
    x: 0pt
    y: 0pt
    width: 100pt
    height: 20pt
    collides_with: [content]
elements:
  - { id: overlay, type: rect, x: 0pt, y: 0pt, width: 10pt, height: 10pt, collision: false }
  - id: content
    type: rect
    x: 20pt
    y: 0pt
    width: 10pt
    height: 10pt
    collision: { group: content }
";
    let scene = compile(yaml).unwrap();
    let page = &scene.pages[0];
    assert_eq!(origin_y(page, "overlay"), Unit::ZERO);
    assert_eq!(origin_y(page, "content"), Unit::points(20).unwrap());
    assert_eq!(page.elements.len(), 2, "exclusions are never paint nodes");
}

#[test]
fn invalid_or_out_of_page_exclusions_fail_explicitly() {
    let auto = br"filemaker: '1.0'
model: document
id: auto-exclusion
page: { width: 100pt, height: 100pt }
exclusions:
  invalid: { x: 0pt, y: 0pt, width: auto, height: 10pt }
";
    let error = Compiler::builder()
        .build()
        .unwrap()
        .compile_template_yaml(auto)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::SchemaField);

    let outside = br"filemaker: '1.0'
model: document
id: outside-exclusion
page: { width: 100pt, height: 100pt }
exclusions:
  invalid: { x: 90pt, y: 0pt, width: 20pt, height: 10pt }
";
    let error = compile(outside).unwrap_err();
    assert_eq!(error.code(), ErrorCode::LayoutInvalid);
}

#[test]
fn repeated_exclusion_instances_share_the_global_geometry_budget() {
    let yaml = br"filemaker: '1.0'
model: document
id: bounded-exclusions
page: { width: 100pt, height: 100pt }
exclusions:
  full-width: { x: 0pt, y: 0pt, width: 100pt, height: 20pt }
elements:
  - id: never-placeable
    type: rect
    width: 10pt
    height: 10pt
    collision: { policy: next_page }
";
    let limits = ResourceLimits {
        max_elements: 2,
        ..ResourceLimits::default()
    };
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let error = LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LimitExceeded);
}

fn compile(yaml: &[u8]) -> appcore_filemaker::Result<appcore_filemaker::ResolvedScene> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(yaml)?;
    let document = compiler.bind(&template, &DataValue::Object(BTreeMap::new()), &[])?;
    LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())?
        .resolve(&document)
}

fn origin_y(page: &appcore_filemaker::ResolvedPage, id: &str) -> Unit {
    page.elements
        .iter()
        .find(|element| element.id == ElementId::new(id).unwrap())
        .unwrap()
        .bounds
        .layout
        .origin
        .y
}

fn contains(region: &appcore_filemaker::Rect, x: i64, y: i64) -> bool {
    let x = Unit::points(x).unwrap();
    let y = Unit::points(y).unwrap();
    x >= region.origin.x
        && y >= region.origin.y
        && x < region.right().unwrap()
        && y < region.bottom().unwrap()
}
