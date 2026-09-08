// =============================================================================
//        #######
//     ###       ###     F: raster_strips.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 12:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Compares strip heights on the same resolved FHD vector scene.

use std::fmt::Write;
use std::hint::black_box;

use appcore_filemaker::{
    export_raster_controlled, Compiler, DataValue, ExportContext, ExportFormat, ExportRequest,
    Fidelity, FontManager, LayoutEngine, LayoutOptions, OperationControl, RasterOptions,
    ResolvedScene, ResourceLimits,
};

pub(super) const CASES: [&str; 8] = [
    "raster_png_rows_8",
    "raster_png_rows_64",
    "raster_png_rows_256",
    "raster_jpeg_rows_8",
    "raster_jpeg_rows_64",
    "raster_jpeg_rows_256",
    "raster_png_dense_rows_8",
    "raster_png_dense_rows_256",
];

pub(super) fn run(selected: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    if selected.is_some_and(|name| !CASES.contains(&name)) {
        return Ok(());
    }
    let limits = ResourceLimits::default();
    let fonts = FontManager::default();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let control = OperationControl::default();
    for (index, case) in CASES.into_iter().enumerate() {
        if selected.is_some_and(|name| name != case) {
            continue;
        }
        let scene = scene(&limits, &fonts, index >= 6)?;
        let request = ExportRequest {
            format: if !(3..6).contains(&index) {
                ExportFormat::Png
            } else {
                ExportFormat::Jpeg
            },
            dpi: 96,
            fidelity: Fidelity::BestEffort,
            ..ExportRequest::default()
        };
        let rows = if index >= 6 {
            [8, 256][index - 6]
        } else {
            [8, 64, 256][index % 3]
        };
        let options = RasterOptions::new(4 * 1024 * 1024, rows)?;
        super::measure(case, 3, || {
            let outcome = export_raster_controlled(
                &scene,
                &request,
                &context,
                options,
                &control,
                &mut std::io::sink(),
            )?;
            assert!(outcome.bytes_written > 0);
            black_box(outcome);
            Ok(())
        })?;
    }
    Ok(())
}

pub(super) fn scene(
    limits: &ResourceLimits,
    fonts: &FontManager,
    dense: bool,
) -> Result<ResolvedScene, Box<dyn std::error::Error>> {
    // Dense geometry disables collision to keep fixture setup within its comparison budget.
    let mut yaml = String::from(
        "filemaker: '1.0'\nmodel: canvas\nid: raster-fhd\npage: { width: 1920px, height: 1080px }\nelements:\n  - { id: background, type: rect, width: 1920px, height: 1080px, collision: false, style: { fill: '#ffffff' } }\n",
    );
    if dense {
        yaml.insert_str(0, "collision: false\n");
    }
    let (columns, count, dx, dy, width, height) = if dense {
        (64, 4096, 30, 16, 20, 10)
    } else {
        (16, 256, 110, 60, 80, 40)
    };
    for index in 0..count {
        writeln!(yaml,
            "  - {{ id: box-{index:03}, type: rect, x: {}px, y: {}px, width: {width}px, height: {height}px, style: {{ fill: '#204080' }} }}",
            (index % columns) * dx, (index / columns) * dy)?;
    }
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(yaml.as_bytes())?;
    let document = compiler.bind(&template, &DataValue::Object(Default::default()), &[])?;
    let scene = LayoutEngine::new(limits, fonts, LayoutOptions::default())?.resolve(&document)?;
    assert_eq!(scene.pages.len(), 1);
    assert_eq!(scene.pages[0].elements.len(), count + 1);
    Ok(scene)
}
