// =============================================================================
//        #######
//     ###       ###     F: pdf_unicode.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

use appcore_filemaker::*;
use std::collections::BTreeMap;

fn scene(text: &str) -> (ResolvedScene, FontManager) {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(br#"filemaker: '1.0'
model: document
id: unicode-pdf
page: {width: 550pt, height: 160pt}
elements:
  - {id: text, type: text, width: 530pt, height: 140pt, binding: data.text, style: {font: Body, font_size: 12pt}, text_options: {overflow: error}}
"#).unwrap();
    let data = DataValue::Object(BTreeMap::from([(
        "text".into(),
        DataValue::String(text.into()),
    )]));
    let document = compiler.bind(&template, &data, &[]).unwrap();
    let mut fonts = FontManager::default();
    fonts
        .register(
            FontAsset::new(
                "Body",
                include_bytes!("../examples/assets/NotoSans-Regular.ttf").to_vec(),
                0,
            )
            .unwrap(),
        )
        .unwrap();
    let resolved = LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    (resolved, fonts)
}
fn pdf(scene: &ResolvedScene, fonts: &FontManager, mode: PdfMode) -> Vec<u8> {
    export_bytes(
        scene,
        &ExportRequest {
            format: ExportFormat::Pdf,
            pdf_mode: mode,
            ..ExportRequest::default()
        },
        &ExportContext {
            limits: &ResourceLimits::default(),
            fonts,
            assets: None,
        },
    )
    .unwrap()
    .0
}
#[test]
fn ligature_cmap_preserves_complete_unicode_sequences() {
    let (scene, fonts) = scene("fi fl ffi ffl office affinity flexible confirmation");
    assert!(scene.pages[0].elements[0]
        .text_layout
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .flat_map(|l| &l.runs)
        .any(|run| run.glyphs.len() < run.text.chars().count()));
    for mode in [PdfMode::Editable, PdfMode::Hybrid] {
        let bytes = pdf(&scene, &fonts, mode);
        let source = String::from_utf8_lossy(&bytes);
        for sequence in ["00660069", "0066006C", "006600660069", "00660066006C"] {
            assert!(
                source.contains(&format!("<{sequence}>")),
                "{mode:?}: missing sequence {sequence}"
            );
        }
        assert_eq!(bytes, pdf(&scene, &fonts, mode));
    }
}
#[test]
fn ambiguous_glyphs_keep_actual_logical_text_and_flattened_stays_outlines_only() {
    let (scene, fonts) = scene("fi ﬁ fl ﬂ ffi ﬃ ffl ﬄ café café x́");
    for mode in [PdfMode::Editable, PdfMode::Hybrid] {
        let bytes = pdf(&scene, &fonts, mode);
        assert!(String::from_utf8_lossy(&bytes).contains("/ActualText"));
    }
    let bytes = pdf(&scene, &fonts, PdfMode::Flattened);
    let source = String::from_utf8_lossy(&bytes);
    assert!(!source.contains("/ActualText"));
    assert!(!source.contains("/ToUnicode"));
}

#[test]
fn malformed_utf8_cluster_is_rejected_instead_of_silently_unmapped() {
    let (mut scene, fonts) = scene("é");
    scene.pages[0].elements[0]
        .text_layout
        .as_mut()
        .unwrap()
        .lines[0]
        .runs[0]
        .glyphs[0]
        .cluster = 1;
    let error = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Pdf,
            pdf_mode: PdfMode::Hybrid,
            ..ExportRequest::default()
        },
        &ExportContext {
            limits: &ResourceLimits::default(),
            fonts: &fonts,
            assets: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code(), ErrorCode::ExportUnsupported);
}
