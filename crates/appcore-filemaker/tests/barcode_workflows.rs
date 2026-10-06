// =============================================================================
//        #######
//     ###       ###     F: barcode_workflows.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/05 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Native barcode and large-document export regression workflows.
use appcore_filemaker::*;
use std::collections::BTreeMap;

fn resolve(yaml: &str, data: DataValue) -> (ResolvedScene, ResourceLimits, FontManager) {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
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
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    (scene, limits, fonts)
}

const BARCODE: &str = "filemaker: '1.0'\nmodel: canvas\nid: barcode\npage: { width: 260pt, height: 100pt }\nelements:\n  - id: code\n    type: barcode\n    x: 10pt\n    y: 10pt\n    width: 226pt\n    height: 60pt\n    binding: data.code\n    style: { fill: black }\n";

#[test]
fn bound_ean13_lowers_once_and_exports_to_all_visual_formats() {
    let (scene, limits, fonts) = resolve(
        BARCODE,
        DataValue::Object(BTreeMap::from([(
            "code".into(),
            DataValue::String("4006381333931".into()),
        )])),
    );
    assert_eq!(scene.pages[0].elements[0].kind, ElementKind::Path);
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    for format in [
        ExportFormat::Pdf,
        ExportFormat::Svg,
        ExportFormat::Png,
        ExportFormat::Jpeg,
        ExportFormat::Html,
    ] {
        let mut output = Vec::new();
        let outcome = export(
            &scene,
            &ExportRequest {
                format,
                dpi: 72,
                ..ExportRequest::default()
            },
            &context,
            &mut output,
        )
        .unwrap();
        assert!(outcome.loss_report.losses.is_empty(), "{format:?}");
        assert!(!output.is_empty());
        if format == ExportFormat::Png {
            let image = ::image::load_from_memory(&output).unwrap().to_rgba8();
            // Scan the actual emitted raster through all 95 symbol modules.
            let modules = appcore_filemaker::barcode::Ean13::parse("4006381333931").unwrap();
            for (index, bar) in modules.modules().iter().enumerate() {
                let pixel = image.get_pixel(10 + (11 + index as u32) * 2 + 1, 30);
                assert_eq!(pixel[3] > 128 && pixel[0] < 128, *bar, "module {index}");
            }
        }
        if std::env::var_os("APPCORE_FILEMAKER_WORKFLOW_ARTIFACTS").is_some() {
            let dir = std::path::PathBuf::from(
                std::env::var_os("APPCORE_FILEMAKER_WORKFLOW_ARTIFACTS").unwrap(),
            );
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("ean13.{format:?}").to_lowercase()), output).unwrap();
        }
    }
}

#[test]
fn invalid_bound_code_fails_before_any_export() {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(BARCODE.as_bytes()).unwrap();
    for code in ["4006381333932", "400638133393", "not-a-code"] {
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "code".into(),
                    DataValue::String(code.into()),
                )])),
                &[],
            )
            .unwrap();
        let limits = ResourceLimits::default();
        let fonts = FontManager::default();
        assert!(LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .is_err());
    }
}

