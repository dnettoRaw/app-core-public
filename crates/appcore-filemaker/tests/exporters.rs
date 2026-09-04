// =============================================================================
//        #######
//     ###       ###     F: exporters.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeMap;

use appcore_filemaker::{
    export, export_bytes, export_collision_mask, export_dataset_csv, export_dataset_csv_bytes,
    preflight, CollisionMask, Color, ColumnWidth, Compiler, ComputedStyle, DataValue, ElementKind,
    ExportCapabilities, ExportContext, ExportFormat, ExportLossKind, ExportRequest,
    ExportStyleOverride, Fidelity, FontAsset, FontManager, GlyphRun, HtmlMode, InMemoryDataset,
    LayoutEngine, LayoutOptions, MaskFormat, MaskView, MemoryResolver, OperationControl, PdfMode,
    PreflightOptions, Rect, ResolvedTableCell, ResolvedTableColumn, ResolvedTableFragment,
    ResolvedTableRow, ResourceLimits, TableColumn, TableSpec, TextDiagnostic, TextLayout, TextLine,
    Unit,
};

fn scene() -> (
    appcore_filemaker::ResolvedScene,
    ResourceLimits,
    FontManager,
) {
    let yaml = r"filemaker: '1.0'
model: canvas
id: export-test
page: { width: 40pt, height: 30pt }
elements:
  - id: box
    type: rect
    x: 2pt
    y: 3pt
    width: 10pt
    height: 8pt
    style: { fill: '#336699' }
"
    .as_bytes();
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    (scene, limits, fonts)
}

fn path_scene() -> (
    appcore_filemaker::ResolvedScene,
    ResourceLimits,
    FontManager,
) {
    let yaml = br"filemaker: '1.0'
model: canvas
id: path-test
page: { width: 40pt, height: 30pt }
elements:
  - id: curve
    type: path
    x: 2pt
    y: 3pt
    width: 20pt
    height: 10pt
    path:
      - { command: move, x: 0%, y: 100% }
      - { command: curve, x1: 25%, y1: 0%, x2: 75%, y2: 0%, x: 100%, y: 100% }
    style: { stroke: '#336699', stroke_width: 1pt }
";
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    (scene, limits, fonts)
}

fn transformed_scene() -> (
    appcore_filemaker::ResolvedScene,
    ResourceLimits,
    FontManager,
) {
    let yaml = br"filemaker: '1.0'
model: canvas
id: transform-export
collision: false
page: { width: 30pt, height: 20pt }
elements:
  - id: shifted
    type: rect
    x: 2pt
    y: 2pt
    width: 4pt
    height: 4pt
    transform: { translate_x: 10pt, rotate: 90, origin_x: 0%, origin_y: 0% }
    style: { fill: '#000000' }
";
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    (scene, limits, fonts)
}

fn hybrid_text_scene() -> (
    appcore_filemaker::ResolvedScene,
    ResourceLimits,
    FontManager,
) {
    let yaml = br"filemaker: '1.0'
model: canvas
id: hybrid-pdf
page: { width: 80pt, height: 30pt }
elements:
  - id: searchable
    type: text
    text: Searchable PDF
    x: 4pt
    y: 4pt
    width: 70pt
    height: 18pt
    style: { font: Resolved Test, font_size: 10pt }
";
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let mut fonts = FontManager::default();
    register_resolved_test_font(&mut fonts);
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    (scene, limits, fonts)
}

fn resolved_text_scene() -> (
    appcore_filemaker::ResolvedScene,
    ResourceLimits,
    FontManager,
) {
    let (mut scene, limits, mut fonts) = scene();
    register_resolved_test_font(&mut fonts);
    let element = &mut scene.pages[0].elements[0];
    element.kind = ElementKind::Text;
    element.text = Some("unresolved original".to_owned());
    element.style.font = None;
    element.bounds.clip = Some(element.bounds.layout);
    element.text_layout = Some(TextLayout {
        writing_mode: appcore_filemaker::WritingMode::Horizontal,
        lines: vec![TextLine {
            runs: vec![GlyphRun {
                font: "Resolved Test".to_owned(),
                rtl: false,
                text: "resolved…".to_owned(),
                glyphs: Vec::new(),
                width: Unit::points(8).unwrap(),
            }],
            width: Unit::points(8).unwrap(),
            height: Unit::points(10).unwrap(),
        }],
        measured: element.bounds.layout.size,
        font_size: Unit::points(9).unwrap(),
        diagnostics: vec![TextDiagnostic::Ellipsized],
    });
    (scene, limits, fonts)
}

