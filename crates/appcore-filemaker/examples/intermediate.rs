// =============================================================================
//        #######
//     ###       ###     F: intermediate.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 09:24:58 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Demonstrates the bounded intermediate workflow for this crate.

use std::path::PathBuf;
use std::sync::Arc;

use appcore_filemaker::{
    export_bytes, preflight, Compiler, DataValue, DocumentFingerprint, ElementId, ExportContext,
    ExportFormat, ExportOutcome, ExportRequest, FontAsset, FontManager, HtmlMode, LayoutEngine,
    LayoutOptions, OperationControl, OperationLog, Patch, PatchOperation, PdfMode,
    PreflightOptions, ResolvedScene, ResourceLimits, SceneCache, SceneInspector,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(include_bytes!("intermediate.yml"))?;
    let data: DataValue = serde_json::from_slice(include_bytes!("intermediate-data.json"))?;
    let mut document = compiler.bind(&template, &data, &[])?;
    let mut log = OperationLog::new_bounded(8, 8 * 1024 * 1024)?;
    let patch = Patch {
        sequence: 1,
        operations: vec![PatchOperation::Move {
            id: ElementId::new("review-marker")?,
            x: "185mm".parse()?,
            y: "112mm".parse()?,
        }],
    };
    log.apply(&mut document, &patch, limits.max_patch_operations)?;
    let fonts = example_fonts()?;
    let fingerprint = DocumentFingerprint::compute_with_patches(
        &template,
        &data,
        std::slice::from_ref(&patch),
        None,
        &fonts,
        &limits,
    )?;
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())?;
    let mut cache = SceneCache::with_byte_capacity(8, 64 * 1024 * 1024)?;
    let scene = engine.resolve_cached(&document, fingerprint, &mut cache)?;
    let cached = engine.resolve_cached(&document, fingerprint, &mut cache)?;
    if !Arc::ptr_eq(&scene, &cached) || scene.pages.len() != 2 {
        return Err(std::io::Error::other("expected one cached two-page scene").into());
    }

    let pdf_request = ExportRequest {
        format: ExportFormat::Pdf,
        pdf_mode: PdfMode::Editable,
        ..ExportRequest::default()
    };
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let report = preflight(
        &scene,
        &pdf_request,
        &context,
        &PreflightOptions {
            strict: true,
            ..PreflightOptions::default()
        },
        &OperationControl::default(),
    )?;
    report.enforce(true)?;
    let (pdf, pdf_outcome) = export_bytes(&scene, &pdf_request, &context)?;
    let (html, html_outcome) = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Html,
            html_mode: HtmlMode::Fixed,
            ..ExportRequest::default()
        },
        &context,
    )?;
    let (page_one_svg, page_one_outcome) = export_svg_page(&scene, 0, &context)?;
    let (page_two_svg, page_two_outcome) = export_svg_page(&scene, 1, &context)?;

    let pdf_path = output_path("intermediate.pdf")?;
    let html_path = output_path("intermediate.html")?;
    let page_one_path = output_path("intermediate-page-1.svg")?;
    let page_two_path = output_path("intermediate-page-2.svg")?;
    let report_path = output_path("intermediate-preflight.json")?;
    std::fs::write(&pdf_path, pdf)?;
    std::fs::write(&html_path, html)?;
    std::fs::write(&page_one_path, page_one_svg)?;
    std::fs::write(&page_two_path, page_two_svg)?;
    std::fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let inspector = SceneInspector::new(&scene);
    println!(
        "pages={} roles={:?}/{:?} undo={} pdf_bytes={} html_bytes={} svg_bytes={}/{}\npdf={}\nhtml={}\npage_1={}\npage_2={}\npreflight={}",
        scene.pages.len(),
        inspector.inspect_page(0)?.role,
        inspector.inspect_page(1)?.role,
        log.undo_len(),
        pdf_outcome.bytes_written,
        html_outcome.bytes_written,
        page_one_outcome.bytes_written,
        page_two_outcome.bytes_written,
        pdf_path.display(),
        html_path.display(),
        page_one_path.display(),
        page_two_path.display(),
        report_path.display()
    );
    Ok(())
}

fn export_svg_page(
    scene: &ResolvedScene,
    page: usize,
    context: &ExportContext<'_>,
) -> appcore_filemaker::Result<(Vec<u8>, ExportOutcome)> {
    export_bytes(
        scene,
        &ExportRequest {
            format: ExportFormat::Svg,
            page: Some(page),
            ..ExportRequest::default()
        },
        context,
    )
}

fn example_fonts() -> Result<FontManager, Box<dyn std::error::Error>> {
    let mut fonts = FontManager::default();
    fonts.register(FontAsset::new(
        "NotoSans",
        include_bytes!("assets/NotoSans-Regular.ttf").to_vec(),
        0,
    )?)?;
    Ok(fonts)
}

fn output_path(name: &str) -> Result<PathBuf, std::io::Error> {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/filemaker-examples");
    std::fs::create_dir_all(&directory)?;
    Ok(directory.join(name))
}
