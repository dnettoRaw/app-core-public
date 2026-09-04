// =============================================================================
//        #######
//     ###       ###     F: basic.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 09:22:55 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::path::PathBuf;

use appcore_filemaker::{
    export_bytes, Compiler, DataValue, ExportContext, ExportFormat, ExportRequest, FontAsset,
    FontManager, LayoutEngine, LayoutOptions, ResourceLimits,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(include_bytes!("basic.yml"))?;
    let data: DataValue = serde_json::from_slice(include_bytes!("basic-data.json"))?;
    let document = compiler.bind(&template, &data, &[])?;
    let fonts = example_fonts()?;
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())?.resolve(&document)?;
    let (output, outcome) = export_bytes(
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
    )?;
    let output_path = output_path("basic.svg")?;
    std::fs::write(&output_path, output)?;
    println!(
        "pages={} svg_bytes={} output={}",
        scene.pages.len(),
        outcome.bytes_written,
        output_path.display()
    );
    Ok(())
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
