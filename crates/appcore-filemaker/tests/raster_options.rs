// =============================================================================
//        #######
//     ###       ###     F: raster_options.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 12:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use appcore_filemaker::*;

fn scene() -> ResolvedScene {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(br"filemaker: '1.0'
model: canvas
id: strip-options
page: { width: 16pt, height: 300pt }
elements:
  - { id: background, type: rect, width: 16pt, height: 300pt, style: { fill: '#ff0000' } }
  - { id: band, type: rect, y: 253pt, width: 16pt, height: 10pt, collision: false, style: { fill: '#0000ff' } }
").unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(Default::default()), &[])
        .unwrap();
    LayoutEngine::new(
        &ResourceLimits::default(),
        &FontManager::default(),
        LayoutOptions::default(),
    )
    .unwrap()
    .resolve(&document)
    .unwrap()
}

#[test]
fn custom_strip_limits_preserve_decoded_png_and_jpeg_pixels() {
    let scene = scene();
    let before = scene.clone();
    let limits = ResourceLimits::default();
    let fonts = FontManager::default();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    for format in [ExportFormat::Png, ExportFormat::Jpeg] {
        let request = ExportRequest {
            format,
            dpi: 72,
            fidelity: Fidelity::BestEffort,
            ..ExportRequest::default()
        };
        let (expected, outcome) = export_bytes(&scene, &request, &context).unwrap();
        let expected_pixels = ::image::load_from_memory(&expected).unwrap().to_rgba8();
        for (bytes, rows) in [(64, 1), (448, 7), (1024, 16), (65536, 4096)] {
            let mut output = Vec::new();
            let actual = export_raster_controlled(
                &scene,
                &request,
                &context,
                RasterOptions::new(bytes, rows).unwrap(),
                &OperationControl::default(),
                &mut output,
            )
            .unwrap();
            assert_eq!(actual.loss_report, outcome.loss_report);
            assert_eq!(actual.capabilities, outcome.capabilities);
            assert_eq!(actual.bytes_written, output.len());
            assert_eq!(
                ::image::load_from_memory(&output).unwrap().to_rgba8(),
                expected_pixels
            );
        }
    }
    assert_eq!(scene, before);
}

#[test]
fn invalid_strip_limits_and_cancelled_exports_never_write() {
    for (bytes, rows) in [(0, 1), (1, 0), (64 * 1024 * 1024 + 1, 1), (1, 4097)] {
        assert_eq!(
            RasterOptions::new(bytes, rows).unwrap_err().code(),
            ErrorCode::LimitExceeded
        );
    }
    let scene = scene();
    let limits = ResourceLimits::default();
    let fonts = FontManager::default();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let mut output = Vec::new();
    let mut request = ExportRequest {
        format: ExportFormat::Png,
        dpi: 72,
        ..ExportRequest::default()
    };
    let control = OperationControl::default();
    let error = export_raster_controlled(
        &scene,
        &request,
        &context,
        RasterOptions::new(63, 1).unwrap(),
        &control,
        &mut output,
    )
    .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LimitExceeded);
    assert!(output.is_empty());
    control.cancellation().cancel();
    let error = export_raster_controlled(
        &scene,
        &request,
        &context,
        RasterOptions::default(),
        &control,
        &mut output,
    )
    .unwrap_err();
    assert_eq!(error.code(), ErrorCode::Cancelled);
    request.format = ExportFormat::Svg;
    let error = export_raster_controlled(
        &scene,
        &request,
        &context,
        RasterOptions::default(),
        &OperationControl::default(),
        &mut output,
    )
    .unwrap_err();
    assert_eq!(error.code(), ErrorCode::ExportUnsupported);
    assert!(output.is_empty());
}
