// =============================================================================
//        #######
//     ###       ###     F: variable_corpus.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Mixed synthetic variable content with grouped rows and final-only closing.
use appcore_filemaker::*;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(include_bytes!("variable-corpus.yml"))?;
    let plan = AdaptiveLayoutPlan::parse_yaml(include_bytes!("variable-corpus-plan.yml"), &limits)?;
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
    for (pages, bytes) in [
        (1, include_bytes!("variable-corpus-1.json").as_slice()),
        (2, include_bytes!("variable-corpus-2.json").as_slice()),
        (5, include_bytes!("variable-corpus-5.json").as_slice()),
        (20, include_bytes!("variable-corpus-20.json").as_slice()),
    ] {
        let data: DataValue = serde_json::from_slice(bytes)?;
        let document = compiler.bind(&template, &data, &[])?;
        let scene = diagnose(&engine, &document, &plan, &limits, &output, pages)?;
        if scene.pages.len() != pages {
            return Err(std::io::Error::other("unexpected corpus page count").into());
        }
        let layout = audit_layout(
            &scene,
            &limits,
            &LayoutSafetyOptions::default(),
            &OperationControl::default(),
        )?;
        layout.validation.enforce(true)?;
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
        let name = format!("variable-corpus-{pages}");
        std::fs::write(output.join(format!("{name}.pdf")), pdf)?;
        std::fs::write(
            output.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&scene)?,
        )?;
        std::fs::write(
            output.join(format!("{name}-preflight.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        println!(
            "{} pages={pages}",
            output.join(format!("{name}.pdf")).display()
        );
    }
    Ok(())
}

fn diagnose(
    engine: &LayoutEngine<'_>,
    document: &DocumentIr,
    plan: &AdaptiveLayoutPlan,
    limits: &ResourceLimits,
    output: &std::path::Path,
    pages: usize,
) -> std::result::Result<ResolvedScene, Box<dyn std::error::Error>> {
    let diagnostics = engine.diagnose_adaptive(document, plan, limits.max_elements);
    std::fs::write(
        output.join(format!("variable-corpus-{pages}-pagination.json")),
        serde_json::to_vec_pretty(diagnostics.decisions())?,
    )?;
    let scene = diagnostics.into_outcome()?;
    let overlays = scene
        .pages
        .iter()
        .map(|page| {
            DebugOverlay::build_bounded(
                &scene,
                page.index,
                &DebugOverlayOptions {
                    grid: None,
                    ruler: false,
                    ids: true,
                    coordinates: true,
                    bounds: true,
                    anchors: true,
                    regions: true,
                    safe_area: true,
                    collision: true,
                    crosshair: false,
                    view: MaskView::Combined,
                },
                limits,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    std::fs::write(
        output.join(format!("variable-corpus-{pages}-debug.json")),
        serde_json::to_vec_pretty(&overlays)?,
    )?;

    for page in &scene.pages {
        let mask = CollisionMask::derive_bounded(&scene, page.index, MaskView::Combined, limits)?;
        let mut svg = Vec::new();
        export_collision_mask(&mask, MaskFormat::Svg, 96, limits, &mut svg)?;
        std::fs::write(
            output.join(format!("variable-corpus-{pages}-mask-{}.svg", page.index)),
            svg,
        )?;
    }
    Ok(scene)
}
