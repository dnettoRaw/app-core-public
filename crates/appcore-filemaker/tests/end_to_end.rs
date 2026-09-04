// =============================================================================
//        #######
//     ###       ###     F: end_to_end.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeMap;

use appcore_filemaker::{
    export, preflight, CollisionMask, Compiler, DataValue, DebugOverlay, DebugOverlayOptions,
    ElementId, ExportCapabilities, ExportContext, ExportFormat, ExportRequest, Fidelity, FontAsset,
    FontManager, LayoutEngine, LayoutOptions, MaskView, OperationControl, Patch, PatchOperation,
    PdfMode, PreflightOptions, ResolvedScene, ResourceLimits, SceneInspector, Size, Unit,
};

const A4_REPORT: &[u8] = br"filemaker: '1.0'
model: document
id: multilingual-report
page:
  preset: A4
  margin: { top: 15mm, right: 15mm, bottom: 15mm, left: 15mm }
  master:
    background:
      - id: confidential
        type: text
        text: CONFIDENTIAL
        x: 5mm
        y: 120mm
        width: 200mm
        height: 30mm
        locked: true
        transform: { rotate: -35 }
        style: { font: Body, font_size: 42pt, color: '#cc3333', opacity: 320000 }
    footer:
      - id: page-number
        type: text
        text: 'Page {page}/{pages}'
        x: 15mm
        y: 282mm
        width: 180mm
        height: 8mm
        style: { font: Body, font_size: 8pt }
elements:
  - { id: pt, type: text, binding: data.pt, x: 15mm, y: 15mm, width: 180mm, height: 10mm, collision: false, style: { font: Body, font_size: 11pt } }
  - { id: fr, type: text, binding: data.fr, x: 15mm, y: 28mm, width: 180mm, height: 10mm, collision: false, style: { font: Body, font_size: 11pt } }
  - { id: ja, type: text, binding: data.ja, x: 15mm, y: 41mm, width: 180mm, height: 10mm, collision: false, style: { font: Body, font_size: 11pt } }
  - { id: ar, type: text, binding: data.ar, x: 15mm, y: 54mm, width: 180mm, height: 10mm, collision: false, style: { font: Body, font_size: 11pt } }
  - id: rows
    type: table
    binding: data.rows
    x: 15mm
    y: 72mm
    width: 180mm
    height: 190mm
    collision: false
    style: { font: Body, font_size: 9pt, stroke: '#444444', stroke_width: 0.5pt }
    table:
      columns:
        - { field: name, header: Name, width: { mode: flex, value: 1 } }
        - { field: note, header: Note, width: { mode: flex, value: 2 } }
      repeat_header: true
      header_height: 10mm
      row_height: 18mm
      max_rows: 20
      max_row_fields: 2
      max_cell_bytes: 128
";

const FHD_REFLOW: &[u8] = br"filemaker: '1.0'
model: canvas
id: fhd-runtime-patch
page: { preset: FHD }
elements:
  - { id: base, type: rect, x: 80px, y: 80px, width: 420px, height: 180px, style: { fill: '#336699' } }
  - { id: reflowed, type: rect, x: 80px, y: 80px, width: 420px, height: 180px, style: { fill: '#88aacc' } }
  - id: watermark
    type: text
    text: CONFIDENTIAL
    x: 300px
    y: 400px
    width: 1320px
    height: 180px
    collision: false
    transform: { rotate: -20 }
    style: { font: Body, font_size: 72pt, color: '#cc3333', opacity: 400000 }
";

