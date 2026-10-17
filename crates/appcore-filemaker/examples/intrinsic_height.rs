// =============================================================================
//        #######
//     ###       ###     F: intrinsic_height.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 09:22:55 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Produces short/long intrinsic-height PDFs from one YAML design.
use appcore_filemaker::*;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(include_bytes!("intrinsic-height.yml"))?;
    let mut fonts = FontManager::default();
    fonts.register(FontAsset::new(
        "Body",
        include_bytes!("assets/NotoSans-Regular.ttf").to_vec(),
        0,
    )?)?;
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())?;
    let output =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/filemaker-examples");
    std::fs::create_dir_all(&output)?;
    for (name, bytes) in [
        (
            "short",
            include_bytes!("intrinsic-height-short.json").as_slice(),
        ),
        (
            "long",
            include_bytes!("intrinsic-height-long.json").as_slice(),
        ),
    ] {
        let data: DataValue = serde_json::from_slice(bytes)?;
        let document = compiler.bind(&template, &data, &[])?;
        let scene = engine.resolve_intrinsic(&document)?;
        let safety = audit_layout(
            &scene,
            &limits,
            &LayoutSafetyOptions::default(),
            &OperationControl::default(),
        )?;
        safety.validation.enforce(true)?;
        let request = ExportRequest {
            format: ExportFormat::Pdf,
            pdf_mode: PdfMode::Hybrid,
            ..ExportRequest::default()
        };
        let (pdf, _) = export_bytes(
            &scene,
            &request,
            &ExportContext {
                limits: &limits,
                fonts: &fonts,
                assets: None,
            },
        )?;
        let path = output.join(format!("intrinsic-height-{name}.pdf"));
        std::fs::write(&path, pdf)?;
        std::fs::write(
            output.join(format!("intrinsic-height-{name}.json")),
            serde_json::to_vec_pretty(&scene)?,
        )?;
        println!(
            "{} pages={} header={:?}",
            path.display(),
            scene.pages.len(),
            scene.pages[0].elements[0].bounds.layout.size.height
        );
    }
    Ok(())
}