#[test]
fn long_text_and_240_items_keep_content_in_pdf_svg_and_fixed_web_preview() {
    let text = (0..80)
        .map(|i| format!("Description line {i:03}"))
        .collect::<Vec<_>>()
        .join("\n");
    let rows = (0..240)
        .map(|i| {
            DataValue::Object(BTreeMap::from([(
                "name".into(),
                DataValue::String(format!("Item {i:03}")),
            )]))
        })
        .collect();
    let yaml = "filemaker: '1.0'\nmodel: document\nid: bounded-report\ncollision: false\npage: { width: 240pt, height: 240pt }\nelements:\n  - id: flow\n    type: group\n    x: 10pt\n    y: 10pt\n    width: 220pt\n    height: 220pt\n    layout: flow_vertical\n    collision: false\n    children:\n      - id: description\n        type: text\n        width: 220pt\n        height: auto\n        binding: data.description\n        style: { font: Body, font_size: 8pt }\n        text_options: { overflow: expand }\n        collision: false\n      - id: rows\n        type: table\n        width: 220pt\n        height: 220pt\n        binding: data.rows\n        style: { font: Body, font_size: 8pt }\n        collision: false\n        table:\n          columns: [{ field: name, header: Name, width: { mode: flex, value: 1 } }]\n          max_rows: 240\n          max_row_fields: 1\n          max_cell_bytes: 64\n          row_height: auto\n          repeat_header: true\n";
    let (scene, limits, fonts) = resolve(
        yaml,
        DataValue::Object(BTreeMap::from([
            ("description".into(), DataValue::String(text.clone())),
            ("rows".into(), DataValue::Array(rows)),
        ])),
    );
    assert!(scene.pages.len() > 2);
    assert!(
        scene.pages.iter().all(|page| page
            .elements
            .iter()
            .any(|element| element.kind != ElementKind::Group)),
        "no empty continuation pages"
    );
    let fragments = scene
        .pages
        .iter()
        .flat_map(|p| &p.elements)
        .filter(|e| e.id.as_str() == "description");
    assert_eq!(
        fragments
            .map(|e| e.text.as_deref().unwrap())
            .collect::<Vec<_>>()
            .join("\n"),
        text
    );
    let count: usize = scene
        .pages
        .iter()
        .flat_map(|p| &p.elements)
        .filter_map(|e| e.table.as_ref())
        .map(|table| table.rows.len())
        .sum();
    assert_eq!(count, 240);
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    for format in [ExportFormat::Pdf, ExportFormat::Svg, ExportFormat::Html] {
        let mut output = Vec::new();
        let request = ExportRequest {
            format,
            html_mode: HtmlMode::Fixed,
            ..ExportRequest::default()
        };
        export(&scene, &request, &context, &mut output).unwrap();
        if format == ExportFormat::Html {
            let html = String::from_utf8(output.clone()).unwrap();
            assert!(html.contains("Item 000"));
            assert!(html.contains("Item 239"));
            assert!(html.contains("Description line 000"));
            assert!(html.contains("Description line 079"));
        }
        if let Some(dir) = std::env::var_os("APPCORE_FILEMAKER_WORKFLOW_ARTIFACTS") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                std::path::PathBuf::from(dir)
                    .join(format!("large-report.{format:?}").to_lowercase()),
                output,
            )
            .unwrap();
        }
    }
}

#[test]
fn bound_ean8_checks_checksum_and_actual_raster_modules() {
    let yaml = BARCODE.replace("226pt", "162pt");
    let (scene, limits, fonts) = resolve(
        &yaml,
        DataValue::Object(BTreeMap::from([(
            "code".into(),
            DataValue::String("96385074".into()),
        )])),
    );
    let expected = "1010001011010111101111010110111010101001110111001010001001011100101";
    let symbol = appcore_filemaker::barcode::Ean8::parse("96385074").unwrap();
    assert_eq!(
        symbol
            .modules()
            .iter()
            .map(|v| if *v { '1' } else { '0' })
            .collect::<String>(),
        expected
    );
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let (bytes, _) = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Png,
            dpi: 72,
            ..ExportRequest::default()
        },
        &context,
    )
    .unwrap();
    let image = ::image::load_from_memory(&bytes).unwrap().to_rgba8();
    for (index, bit) in expected.bytes().enumerate() {
        let pixel = image.get_pixel(10 + (7 + index as u32) * 2 + 1, 30);
        assert_eq!(
            pixel[3] > 128 && pixel[0] < 128,
            bit == b'1',
            "EAN-8 module {index}"
        );
    }
    for x in [10, 22, 158, 170] {
        assert_eq!(image.get_pixel(x, 30)[3], 0);
    }
    assert!(appcore_filemaker::barcode::Ean8::parse("96385075").is_err());
    assert_eq!(
        appcore_filemaker::barcode::Ean8::parse("00000000")
            .unwrap()
            .digits(),
        &[0; 8]
    );
    assert!(appcore_filemaker::barcode::ean_shape(
        "12345678901234",
        scene.pages[0].elements[0].bounds.layout
    )
    .is_err());
}

