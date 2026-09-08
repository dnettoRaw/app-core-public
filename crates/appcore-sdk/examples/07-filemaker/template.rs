// =============================================================================
//        #######
//     ###       ###     F: template.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 16:22:29 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Compile and export a deterministic `FileMaker` document as PDF.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use appcore_sdk::filemaker::{
    export_bytes, Compiler, DataValue, ExportContext, ExportFormat, ExportRequest, FontManager,
    LayoutEngine, LayoutOptions, ResourceLimits,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;

    // The example template is bundled as bytes, so no resolver is involved.
    let template = compiler.compile_template_yaml(include_bytes!("template.yml"))?;
    let document = compiler.bind(&template, &DataValue::Object(BTreeMap::new()), &[])?;
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())?.resolve(&document)?;
    let (pdf, outcome) = export_bytes(
        &scene,
        &ExportRequest {
            format: ExportFormat::Pdf,
            ..ExportRequest::default()
        },
        &ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        },
    )?;
    let output_path = output_path("filemaker-template.pdf")?;
    std::fs::write(&output_path, pdf)?;

    appcore_sdk::run("filemaker-template", move |app| {
        let log = app.logger().component("filemaker");

        log.info(format!(
            "FileMaker PDF created: {} ({} bytes)",
            output_path.display(),
            outcome.bytes_written
        ));

        Ok(())
    })?;
    Ok(())
}

fn output_path(name: &str) -> Result<PathBuf, std::io::Error> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/appcore-sdk-examples");
    std::fs::create_dir_all(&directory)?;
    Ok(directory.canonicalize()?.join(name))
}
