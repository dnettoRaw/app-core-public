// =============================================================================
//        #######
//     ###       ###     F: reflow.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 12:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::fmt::Write;

use appcore_filemaker::{
    Compiler, DataValue, DocumentIr, ErrorCode, FontManager, LayoutEngine, LayoutOptions,
    ResourceLimits, Unit,
};

fn bind(yaml: &str) -> DocumentIr {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
    compiler
        .bind(&template, &DataValue::Object(Default::default()), &[])
        .unwrap()
}

#[test]
fn dense_push_succeeds_at_exact_attempt_boundary_and_rejects_one_less() {
    let mut yaml = String::from(
        "filemaker: '1.0'\nmodel: canvas\nid: reflow-boundary\npage: { width: 100pt, height: 1000pt }\nelements:\n",
    );
    for index in 0..64 {
        writeln!(
            yaml,
            "  - {{ id: box-{index:03}, type: rect, x: 0pt, y: 0pt, width: 10pt, height: 10pt }}"
        )
        .unwrap();
    }
    let document = bind(&yaml);
    let before = document.clone();
    let fonts = FontManager::default();
    for maximum in [63, 64] {
        let limits = ResourceLimits {
            max_reflows: maximum,
            ..ResourceLimits::default()
        };
        let result = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document);
        if maximum == 63 {
            let error = result.unwrap_err();
            assert_eq!(error.code(), ErrorCode::LayoutNonConvergent);
            assert!(error.to_string().contains("iteration limit exceeded"));
        } else {
            let scene = result.unwrap();
            assert_eq!(scene.pages.len(), 1);
            assert_eq!(scene.pages[0].elements.len(), 64);
            for (index, element) in scene.pages[0].elements.iter().enumerate() {
                assert_eq!(
                    element.bounds.layout.origin.y,
                    Unit::points(index as i64 * 10).unwrap()
                );
            }
        }
        assert_eq!(document, before);
    }
}

#[test]
fn negative_gap_cannot_create_backwards_push_cycles() {
    let limits = ResourceLimits::default();
    let fonts = FontManager::default();
    let options = LayoutOptions {
        collision_gap: Unit::from_raw(-1),
        ..LayoutOptions::default()
    };
    let error = LayoutEngine::new(&limits, &fonts, options).err().unwrap();
    assert_eq!(error.code(), ErrorCode::LayoutInvalid);
}

#[test]
fn enclosed_shrink_stops_at_minimum_instead_of_spending_all_attempts() {
    let document = bind("filemaker: '1.0'\nmodel: canvas\nid: shrink-floor\npage: { width: 100pt, height: 100pt }\nelements:\n  - { id: fixed, type: rect, width: 20pt, height: 20pt }\n  - { id: shrinking, type: rect, width: 10pt, height: 10pt, collision: { policy: shrink } }\n");
    let limits = ResourceLimits::default();
    let fonts = FontManager::default();
    let error = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LayoutNonConvergent);
    assert!(error.to_string().contains("shrink reached minimum size"));
}