#[test]
fn qr_exports_and_independent_decoder_recovers_exact_utf8_payload() {
    let yaml = BARCODE
        .replace("type: barcode", "type: qr")
        .replace("height: 100pt", "height: 240pt")
        .replace("height: 60pt", "height: 200pt");
    for payload in [
        "https://example.org/local?id=42".to_owned(),
        "Olá, café! 日本語 🦀".to_owned(),
        "x".repeat(1024),
    ] {
        let (scene, limits, fonts) = resolve(
            &yaml,
            DataValue::Object(BTreeMap::from([(
                "code".into(),
                DataValue::String(payload.clone()),
            )])),
        );
        let context = ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        };
        assert_eq!(scene.pages[0].elements[0].kind, ElementKind::Path);
        for format in [
            ExportFormat::Pdf,
            ExportFormat::Svg,
            ExportFormat::Png,
            ExportFormat::Jpeg,
            ExportFormat::Html,
        ] {
            let (bytes, outcome) = export_bytes(
                &scene,
                &ExportRequest {
                    format,
                    dpi: 144,
                    ..ExportRequest::default()
                },
                &context,
            )
            .unwrap();
            assert!(outcome.loss_report.losses.is_empty());
            if matches!(format, ExportFormat::Png | ExportFormat::Jpeg) {
                let rgba = ::image::load_from_memory(&bytes).unwrap().to_rgba8();
                let gray: Vec<u8> = rgba
                    .pixels()
                    .map(|p| if p[3] < 128 { 255 } else { p[0] })
                    .collect();
                let mut decoder = quircs::Quirc::default();
                let decoded: Vec<_> = decoder
                    .identify(rgba.width() as usize, rgba.height() as usize, &gray)
                    .map(|code| code.unwrap().decode().unwrap())
                    .collect();
                assert_eq!(decoded.len(), 1);
                assert_eq!(decoded[0].payload, payload.as_bytes(), "{format:?}");
            }
        }
    }
}

#[test]
fn qr_limits_geometry_and_schema_fail_closed() {
    use appcore_filemaker::qrcode::{QrCode, MAX_QR_PAYLOAD_BYTES};
    assert!(QrCode::encode("").is_err());
    assert!(QrCode::encode(&"x".repeat(MAX_QR_PAYLOAD_BYTES + 1)).is_err());
    let symbol = QrCode::encode("payload").unwrap();
    assert!(symbol.module(symbol.width(), 0).is_none());
    assert!(!format!("{symbol:?}").contains("payload"));
    let tiny = Rect::new(Unit::ZERO, Unit::ZERO, Unit::from_raw(1), Unit::from_raw(1)).unwrap();
    assert!(symbol.shape(tiny, usize::MAX).is_err());
    let rect = Rect::new(
        Unit::ZERO,
        Unit::ZERO,
        Unit::points(100).unwrap(),
        Unit::points(100).unwrap(),
    )
    .unwrap();
    assert!(symbol.shape(rect, 1).is_err());
    let limits = ResourceLimits {
        max_path_commands: 1,
        ..ResourceLimits::default()
    };
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let yaml = BARCODE.replace("type: barcode", "type: qr");
    let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
    let document = compiler
        .bind(
            &template,
            &DataValue::Object(BTreeMap::from([(
                "code".into(),
                DataValue::String("abc".into()),
            )])),
            &[],
        )
        .unwrap();
    assert!(
        LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .is_err()
    );
    assert!(compiler
        .compile_template_yaml(
            yaml.replace("type: qr", "type: qr\n    correction: mystery")
                .as_bytes()
        )
        .is_err());
}

#[test]
fn repeated_qr_symbols_share_one_scene_path_budget() {
    let yaml = "filemaker: '1.0'\nmodel: canvas\nid: repeated-qr\ncollision: false\npage: { width: 240pt, height: 120pt }\nelements:\n  - id: one\n    type: qr\n    x: 0pt\n    width: 100pt\n    height: 100pt\n    text: abc\n  - id: two\n    type: qr\n    x: 120pt\n    width: 100pt\n    height: 100pt\n    text: abc\n";
    let (scene, _, _) = resolve(yaml, DataValue::Object(BTreeMap::new()));
    let commands: usize = scene.pages[0]
        .elements
        .iter()
        .map(|element| match &element.shape {
            Shape::Path { commands, .. } => commands.len(),
            _ => panic!("QR must resolve to paths"),
        })
        .sum();
    let limits = ResourceLimits {
        max_path_commands: commands - 1,
        ..ResourceLimits::default()
    };
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    assert!(
        LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .is_err()
    );
}