fn resolved_table_scene() -> (
    appcore_filemaker::ResolvedScene,
    ResourceLimits,
    FontManager,
) {
    let (mut scene, limits, mut fonts) = scene();
    register_resolved_test_font(&mut fonts);
    let element = &mut scene.pages[0].elements[0];
    element.kind = ElementKind::Table;
    let style = ComputedStyle {
        fill: Some(Color::parse("white").unwrap()),
        stroke: Some(Color::parse("black").unwrap()),
        stroke_width: Unit::points(1).unwrap(),
        opacity: 1_000_000,
        font: None,
        font_size: Unit::points(8).unwrap(),
        color: Color::parse("black").unwrap(),
    };
    let header = table_cell("name", "Name", 3, style.clone());
    let row_cell = table_cell("name", "Alpha", 8, style.clone());
    let total = table_cell("name", "1", 13, style.clone());
    element.table = Some(ResolvedTableFragment {
        index: 0,
        columns: vec![ResolvedTableColumn {
            field: "name".to_owned(),
            header: "Name".to_owned(),
            width: Unit::points(10).unwrap(),
        }],
        header: vec![header],
        rows: vec![ResolvedTableRow {
            source_index: 0,
            bounds: Rect::new(
                Unit::points(2).unwrap(),
                Unit::points(8).unwrap(),
                Unit::points(10).unwrap(),
                Unit::points(5).unwrap(),
            )
            .unwrap(),
            group_start: Some("A".to_owned()),
            style: style.clone(),
            cells: vec![row_cell],
        }],
        totals: vec![total],
        starting_group: Some("A".to_owned()),
    });
    (scene, limits, fonts)
}

fn table_cell(field: &str, text: &str, y: i64, style: ComputedStyle) -> ResolvedTableCell {
    let bounds = Rect::new(
        Unit::points(2).unwrap(),
        Unit::points(y).unwrap(),
        Unit::points(10).unwrap(),
        Unit::points(5).unwrap(),
    )
    .unwrap();
    ResolvedTableCell {
        field: field.to_owned(),
        text: text.to_owned(),
        bounds,
        style,
        text_layout: TextLayout {
            writing_mode: appcore_filemaker::WritingMode::Horizontal,
            lines: vec![TextLine {
                runs: vec![GlyphRun {
                    font: "Resolved Test".to_owned(),
                    rtl: false,
                    text: text.to_owned(),
                    glyphs: Vec::new(),
                    width: Unit::points(4).unwrap(),
                }],
                width: Unit::points(4).unwrap(),
                height: Unit::points(5).unwrap(),
            }],
            measured: bounds.size,
            font_size: Unit::points(4).unwrap(),
            diagnostics: Vec::new(),
        },
    }
}

fn register_resolved_test_font(fonts: &mut FontManager) {
    fonts
        .register(
            FontAsset::new(
                "Resolved Test",
                include_bytes!("../examples/assets/NotoSans-Regular.ttf").to_vec(),
                0,
            )
            .unwrap(),
        )
        .unwrap();
}

fn vertical_text_scene() -> (
    appcore_filemaker::ResolvedScene,
    ResourceLimits,
    FontManager,
) {
    let yaml = r"filemaker: '1.0'
model: canvas
id: vertical-export
page: { width: 80pt, height: 80pt }
elements:
  - id: japanese
    type: text
    text: 日本語の運用レポート
    x: 10pt
    y: 10pt
    width: 60pt
    height: 60pt
    style: { font: Japanese, font_size: 12pt, color: '#112233' }
    text_options: { writing_mode: vertical, overflow: wrap }
"
    .as_bytes();
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let mut fonts = FontManager::default();
    fonts
        .register(
            FontAsset::new(
                "Japanese",
                include_bytes!("assets/NotoSansJP-Test.ttf").to_vec(),
                0,
            )
            .unwrap(),
        )
        .unwrap();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    (scene, limits, fonts)
}

#[test]
fn deterministic_svg_matches_the_visual_snapshot() {
    let (scene, limits, fonts) = scene();
    let bytes = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Svg,
            ..ExportRequest::default()
        },
        &ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        },
    )
    .unwrap()
    .0;
    assert_eq!(
        std::str::from_utf8(&bytes).unwrap(),
        include_str!("snapshots/simple-scene.svg").trim_end()
    );
}

#[test]
fn vertical_text_exports_from_resolved_geometry_without_capability_loss() {
    let (scene, limits, fonts) = vertical_text_scene();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    for format in [ExportFormat::Svg, ExportFormat::Html, ExportFormat::Png] {
        let (bytes, outcome) = export_bytes(
            &scene,
            &ExportRequest {
                format,
                fidelity: Fidelity::Strict,
                dpi: 72,
                ..ExportRequest::default()
            },
            &context,
        )
        .unwrap();
        assert!(outcome.loss_report.losses.is_empty());
        match format {
            ExportFormat::Svg => assert!(String::from_utf8(bytes)
                .unwrap()
                .contains("writing-mode=\"vertical-rl\"")),
            ExportFormat::Html => assert!(String::from_utf8(bytes)
                .unwrap()
                .contains("writing-mode:vertical-rl")),
            ExportFormat::Png => assert!(image::load_from_memory(&bytes)
                .unwrap()
                .to_rgba8()
                .pixels()
                .any(|pixel| pixel.0[3] > 0)),
            _ => unreachable!(),
        }
    }
    for pdf_mode in [PdfMode::Editable, PdfMode::Flattened, PdfMode::Hybrid] {
        let (bytes, outcome) = export_bytes(
            &scene,
            &ExportRequest {
                format: ExportFormat::Pdf,
                pdf_mode,
                fidelity: Fidelity::Strict,
                ..ExportRequest::default()
            },
            &context,
        )
        .unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(outcome.loss_report.losses.is_empty());
    }
}