#[test]
fn a4_table_multilingual_pages_patch_inspection_and_exports_complete() {
    let limits = ResourceLimits::default();
    let fonts = multilingual_fonts();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(A4_REPORT).unwrap();
    let patches = [Patch {
        sequence: 7,
        operations: vec![PatchOperation::SetText {
            id: ElementId::new("pt").unwrap(),
            text: "Relatorio operacional atualizado".to_owned(),
        }],
    }];
    let document = compiler.bind(&template, &report_data(), &patches).unwrap();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();

    assert!(scene.pages.len() >= 2);
    assert_eq!(scene.pages[0].size, template.page_size.unwrap());
    assert_multilingual_measurement(&scene);
    assert_page_masters(&scene);

    let inspector = SceneInspector::new(&scene);
    let pt = inspector
        .inspect_element(&ElementId::new("pt").unwrap())
        .unwrap();
    let resolved_pt = scene.pages[0]
        .elements
        .iter()
        .find(|element| element.id.as_str() == "pt")
        .unwrap();
    assert_eq!(
        resolved_pt.text.as_deref(),
        Some("Relatorio operacional atualizado")
    );
    assert!(pt.provenance.patches.contains(&7));
    assert!(!inspector
        .query_free_regions(
            0,
            Size::new(pt.bounds.layout.size.width, Unit::points(1).unwrap()).unwrap()
        )
        .unwrap()
        .is_empty());
    assert!(inspector
        .explain_layout(&ElementId::new("rows").unwrap())
        .unwrap()
        .decisions
        .iter()
        .any(|decision| decision.contains("page/reflow")));

    assert_debug_outputs(&scene);

    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let report = preflight(
        &scene,
        &ExportRequest::default(),
        &context,
        &PreflightOptions::default(),
        &OperationControl::default(),
    )
    .unwrap();
    assert!(!report.has_errors());
    for request in export_requests() {
        let mut output = Vec::new();
        let outcome = export(&scene, &request, &context, &mut output).unwrap();
        assert!(!output.is_empty(), "empty {:?} export", request.format);
        if request.pdf_mode == PdfMode::Hybrid {
            assert!(outcome
                .capabilities
                .contains(&ExportCapabilities::EditableText));
            assert!(output.windows(4).any(|window| window == b"3 Tr"));
        }
        if request.format == ExportFormat::Svg {
            let svg = String::from_utf8(output).unwrap();
            assert!(svg.contains("Arabic"));
            assert!(svg.contains("Japanese"));
        }
    }
}

fn assert_page_masters(scene: &ResolvedScene) {
    for (index, page) in scene.pages.iter().enumerate() {
        let watermark = page
            .elements
            .iter()
            .find(|element| element.id.as_str() == "confidential")
            .unwrap();
        assert!(!watermark.collidable);
        assert_ne!(watermark.transform, appcore_filemaker::Transform::IDENTITY);
        let number = page
            .elements
            .iter()
            .find(|element| element.id.as_str() == "page-number")
            .and_then(|element| element.text.as_deref())
            .unwrap();
        assert_eq!(number, format!("Page {}/{}", index + 1, scene.pages.len()));
    }
}

fn assert_debug_outputs(scene: &ResolvedScene) {
    for spacing in [1, 5, 10, 20] {
        let overlay = DebugOverlay::build(
            scene,
            0,
            &DebugOverlayOptions {
                grid: Some(Unit::points(spacing).unwrap()),
                ruler: false,
                ids: true,
                coordinates: false,
                bounds: true,
                anchors: false,
                regions: false,
                safe_area: true,
                collision: true,
                crosshair: false,
                view: MaskView::Combined,
            },
        )
        .unwrap();
        assert!(!overlay.primitives.is_empty());
    }
    let mask = CollisionMask::derive(scene, 0, MaskView::Combined).unwrap();
    assert!(!mask.to_json().unwrap().is_empty());
}

