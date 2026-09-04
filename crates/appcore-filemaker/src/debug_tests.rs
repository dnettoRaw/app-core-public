// =============================================================================
//        #######
//     ###       ###     F: debug_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{
    DebugOverlay, DebugOverlayOptions, MaskView, ResolvedPage, ResolvedScene, Size, Unit,
    ENGINE_VERSION,
};

#[test]
fn empty_overlay_does_not_mutate_scene() {
    let scene = ResolvedScene {
        template_id: "x".to_owned(),
        pages: vec![ResolvedPage {
            index: 0,
            role: crate::PageRole::First,
            size: Size::new(Unit::points(10).unwrap(), Unit::points(10).unwrap()).unwrap(),
            page_template: None,
            exclusions: Vec::new(),
            regions: Vec::new(),
            elements: Vec::new(),
        }],
        engine_version: ENGINE_VERSION.to_owned(),
    };
    let before = scene.clone();
    let overlay = DebugOverlay::build(
        &scene,
        0,
        &DebugOverlayOptions {
            grid: Some(Unit::points(5).unwrap()),
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
    assert_eq!(scene, before);
}
