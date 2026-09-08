// =============================================================================
//        #######
//     ###       ###     F: reflow.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 12:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Measures dense push reflow at its success/failure iteration boundary.

use std::fmt::Write;
use std::hint::black_box;

use appcore_filemaker::{
    Compiler, DataValue, DocumentIr, ErrorCode, FontManager, LayoutEngine, LayoutOptions,
    ResourceLimits, Unit,
};

pub(super) const CASES: [&str; 2] = ["reflow_dense_64", "reflow_limit_63"];

pub(super) fn run(selected: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    if selected.is_some_and(|name| !CASES.contains(&name)) {
        return Ok(());
    }
    let document = document()?;
    let fonts = FontManager::default();
    for (case, maximum) in CASES.into_iter().zip([64, 63]) {
        if selected.is_some_and(|name| name != case) {
            continue;
        }
        let limits = ResourceLimits {
            max_reflows: maximum,
            ..ResourceLimits::default()
        };
        super::measure(case, 10, || {
            let result =
                LayoutEngine::new(&limits, &fonts, LayoutOptions::default())?.resolve(&document);
            if maximum == 63 {
                let error = result.expect_err("last element must exhaust 63 attempts");
                assert_eq!(error.code(), ErrorCode::LayoutNonConvergent);
                assert!(error.to_string().contains("iteration limit exceeded"));
                black_box(error);
            } else {
                let scene = result?;
                assert_eq!(scene.pages.len(), 1);
                assert_eq!(scene.pages[0].elements.len(), 64);
                for (index, element) in scene.pages[0].elements.iter().enumerate() {
                    assert_eq!(
                        element.bounds.layout.origin.y,
                        Unit::points(index as i64 * 10)?
                    );
                }
                black_box(scene);
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn document() -> Result<DocumentIr, Box<dyn std::error::Error>> {
    // Identical proposed boxes force progressively more push attempts.
    let mut yaml = String::from(
        "filemaker: '1.0'\nmodel: canvas\nid: dense-reflow\npage: { width: 100pt, height: 1000pt }\nelements:\n",
    );
    for index in 0..64 {
        writeln!(
            yaml,
            "  - {{ id: box-{index:03}, type: rect, x: 0pt, y: 0pt, width: 10pt, height: 10pt }}"
        )?;
    }
    let compiler = Compiler::builder().build()?;
    let template = compiler.compile_template_yaml(yaml.as_bytes())?;
    Ok(compiler.bind(&template, &DataValue::Object(Default::default()), &[])?)
}