#[test]
fn svg_and_html_render_resolved_table_structure_and_text() {
    let (scene, limits, fonts) = resolved_table_scene();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let mut svg = Vec::new();
    export(
        &scene,
        &ExportRequest {
            format: ExportFormat::Svg,
            ..ExportRequest::default()
        },
        &context,
        &mut svg,
    )
    .unwrap();
    let svg = String::from_utf8(svg).unwrap();
    assert!(svg.contains("data-table-fragment=\"0\""));
    assert!(svg.contains(">Alpha</tspan>"));

    let mut html = Vec::new();
    export(
        &scene,
        &ExportRequest {
            format: ExportFormat::Html,
            ..ExportRequest::default()
        },
        &context,
        &mut html,
    )
    .unwrap();
    let html = String::from_utf8(html).unwrap();
    assert!(html.contains("<table id=\"box\""));
    assert!(html.contains("<thead><tr>"));
    assert!(html.contains("data-source-row=\"0\" data-group-start=\"A\""));
    assert!(html.contains("<tfoot><tr>"));
}

#[test]
fn pdf_and_raster_render_resolved_table_cell_geometry() {
    let (mut scene, limits, fonts) = resolved_table_scene();
    let table = scene.pages[0].elements[0].table.as_mut().unwrap();
    for cell in table
        .header
        .iter_mut()
        .chain(table.rows.iter_mut().flat_map(|row| &mut row.cells))
        .chain(&mut table.totals)
    {
        cell.text_layout.lines.clear();
    }
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let mut pdf = Vec::new();
    export(
        &scene,
        &ExportRequest {
            format: ExportFormat::Pdf,
            ..ExportRequest::default()
        },
        &context,
        &mut pdf,
    )
    .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));

    let mut png = Vec::new();
    export(
        &scene,
        &ExportRequest {
            format: ExportFormat::Png,
            ..ExportRequest::default()
        },
        &context,
        &mut png,
    )
    .unwrap();
    let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
    assert!(pixels.pixels().any(|pixel| pixel.0[..3] != [255, 255, 255]));
}

#[test]
fn tiled_png_preserves_geometry_across_strip_boundaries() {
    let yaml = br"filemaker: '1.0'
model: canvas
id: tiled-raster
collision: false
page: { width: 100pt, height: 600pt }
elements:
  - id: background
    type: rect
    x: 0pt
    y: 0pt
    width: 100pt
    height: 600pt
    style: { fill: '#336699' }
";
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    let (png, _) = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Png,
            dpi: 72,
            ..ExportRequest::default()
        },
        &ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        },
    )
    .unwrap();
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(decoded.dimensions(), (100, 600));
    for y in [1, 255, 256, 257, 511, 512, 598] {
        assert_eq!(decoded.get_pixel(50, y).0, [51, 102, 153, 255]);
    }
}

#[test]
fn table_preflight_rejects_unavailable_embedded_pdf_font() {
    let (scene, limits, fonts) = resolved_table_scene();
    for pdf_mode in [
        appcore_filemaker::PdfMode::Editable,
        appcore_filemaker::PdfMode::Hybrid,
    ] {
        let error = preflight(
            &scene,
            &ExportRequest {
                format: ExportFormat::Pdf,
                pdf_mode,
                ..ExportRequest::default()
            },
            &ExportContext {
                limits: &limits,
                fonts: &fonts,
                assets: None,
            },
            &PreflightOptions::default(),
            &OperationControl::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), appcore_filemaker::ErrorCode::Validation);
    }
}

#[test]
fn collision_mask_exports_json_svg_png_and_pdf() {
    let (scene, limits, _) = scene();
    let mask = CollisionMask::derive(&scene, 0, MaskView::CollisionMask).unwrap();
    for (format, prefix) in [
        (MaskFormat::Json, b"{".as_slice()),
        (MaskFormat::Svg, b"<svg".as_slice()),
        (MaskFormat::Png, b"\x89PNG".as_slice()),
        (MaskFormat::Pdf, b"%PDF-".as_slice()),
    ] {
        let mut output = Vec::new();
        let written = export_collision_mask(&mask, format, 144, &limits, &mut output).unwrap();
        assert!(output.starts_with(prefix));
        assert_eq!(written, output.len());
    }
}

