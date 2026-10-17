// =============================================================================
//        #######
//     ###       ###     F: adaptive_header.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 09:22:55 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Automatic two-column header, measured first body and final-only closing.
use appcore_filemaker::*;
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(include_bytes!("adaptive-header.yml"))?;
    let plan = AdaptiveLayoutPlan::parse_yaml(include_bytes!("adaptive-header-plan.yml"), &limits)?;
    let mut fonts = FontManager::default();
    fonts.register(FontAsset::new(
        "Body",
        include_bytes!("assets/NotoSans-Regular.ttf").to_vec(),
        0,
    )?)?;
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())?;
    let request = ExportRequest {
        format: ExportFormat::Pdf,
        pdf_mode: PdfMode::Hybrid,
        ..ExportRequest::default()
    };
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let output =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/filemaker-examples");
    std::fs::create_dir_all(&output)?;
    for (name, bytes) in [
        (
            "short",
            include_bytes!("adaptive-header-short.json").as_slice(),
        ),
        (
            "long",
            include_bytes!("adaptive-header-long.json").as_slice(),
        ),
    ] {
        let data: DataValue = serde_json::from_slice(bytes)?;
        let document = compiler.bind(&template, &data, &[])?;
        let scene = engine.resolve_adaptive(&document, &plan)?;
        audit_layout(
            &scene,
            &limits,
            &LayoutSafetyOptions::default(),
            &OperationControl::default(),
        )?
        .validation
        .enforce(true)?;
        let report = preflight(
            &scene,
            &request,
            &context,
            &PreflightOptions {
                strict: true,
                ..PreflightOptions::default()
            },
            &OperationControl::default(),
        )?;
        report.enforce(true)?;
        let (pdf, _) = export_bytes(&scene, &request, &context)?;
        std::fs::write(output.join(format!("adaptive-header-{name}.pdf")), pdf)?;
        std::fs::write(
            output.join(format!("adaptive-header-{name}.json")),
            serde_json::to_vec_pretty(&scene)?,
        )?;
        println!(
            "{} pages={}",
            output.join(format!("adaptive-header-{name}.pdf")).display(),
            scene.pages.len()
        );
    }
    Ok(())
}