#[test]
fn fhd_collision_reflow_and_runtime_geometry_override_are_fresh() {
    let limits = ResourceLimits::default();
    let fonts = multilingual_fonts();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(FHD_REFLOW).unwrap();
    let patches = [Patch {
        sequence: 11,
        operations: vec![
            PatchOperation::Move {
                id: ElementId::new("base").unwrap(),
                x: "200px".parse().unwrap(),
                y: "120px".parse().unwrap(),
            },
            PatchOperation::Resize {
                id: ElementId::new("base").unwrap(),
                width: "600px".parse().unwrap(),
                height: "220px".parse().unwrap(),
            },
        ],
    }];
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &patches)
        .unwrap();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    let page = &scene.pages[0];
    assert_eq!(page.size.width, Unit::points(1440).unwrap());
    assert_eq!(page.size.height, Unit::points(810).unwrap());
    let base = page
        .elements
        .iter()
        .find(|value| value.id.as_str() == "base")
        .unwrap();
    let pushed = page
        .elements
        .iter()
        .find(|value| value.id.as_str() == "reflowed")
        .unwrap();
    assert_eq!(base.bounds.layout.origin.x, Unit::points(150).unwrap());
    assert_eq!(base.bounds.layout.size.width, Unit::points(450).unwrap());
    assert!(pushed.bounds.layout.origin.y > Unit::points(60).unwrap());
    assert!(pushed.layout_trace.reflowed);
    let watermark = page
        .elements
        .iter()
        .find(|value| value.id.as_str() == "watermark")
        .unwrap();
    assert!(!watermark.collidable);
    assert!(!watermark.layout_trace.reflowed);
}

fn multilingual_fonts() -> FontManager {
    let mut fonts = FontManager::default();
    for (name, bytes) in [
        (
            "Body",
            include_bytes!("../examples/assets/NotoSans-Regular.ttf").as_slice(),
        ),
        (
            "Arabic",
            include_bytes!("assets/NotoSansArabic-Test.ttf").as_slice(),
        ),
        (
            "Japanese",
            include_bytes!("assets/NotoSansJP-Test.ttf").as_slice(),
        ),
    ] {
        fonts
            .register(FontAsset::new(name, bytes.to_vec(), 0).unwrap())
            .unwrap();
    }
    fonts
        .set_fallback(vec!["Arabic".to_owned(), "Japanese".to_owned()])
        .unwrap();
    fonts
}

fn report_data() -> DataValue {
    let rows = (1..=12)
        .map(|index| {
            DataValue::Object(BTreeMap::from([
                ("name".to_owned(), DataValue::String(format!("Row {index}"))),
                (
                    "note".to_owned(),
                    DataValue::String("東京 العربية".to_owned()),
                ),
            ]))
        })
        .collect();
    DataValue::Object(BTreeMap::from([
        (
            "pt".to_owned(),
            DataValue::String("Relatorio operacional".to_owned()),
        ),
        (
            "fr".to_owned(),
            DataValue::String("Rapport operationnel".to_owned()),
        ),
        (
            "ja".to_owned(),
            DataValue::String("日本語の運用レポート".to_owned()),
        ),
        (
            "ar".to_owned(),
            DataValue::String("مرحبا بالعالم العربية ١٢٣".to_owned()),
        ),
        ("rows".to_owned(), DataValue::Array(rows)),
    ]))
}

fn assert_multilingual_measurement(scene: &appcore_filemaker::ResolvedScene) {
    let runs = scene
        .pages
        .iter()
        .flat_map(|page| &page.elements)
        .filter_map(|element| element.text_layout.as_ref())
        .flat_map(|layout| &layout.lines)
        .flat_map(|line| &line.runs)
        .collect::<Vec<_>>();
    assert!(runs.iter().any(|run| run.font == "Japanese"));
    assert!(runs.iter().any(|run| run.font == "Arabic" && run.rtl));
    assert!(runs.iter().all(|run| !run.glyphs.is_empty()));
}

fn export_requests() -> [ExportRequest; 7] {
    [
        ExportRequest {
            format: ExportFormat::Pdf,
            pdf_mode: PdfMode::Editable,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Pdf,
            pdf_mode: PdfMode::Flattened,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Pdf,
            pdf_mode: PdfMode::Hybrid,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Svg,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Html,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Png,
            page: Some(0),
            dpi: 36,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Jpeg,
            fidelity: Fidelity::BestEffort,
            page: Some(0),
            dpi: 36,
            ..ExportRequest::default()
        },
    ]
}