#[test]
fn collision_mask_json_streams_the_exact_preflighted_document() {
    #[derive(Default)]
    struct WriteProbe {
        bytes: Vec<u8>,
        calls: usize,
        largest: usize,
    }

    impl std::io::Write for WriteProbe {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            self.largest = self.largest.max(bytes.len());
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let (scene, limits, _) = scene();
    let mask = CollisionMask::derive(&scene, 0, MaskView::Combined).unwrap();
    let expected = mask.to_json_bounded(&limits).unwrap();
    let mut probe = WriteProbe::default();
    let written = export_collision_mask(&mask, MaskFormat::Json, 144, &limits, &mut probe).unwrap();
    assert_eq!(probe.bytes, expected);
    assert_eq!(written, expected.len());
    assert!(probe.calls > 1);
    assert!(probe.largest < written);
}

#[test]
fn collision_mask_json_svg_and_pdf_reject_output_limit_before_writing() {
    let (scene, limits, _) = scene();
    let mask = CollisionMask::derive(&scene, 0, MaskView::Combined).unwrap();
    for format in [MaskFormat::Json, MaskFormat::Svg, MaskFormat::Pdf] {
        let mut expected = Vec::new();
        export_collision_mask(&mask, format, 144, &limits, &mut expected).unwrap();
        let exact_limits = ResourceLimits {
            max_output_bytes: expected.len(),
            ..limits.clone()
        };
        let mut exact = Vec::new();
        let written = export_collision_mask(&mask, format, 144, &exact_limits, &mut exact).unwrap();
        assert_eq!(written, expected.len());
        assert_eq!(exact, expected);

        let strict_limits = ResourceLimits {
            max_output_bytes: expected.len() - 1,
            ..limits.clone()
        };
        let mut rejected = Vec::new();
        let error =
            export_collision_mask(&mask, format, 144, &strict_limits, &mut rejected).unwrap_err();
        assert_eq!(error.code(), appcore_filemaker::ErrorCode::LimitExceeded);
        assert!(rejected.is_empty());
    }
}

#[test]
fn collision_mask_pdf_streams_objects_and_page_content() {
    #[derive(Default)]
    struct WriteProbe {
        bytes: Vec<u8>,
        calls: usize,
        largest: usize,
    }

    impl std::io::Write for WriteProbe {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            self.largest = self.largest.max(bytes.len());
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let (scene, limits, _) = scene();
    let mask = CollisionMask::derive(&scene, 0, MaskView::Combined).unwrap();
    let mut probe = WriteProbe::default();
    let written = export_collision_mask(&mask, MaskFormat::Pdf, 144, &limits, &mut probe).unwrap();
    assert_eq!(written, probe.bytes.len());
    assert!(probe.bytes.starts_with(b"%PDF-1.7"));
    assert!(probe.bytes.windows(4).any(|window| window == b"xref"));
    assert!(probe.calls > 10);
    assert!(probe.largest < written);
    assert_debug_pdf_structure(&probe.bytes);
}

fn assert_debug_pdf_structure(pdf: &[u8]) {
    let length_label = find_bytes(pdf, b"/Length ") + b"/Length ".len();
    let length_end = pdf[length_label..]
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap()
        + length_label;
    let content_length = std::str::from_utf8(&pdf[length_label..length_end])
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let content_start = find_bytes(pdf, b"\nstream\n") + b"\nstream\n".len();
    let content_end = find_bytes(&pdf[content_start..], b"\nendstream") + content_start;
    assert_eq!(content_end - content_start, content_length);

    let startxref = find_bytes(pdf, b"startxref\n") + b"startxref\n".len();
    let startxref_end = pdf[startxref..]
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap()
        + startxref;
    let xref_offset = std::str::from_utf8(&pdf[startxref..startxref_end])
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let mut lines = std::str::from_utf8(&pdf[xref_offset..startxref])
        .unwrap()
        .lines();
    assert_eq!(lines.next(), Some("xref"));
    assert_eq!(lines.next(), Some("0 6"));
    assert_eq!(lines.next(), Some("0000000000 65535 f "));
    for reference in 1..=5 {
        let entry = lines.next().unwrap();
        let offset = entry[..10].parse::<usize>().unwrap();
        assert!(pdf[offset..].starts_with(format!("{reference} 0 obj").as_bytes()));
    }
}

fn find_bytes(source: &[u8], needle: &[u8]) -> usize {
    source
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap()
}

#[test]
fn exporters_reject_stale_or_over_budget_public_scenes_before_writing() {
    let (scene, original_limits, fonts) = path_scene();
    let request = ExportRequest {
        format: ExportFormat::Svg,
        ..ExportRequest::default()
    };

    let mut stale = scene.clone();
    stale.engine_version = "stale-engine".to_owned();
    let mut output = Vec::new();
    let error = export(
        &stale,
        &request,
        &ExportContext {
            limits: &original_limits,
            fonts: &fonts,
            assets: None,
        },
        &mut output,
    )
    .unwrap_err();
    assert_eq!(error.code(), appcore_filemaker::ErrorCode::Validation);
    assert!(output.is_empty());

    let limits = ResourceLimits {
        max_path_commands: 1,
        ..original_limits
    };
    let mut output = Vec::new();
    let error = export(
        &scene,
        &request,
        &ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        },
        &mut output,
    )
    .unwrap_err();
    assert_eq!(error.code(), appcore_filemaker::ErrorCode::LimitExceeded);
    assert!(output.is_empty());
}

#[test]
fn formatted_export_stops_at_the_output_budget_before_writing() {
    let (scene, original_limits, fonts) = scene();
    let limits = ResourceLimits {
        max_output_bytes: 32,
        ..original_limits
    };
    for format in [ExportFormat::Svg, ExportFormat::Html, ExportFormat::Pdf] {
        let mut output = Vec::new();
        let error = export(
            &scene,
            &ExportRequest {
                format,
                ..ExportRequest::default()
            },
            &ExportContext {
                limits: &limits,
                fonts: &fonts,
                assets: None,
            },
            &mut output,
        )
        .unwrap_err();
        assert_eq!(error.code(), appcore_filemaker::ErrorCode::LimitExceeded);
        assert!(output.is_empty());
    }
}

#[test]
fn svg_html_and_pdf_write_incrementally_after_bounded_sizing() {
    #[derive(Default)]
    struct WriteProbe {
        bytes: Vec<u8>,
        calls: usize,
        largest: usize,
    }

    impl std::io::Write for WriteProbe {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            self.largest = self.largest.max(bytes.len());
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let (scene, limits, fonts) = path_scene();
    for format in [ExportFormat::Svg, ExportFormat::Html, ExportFormat::Pdf] {
        let mut probe = WriteProbe::default();
        let outcome = export(
            &scene,
            &ExportRequest {
                format,
                ..ExportRequest::default()
            },
            &ExportContext {
                limits: &limits,
                fonts: &fonts,
                assets: None,
            },
            &mut probe,
        )
        .unwrap();
        assert_eq!(outcome.bytes_written, probe.bytes.len());
        assert!(probe.calls > 4);
        assert!(probe.largest < probe.bytes.len());
    }
}

#[test]
fn mask_export_rejects_oversized_public_geometry_before_writing() {
    let (scene, _, _) = scene();
    let mask = CollisionMask::derive(&scene, 0, MaskView::Combined).unwrap();
    let limits = ResourceLimits {
        max_preflight_comparisons: 1,
        ..ResourceLimits::default()
    };
    let mut output = Vec::new();
    let error =
        export_collision_mask(&mask, MaskFormat::Svg, 144, &limits, &mut output).unwrap_err();
    assert_eq!(error.code(), appcore_filemaker::ErrorCode::LimitExceeded);
    assert!(output.is_empty());
}

#[test]
fn svg_png_and_html_export_resolved_geometry() {
    let (scene, limits, fonts) = scene();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    for (format, prefix) in [
        (ExportFormat::Pdf, b"%PDF-".as_slice()),
        (ExportFormat::Svg, b"<svg".as_slice()),
        (ExportFormat::Png, b"\x89PNG".as_slice()),
        (ExportFormat::Html, b"<!doctype html>".as_slice()),
    ] {
        let mut output = Vec::new();
        let outcome = export(
            &scene,
            &ExportRequest {
                format,
                fidelity: Fidelity::Strict,
                ..ExportRequest::default()
            },
            &context,
            &mut output,
        )
        .unwrap();
        assert!(output.starts_with(prefix));
        assert_eq!(outcome.bytes_written, output.len());
        assert!(outcome.loss_report.losses.is_empty());
    }
}

#[test]
fn exporter_options_and_capabilities_are_format_scoped() {
    let (scene, limits, fonts) = scene();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let (svg, _) = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Svg,
            dpi: 0,
            jpeg_quality: 0,
            ..ExportRequest::default()
        },
        &context,
    )
    .unwrap();
    assert!(svg.starts_with(b"<svg"));

    for request in [
        ExportRequest {
            format: ExportFormat::Png,
            dpi: 0,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Jpeg,
            jpeg_quality: 0,
            ..ExportRequest::default()
        },
    ] {
        assert_eq!(
            export_bytes(&scene, &request, &context).unwrap_err().code(),
            appcore_filemaker::ErrorCode::ExportUnsupported
        );
    }

    let (_, semantic) = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Html,
            html_mode: HtmlMode::Semantic,
            ..ExportRequest::default()
        },
        &context,
    )
    .unwrap();
    let (_, fixed) = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Html,
            html_mode: HtmlMode::Fixed,
            ..ExportRequest::default()
        },
        &context,
    )
    .unwrap();
    assert!(semantic
        .capabilities
        .contains(&ExportCapabilities::Semantic));
    assert!(!fixed.capabilities.contains(&ExportCapabilities::Semantic));
}

