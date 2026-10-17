// =============================================================================
//        #######
//     ###       ###     F: pdf_unicode.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Multi-codepoint `CMaps` and context-specific logical text in native PDF modes.
use appcore_filemaker::*;
use std::collections::BTreeMap;
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().build()?;
    let template = compiler.compile_template_yaml(include_bytes!("pdf-unicode.yml"))?;
    let document = compiler.bind(&template, &DataValue::Object(BTreeMap::new()), &[])?;
    let mut fonts = FontManager::default();
    for (name, bytes) in [
        (
            "Body",
            include_bytes!("assets/NotoSans-Regular.ttf").as_slice(),
        ),
        (
            "Marks",
            include_bytes!("assets/NotoSans-Regular.ttf").as_slice(),
        ),
        (
            "Arabic",
            include_bytes!("../tests/assets/NotoSansArabic-Test.ttf").as_slice(),
        ),
    ] {
        fonts.register(FontAsset::new(name, bytes.to_vec(), 0)?)?;
    }
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())?.resolve(&document)?;
    audit_layout(
        &scene,
        &limits,
        &LayoutSafetyOptions::default(),
        &OperationControl::default(),
    )?
    .validation
    .enforce(true)?;
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let out =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/filemaker-examples");
    std::fs::create_dir_all(&out)?;
    for (name, pdf_mode) in [
        ("editable", PdfMode::Editable),
        ("hybrid", PdfMode::Hybrid),
        ("flattened", PdfMode::Flattened),
    ] {
        let request = ExportRequest {
            format: ExportFormat::Pdf,
            pdf_mode,
            ..ExportRequest::default()
        };
        preflight(
            &scene,
            &request,
            &context,
            &PreflightOptions {
                strict: true,
                ..PreflightOptions::default()
            },
            &OperationControl::default(),
        )?
        .enforce(true)?;
        let (bytes, _) = export_bytes(&scene, &request, &context)?;
        let path = out.join(format!("pdf-unicode-{name}.pdf"));
        std::fs::write(&path, bytes)?;
        println!("{}", path.display());
    }
    std::fs::write(
        out.join("pdf-unicode-scene.json"),
        serde_json::to_vec_pretty(&scene)?,
    )?;
    Ok(())
}
