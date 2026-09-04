// =============================================================================
//        #######
//     ###       ###     F: debug_inspect.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeMap;

use appcore_filemaker::{
    CollisionMask, Compiler, DataValue, DebugOverlay, DebugOverlayOptions, DebugPrimitive,
    ElementId, ErrorCode, FontManager, LayoutEngine, LayoutOptions, MaskView, ResourceLimits,
    SceneInspector, Size, Unit,
};

const TEMPLATE: &[u8] = br"filemaker: '1.0'
model: canvas
id: debug-inspect
page:
  width: 40pt
  height: 40pt
  safe: { top: 1pt, right: 2pt, bottom: 3pt, left: 4pt }
regions:
  panel: { x: 20pt, y: 20pt, width: 15pt, height: 15pt }
exclusions:
  reserved: { x: 30pt, y: 0pt, width: 5pt, height: 5pt }
elements:
  - id: first
    type: rect
    x: 0pt
    y: 0pt
    width: 10pt
    height: 10pt
    style: { stroke: '#000000', stroke_width: 2pt }
  - { id: pushed, type: rect, x: 0pt, y: 0pt, width: 10pt, height: 10pt }
  - id: anchored
    type: rect
    width: 5pt
    height: 5pt
    anchors: { left: first.right, top: first.top }
    collision: false
  - { id: in-panel, type: rect, region: panel, x: 0pt, y: 0pt, width: 5pt, height: 5pt }
  - { id: overflow, type: rect, x: 38pt, y: 30pt, width: 5pt, height: 5pt, collision: false }
";

#[test]
fn overlay_covers_debug_contract_without_mutating_layout() {
    let scene = scene();
    let before = scene.clone();
    for spacing in [1, 5, 10, 20] {
        let overlay = DebugOverlay::build(
            &scene,
            0,
            &DebugOverlayOptions {
                grid: Some(Unit::points(spacing).unwrap()),
                ruler: true,
                ids: true,
                coordinates: true,
                bounds: true,
                anchors: true,
                regions: true,
                safe_area: true,
                collision: true,
                crosshair: true,
                view: MaskView::Combined,
            },
        )
        .unwrap();
        assert!(!overlay.primitives.is_empty());
        if spacing == 5 {
            let labels = overlay
                .primitives
                .iter()
                .filter_map(|primitive| match primitive {
                    DebugPrimitive::Label { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            for expected in [
                "safe",
                "region:panel",
                "collision:first",
                "exclusion:reserved",
                "anchor:left=first.right",
                "first",
            ] {
                assert!(labels.contains(&expected), "missing debug label {expected}");
            }
            assert!(labels.iter().any(|label| label.ends_with("pt")));
        }
    }
    assert_eq!(scene, before);
    let invalid = DebugOverlay::build(
        &scene,
        0,
        &DebugOverlayOptions {
            grid: Some(Unit::from_raw(1)),
            ruler: false,
            ids: false,
            coordinates: false,
            bounds: false,
            anchors: false,
            regions: false,
            safe_area: false,
            collision: false,
            crosshair: false,
            view: MaskView::LayoutBounds,
        },
    )
    .unwrap_err();
    assert!(invalid.to_string().contains("1, 5, 10, or 20"));
}

#[test]
fn mask_views_derive_their_own_occupied_and_free_geometry() {
    let scene = scene();
    let collision = CollisionMask::derive(&scene, 0, MaskView::CollisionMask).unwrap();
    let layout = CollisionMask::derive(&scene, 0, MaskView::LayoutBounds).unwrap();
    let combined = CollisionMask::derive(&scene, 0, MaskView::Combined).unwrap();

    assert!(collision
        .occupied
        .iter()
        .any(|(id, _)| id.as_str() == "exclusion.reserved"));
    assert!(!collision
        .occupied
        .iter()
        .any(|(id, _)| id.as_str() == "overflow"));
    assert!(layout
        .occupied
        .iter()
        .any(|(id, _)| id.as_str() == "overflow"));
    assert!(combined.occupied.len() > layout.occupied.len());
    assert_ne!(collision.free, layout.free);
    assert!(combined
        .collisions
        .iter()
        .all(|(left, right, _)| left != right));
    assert!(combined.overflow.iter().any(|id| id.as_str() == "overflow"));

    let json: serde_json::Value = serde_json::from_slice(&combined.to_json().unwrap()).unwrap();
    for key in ["occupied", "free", "collisions", "overflow"] {
        assert!(json.get(key).is_some(), "missing mask JSON field {key}");
    }
}

#[test]
fn inspection_retains_source_geometry_measurement_collision_and_reflow() {
    let scene = scene();
    let inspector = SceneInspector::new(&scene);
    let explanation = inspector
        .explain_layout(&ElementId::new("pushed").unwrap())
        .unwrap();
    assert!(explanation.trace.reflowed);
    assert_eq!(explanation.trace.initial_page, 0);
    assert_eq!(explanation.trace.proposed.origin.y, Unit::ZERO);
    assert_eq!(
        explanation.bounds.layout.origin.y,
        Unit::points(10).unwrap()
    );
    for decision in ["source x=", "measurement", "collision", "page/reflow"] {
        assert!(explanation
            .decisions
            .iter()
            .any(|entry| entry.contains(decision)));
    }

    let anchored = inspector
        .inspect_element(&ElementId::new("anchored").unwrap())
        .unwrap();
    assert_eq!(
        anchored.layout_trace.geometry.anchors["left"],
        "first.right"
    );
    let page = inspector.inspect_page(0).unwrap();
    assert_eq!(page.regions, ["panel"]);
    assert_eq!(page.exclusions, ["reserved"]);
    assert_eq!(page.safe.unwrap().origin.x, Unit::points(4).unwrap());

    let free = inspector
        .query_free_regions(
            0,
            Size::new(Unit::points(1).unwrap(), Unit::points(1).unwrap()).unwrap(),
        )
        .unwrap();
    assert!(!free.is_empty());
}

#[test]
fn diagnostic_geometry_fails_closed_at_the_caller_budget() {
    let scene = scene();
    let limits = ResourceLimits {
        max_preflight_comparisons: 1,
        ..ResourceLimits::default()
    };
    let options = DebugOverlayOptions {
        grid: Some(Unit::points(1).unwrap()),
        ruler: true,
        ids: true,
        coordinates: true,
        bounds: true,
        anchors: true,
        regions: true,
        safe_area: true,
        collision: true,
        crosshair: true,
        view: MaskView::Combined,
    };
    assert_eq!(
        DebugOverlay::build_bounded(&scene, 0, &options, &limits)
            .unwrap_err()
            .code(),
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        CollisionMask::derive_bounded(&scene, 0, MaskView::Combined, &limits)
            .unwrap_err()
            .code(),
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        SceneInspector::new(&scene)
            .query_free_regions_bounded(
                0,
                Size::new(Unit::points(1).unwrap(), Unit::points(1).unwrap()).unwrap(),
                &limits,
            )
            .unwrap_err()
            .code(),
        ErrorCode::LimitExceeded
    );
}

fn scene() -> appcore_filemaker::ResolvedScene {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(TEMPLATE).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap()
}