#[test]
fn png_preserves_alpha_and_jpeg_reports_flattening() {
    let (mut scene, limits, fonts) = scene();
    scene.pages[0].elements[0].style.opacity = 500_000;
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let (png, outcome) = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Png,
            dpi: 72,
            ..ExportRequest::default()
        },
        &context,
    )
    .unwrap();
    assert!(outcome.loss_report.losses.is_empty());
    let pixels = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(pixels.get_pixel(0, 0)[3], 0);
    assert!(pixels.get_pixel(3, 4)[3] < 255);

    let request = ExportRequest {
        format: ExportFormat::Jpeg,
        dpi: 72,
        ..ExportRequest::default()
    };
    assert_eq!(
        export_bytes(&scene, &request, &context).unwrap_err().code(),
        appcore_filemaker::ErrorCode::ExportUnsupported
    );
    let (_, outcome) = export_bytes(
        &scene,
        &ExportRequest {
            fidelity: Fidelity::BestEffort,
            ..request
        },
        &context,
    )
    .unwrap();
    assert_eq!(
        outcome.loss_report.losses[0].kind,
        ExportLossKind::TransparencyFlattened
    );
}

#[test]
fn pdf_emits_deterministic_metadata() {
    let (scene, limits, fonts) = scene();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let (pdf, outcome) = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Pdf,
            ..ExportRequest::default()
        },
        &context,
    )
    .unwrap();
    let source = String::from_utf8_lossy(&pdf);
    assert!(source.contains("/Title (export-test)"));
    assert!(source.contains("/Creator (AppCore FileMaker)"));
    assert!(source.contains("/Producer (appcore-filemaker 0.1.0-alpha.1)"));
    assert_classic_xref(&pdf);
    assert!(outcome.capabilities.contains(&ExportCapabilities::Metadata));
}

fn assert_classic_xref(pdf: &[u8]) {
    let marker = b"startxref\n";
    let marker_offset = pdf
        .windows(marker.len())
        .rposition(|window| window == marker)
        .unwrap();
    let offset_start = marker_offset + marker.len();
    let offset_end = pdf[offset_start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|end| offset_start + end)
        .unwrap();
    let xref_offset = std::str::from_utf8(&pdf[offset_start..offset_end])
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let xref = std::str::from_utf8(&pdf[xref_offset..]).unwrap();
    let mut lines = xref.lines();
    assert_eq!(lines.next(), Some("xref"));
    let header = lines.next().unwrap().split_whitespace().collect::<Vec<_>>();
    assert_eq!(header[0], "0");
    let count = header[1].parse::<usize>().unwrap();
    assert_eq!(lines.next(), Some("0000000000 65535 f "));
    for reference in 1..count {
        let entry = lines.next().unwrap();
        if entry.ends_with(" n ") {
            let offset = entry[..10].parse::<usize>().unwrap();
            assert!(pdf[offset..].starts_with(format!("{reference} 0 obj").as_bytes()));
        }
    }
}

#[test]
fn vector_curves_survive_layout_and_every_graphical_export() {
    let (scene, limits, fonts) = path_scene();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    assert!(matches!(
        &scene.pages[0].elements[0].shape,
        appcore_filemaker::Shape::Path { commands, .. }
            if matches!(commands[1], appcore_filemaker::PathCommand::Curve { .. })
    ));
    for format in [
        ExportFormat::Pdf,
        ExportFormat::Svg,
        ExportFormat::Png,
        ExportFormat::Html,
    ] {
        let mut output = Vec::new();
        export(
            &scene,
            &ExportRequest {
                format,
                fidelity: Fidelity::Strict,
                ..ExportRequest::default()
            },
            &context,
            &mut output,
        )
        .unwrap();
        if format == ExportFormat::Svg {
            assert!(String::from_utf8(output)
                .unwrap()
                .contains("C 7.000000 3.000000 17.000000 3.000000 22.000000 13.000000"));
        } else if format == ExportFormat::Html {
            assert!(String::from_utf8(output)
                .unwrap()
                .contains("C 5.000000 0.000000 15.000000 0.000000 20.000000 10.000000"));
        }
    }
}

#[test]
fn transforms_are_preserved_by_every_graphical_exporter() {
    let (scene, limits, fonts) = transformed_scene();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    for format in [
        ExportFormat::Pdf,
        ExportFormat::Svg,
        ExportFormat::Png,
        ExportFormat::Html,
    ] {
        let mut output = Vec::new();
        let outcome = export(
            &scene,
            &ExportRequest {
                format,
                fidelity: Fidelity::Strict,
                dpi: 72,
                ..ExportRequest::default()
            },
            &context,
            &mut output,
        )
        .unwrap();
        assert!(outcome.loss_report.losses.is_empty());
        match format {
            ExportFormat::Svg => assert!(String::from_utf8(output)
                .unwrap()
                .contains("matrix(0.000000 1.000000 -1.000000 0.000000 14.000000 0.000000)")),
            ExportFormat::Html => assert!(String::from_utf8(output).unwrap().contains(
                "transform:matrix(0.000000,1.000000,-1.000000,0.000000,13.333333,0.000000)"
            )),
            ExportFormat::Png => {
                let image = image::load_from_memory(&output).unwrap().to_rgba8();
                assert_eq!(image.get_pixel(3, 3).0, [0, 0, 0, 0]);
                assert_eq!(image.get_pixel(10, 4).0, [0, 0, 0, 255]);
            }
            ExportFormat::Pdf => {
                let pdf = String::from_utf8_lossy(&output);
                assert!(pdf.contains("0 -1 1 0 -6 20 cm"));
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn svg_and_html_render_resolved_text_and_clip_bounds() {
    let (scene, limits, fonts) = resolved_text_scene();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    for format in [ExportFormat::Svg, ExportFormat::Html] {
        let mut output = Vec::new();
        export(
            &scene,
            &ExportRequest {
                format,
                fidelity: Fidelity::Strict,
                ..ExportRequest::default()
            },
            &context,
            &mut output,
        )
        .unwrap();
        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains("resolved…"));
        assert!(!rendered.contains("unresolved original"));
        match format {
            ExportFormat::Svg => assert!(rendered.contains("clipPath")),
            ExportFormat::Html => assert!(rendered.contains("overflow:hidden")),
            _ => unreachable!(),
        }
    }
}

#[test]
fn prepared_text_capabilities_are_explicit_export_losses() {
    let (mut scene, limits, fonts) = resolved_text_scene();
    scene.pages[0].elements[0]
        .text_layout
        .as_mut()
        .unwrap()
        .diagnostics
        .push(TextDiagnostic::VerticalWritingUnavailable);
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let outcome = export(
        &scene,
        &ExportRequest {
            format: ExportFormat::Svg,
            fidelity: Fidelity::BestEffort,
            ..ExportRequest::default()
        },
        &context,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(
        outcome.loss_report.losses[0].kind,
        ExportLossKind::TextCapabilityUnsupported
    );
    let error = export(
        &scene,
        &ExportRequest {
            format: ExportFormat::Svg,
            fidelity: Fidelity::Strict,
            ..ExportRequest::default()
        },
        &context,
        &mut Vec::new(),
    )
    .unwrap_err();
    assert_eq!(
        error.code(),
        appcore_filemaker::ErrorCode::ExportUnsupported
    );
    let report = preflight(
        &scene,
        &ExportRequest {
            format: ExportFormat::Jpeg,
            dpi: 72,
            ..ExportRequest::default()
        },
        &context,
        &PreflightOptions::default(),
        &OperationControl::default(),
    )
    .unwrap();
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == appcore_filemaker::ValidationCode::Capability));
}

#[test]
fn image_cover_and_focal_geometry_is_shared_by_exporters() {
    let yaml = br"filemaker: '1.0'
model: canvas
id: image-test
page: { width: 60pt, height: 60pt }
elements:
  - id: image
    type: image
    asset: sample.png
    x: 5pt
    y: 5pt
    width: 50pt
    height: 50pt
    image: { fit: cover, focal_x: 1000000 }
";
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        200,
        100,
        image::Rgba([20, 40, 60, 255]),
    ))
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let mut assets = MemoryResolver::default();
    assets
        .insert("sample.png", "image/png", png.into_inner())
        .unwrap();
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .with_assets(&assets)
        .resolve(&document)
        .unwrap();
    let placement = scene.pages[0].elements[0].image_placement.unwrap();
    assert_eq!(placement.source.x, 100);
    assert_eq!(placement.source.width, 100);
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: Some(&assets),
    };
    let dpi_report = preflight(
        &scene,
        &ExportRequest::default(),
        &context,
        &PreflightOptions {
            minimum_image_dpi: 600,
            ..PreflightOptions::default()
        },
        &OperationControl::default(),
    )
    .unwrap();
    assert!(dpi_report
        .issues
        .iter()
        .any(|issue| issue.code == appcore_filemaker::ValidationCode::Dpi));
    for format in [
        ExportFormat::Pdf,
        ExportFormat::Svg,
        ExportFormat::Png,
        ExportFormat::Html,
    ] {
        let outcome = export(
            &scene,
            &ExportRequest {
                format,
                fidelity: Fidelity::Strict,
                ..ExportRequest::default()
            },
            &context,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(outcome.loss_report.losses.is_empty());
    }
}

#[test]
fn jpeg_reports_alpha_loss_from_raster_assets() {
    let yaml = br"filemaker: '1.0'
model: canvas
id: alpha-image
page: { width: 4pt, height: 4pt }
elements:
  - { id: image, type: image, asset: alpha.png, x: 1pt, y: 1pt, width: 2pt, height: 2pt }
";
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        2,
        image::Rgba([20, 40, 60, 128]),
    ))
    .write_to(&mut encoded, image::ImageFormat::Png)
    .unwrap();
    let mut assets = MemoryResolver::default();
    assets
        .insert("alpha.png", "image/png", encoded.into_inner())
        .unwrap();
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .with_assets(&assets)
        .resolve(&document)
        .unwrap();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: Some(&assets),
    };
    let (png, outcome) = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Png,
            dpi: 72,
            ..ExportRequest::default()
        },
        &context,
    )
    .unwrap();
    assert!(outcome.loss_report.losses.is_empty());
    assert!(
        image::load_from_memory(&png)
            .unwrap()
            .to_rgba8()
            .get_pixel(1, 1)[3]
            < 255
    );

    let error = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Jpeg,
            dpi: 72,
            ..ExportRequest::default()
        },
        &context,
    )
    .unwrap_err();
    assert_eq!(
        error.code(),
        appcore_filemaker::ErrorCode::ExportUnsupported
    );
    let report = preflight(
        &scene,
        &ExportRequest {
            format: ExportFormat::Jpeg,
            dpi: 72,
            ..ExportRequest::default()
        },
        &context,
        &PreflightOptions::default(),
        &OperationControl::default(),
    )
    .unwrap();
    assert!(report.issues.iter().any(|issue| {
        issue.code == appcore_filemaker::ValidationCode::Capability
            && issue.message.contains("image alpha")
    }));
}

#[test]
fn svg_assets_embed_in_vector_outputs_and_report_rasterization_loss() {
    let yaml = br"filemaker: '1.0'
model: canvas
id: svg-image
page: { width: 60pt, height: 60pt }
elements:
  - { id: image, type: image, asset: sample.svg, x: 5pt, y: 5pt, width: 50pt, height: 50pt }
";
    let mut assets = MemoryResolver::default();
    assets
        .insert(
            "sample.svg",
            "image/svg+xml",
            br#"<svg viewBox="0 0 200 100"><rect width="200" height="100"/></svg>"#.to_vec(),
        )
        .unwrap();
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .with_assets(&assets)
        .resolve(&document)
        .unwrap();
    assert!(scene.pages[0].elements[0].image_placement.unwrap().vector);
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: Some(&assets),
    };
    for format in [ExportFormat::Svg, ExportFormat::Html] {
        let outcome = export(
            &scene,
            &ExportRequest {
                format,
                ..ExportRequest::default()
            },
            &context,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(outcome.loss_report.losses.is_empty());
    }
    for format in [ExportFormat::Pdf, ExportFormat::Png] {
        let report = preflight(
            &scene,
            &ExportRequest {
                format,
                ..ExportRequest::default()
            },
            &context,
            &PreflightOptions::default(),
            &OperationControl::default(),
        )
        .unwrap();
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == appcore_filemaker::ValidationCode::Capability));
        let strict = export(
            &scene,
            &ExportRequest {
                format,
                ..ExportRequest::default()
            },
            &context,
            &mut Vec::new(),
        )
        .unwrap_err();
        assert_eq!(
            strict.code(),
            appcore_filemaker::ErrorCode::ExportUnsupported
        );
        let outcome = export(
            &scene,
            &ExportRequest {
                format,
                fidelity: Fidelity::BestEffort,
                ..ExportRequest::default()
            },
            &context,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            outcome.loss_report.losses[0].kind,
            ExportLossKind::UnsupportedElement
        );
    }
}

#[test]
fn pdf_hybrid_combines_outlines_with_searchable_text() {
    let (scene, limits, fonts) = hybrid_text_scene();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let mut output = Vec::new();
    let outcome = export(
        &scene,
        &ExportRequest {
            format: ExportFormat::Pdf,
            pdf_mode: appcore_filemaker::PdfMode::Hybrid,
            ..ExportRequest::default()
        },
        &context,
        &mut output,
    )
    .unwrap();

    assert!(outcome
        .capabilities
        .contains(&ExportCapabilities::EditableText));
    assert!(outcome
        .capabilities
        .contains(&ExportCapabilities::EmbeddedFonts));
    assert!(output.windows(4).any(|window| window == b"3 Tr"));
    assert!(output
        .windows(b"/ToUnicode".len())
        .any(|window| window == b"/ToUnicode"));
}

#[test]
fn export_style_override_is_paint_only_validated_and_reports_color_loss() {
    let (scene, limits, fonts) = scene();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let cmyk = ExportStyleOverride {
        fill: Some(Color::Cmyk {
            c: 1_000_000,
            m: 0,
            y: 0,
            k: 0,
        }),
        ..ExportStyleOverride::default()
    };
    let strict = export(
        &scene,
        &ExportRequest {
            format: ExportFormat::Svg,
            style_override: Some(cmyk),
            ..ExportRequest::default()
        },
        &context,
        &mut Vec::new(),
    )
    .unwrap_err();
    assert_eq!(
        strict.code(),
        appcore_filemaker::ErrorCode::ExportUnsupported
    );

    let outcome = export(
        &scene,
        &ExportRequest {
            format: ExportFormat::Svg,
            fidelity: Fidelity::BestEffort,
            style_override: Some(cmyk),
            ..ExportRequest::default()
        },
        &context,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(
        outcome.loss_report.losses[0].kind,
        ExportLossKind::CmykConvertedToRgb
    );
    let report = preflight(
        &scene,
        &ExportRequest {
            format: ExportFormat::Svg,
            style_override: Some(cmyk),
            ..ExportRequest::default()
        },
        &context,
        &PreflightOptions::default(),
        &OperationControl::default(),
    )
    .unwrap();
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == appcore_filemaker::ValidationCode::Capability));

    let invalid_request = ExportRequest {
        style_override: Some(ExportStyleOverride {
            opacity: Some(1_000_001),
            ..ExportStyleOverride::default()
        }),
        ..ExportRequest::default()
    };
    let invalid = export(&scene, &invalid_request, &context, &mut Vec::new()).unwrap_err();
    assert_eq!(
        invalid.code(),
        appcore_filemaker::ErrorCode::ExportUnsupported
    );
    let preflight_error = preflight(
        &scene,
        &invalid_request,
        &context,
        &PreflightOptions::default(),
        &OperationControl::default(),
    )
    .unwrap_err();
    assert_eq!(
        preflight_error.code(),
        appcore_filemaker::ErrorCode::ExportUnsupported
    );
}

#[test]
fn csv_streams_column_order_and_escaping() {
    let limits = ResourceLimits::default();
    let spec = TableSpec {
        columns: vec![TableColumn {
            field: "name".to_owned(),
            header: "Name".to_owned(),
            width: ColumnWidth::Flex(1),
        }],
        repeat_header: true,
        group_by: None,
        total_fields: Vec::new(),
        conditional_styles: Vec::new(),
        style_expression_steps: 64,
        auto_sample_rows: 16,
        max_rows: 10,
        max_row_fields: 16,
        max_cell_bytes: 1_024,
    };
    let dataset = InMemoryDataset {
        rows: vec![BTreeMap::from([(
            "name".to_owned(),
            DataValue::String("Ada, \"A\"".to_owned()),
        )])],
    };
    let mut output = Vec::new();
    export_dataset_csv(&spec, &dataset, &limits, &mut output).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "Name\r\n\"Ada, \"\"A\"\"\"\r\n"
    );
    let (bytes, outcome) = export_dataset_csv_bytes(&spec, &dataset, &limits).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "Name\r\n\"Ada, \"\"A\"\"\"\r\n"
    );
    assert!(outcome.loss_report.losses.is_empty());
}
