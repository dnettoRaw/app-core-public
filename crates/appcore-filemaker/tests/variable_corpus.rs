// =============================================================================
//        #######
//     ###       ###     F: variable_corpus.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

use appcore_filemaker::*;
use std::collections::BTreeMap;

const CASES: [(usize, &[u8]); 4] = [
    (1, include_bytes!("../examples/variable-corpus-1.json")),
    (2, include_bytes!("../examples/variable-corpus-2.json")),
    (5, include_bytes!("../examples/variable-corpus-5.json")),
    (20, include_bytes!("../examples/variable-corpus-20.json")),
];
fn fonts() -> FontManager {
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
    fonts
}
#[test]
fn mixed_variable_content_preserves_rows_groups_and_final_elements() {
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler
        .compile_template_yaml(include_bytes!("../examples/variable-corpus.yml"))
        .unwrap();
    let plan = AdaptiveLayoutPlan::parse_yaml(
        include_bytes!("../examples/variable-corpus-plan.yml"),
        &limits,
    )
    .unwrap();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    for (pages, bytes) in CASES {
        let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let data: DataValue = serde_json::from_slice(bytes).unwrap();
        let document = compiler.bind(&template, &data, &[]).unwrap();
        let report = engine.diagnose_adaptive(&document, &plan, limits.max_elements);
        let scene = report.into_outcome().unwrap();
        assert_eq!(scene.pages.len(), pages, "fixture {pages}");
        assert_rows_and_closing(&scene, &raw, pages);
        let safety = audit_layout(
            &scene,
            &limits,
            &LayoutSafetyOptions::default(),
            &OperationControl::default(),
        )
        .unwrap();
        safety.validation.enforce(true).unwrap();
        let context = ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        };
        preflight(
            &scene,
            &ExportRequest {
                format: ExportFormat::Pdf,
                pdf_mode: PdfMode::Hybrid,
                ..ExportRequest::default()
            },
            &context,
            &PreflightOptions {
                strict: true,
                ..PreflightOptions::default()
            },
            &OperationControl::default(),
        )
        .unwrap()
        .enforce(true)
        .unwrap();
    }
}

fn assert_rows_and_closing(scene: &ResolvedScene, raw: &serde_json::Value, pages: usize) {
    let mut ordinals = Vec::new();
    let mut group_pages = BTreeMap::new();
    for page in &scene.pages {
        assert!(page.elements.iter().any(|e| e.id.as_str() == "watermark"));
        assert_eq!(
            page.elements.iter().any(|e| e.id.as_str() == "stamp"),
            page.index == 0
        );
        assert!(page
            .elements
            .iter()
            .find_map(|e| e.table.as_ref())
            .is_some_and(|table| !table.rows.is_empty()));
        for row in page
            .elements
            .iter()
            .filter_map(|e| e.table.as_ref())
            .flat_map(|t| &t.rows)
        {
            ordinals.push(row.source_index);
            let group = row.source_index / 3;
            if let Some(previous) = group_pages.insert(group, page.index) {
                assert_eq!(previous, page.index, "split group {group}");
            }
            for cell in &row.cells {
                assert_eq!(
                    cell.text,
                    raw["value"]["rows"]["value"][row.source_index as usize]["value"][&cell.field]
                        ["value"]
                        .as_str()
                        .unwrap()
                );
                assert_cell_content(cell);
            }
        }
    }
    assert_eq!(
        ordinals,
        (0..raw["value"]["rows"]["value"].as_array().unwrap().len() as u64).collect::<Vec<_>>()
    );
    for id in [
        "closing",
        "summary",
        "terms",
        "signature",
        "code",
        "code-label",
    ] {
        let locations: Vec<_> = scene
            .pages
            .iter()
            .filter(|p| p.elements.iter().any(|e| e.id.as_str() == id))
            .map(|p| p.index)
            .collect();
        assert_eq!(locations, vec![pages - 1], "{id}");
    }
    let page = &scene.pages[0];
    let strike = page
        .elements
        .iter()
        .find(|e| e.id.as_str() == "old-value-strike")
        .unwrap();
    let first = page.elements.iter().find_map(|e| e.table.as_ref()).unwrap();
    let cell = first.rows[0]
        .cells
        .iter()
        .find(|c| c.field == "previous")
        .unwrap();
    assert!(strike.bounds.layout.origin.x >= cell.bounds.origin.x);
    assert!(strike.bounds.layout.right().unwrap() <= cell.bounds.right().unwrap());
    assert!(strike.bounds.layout.origin.y > cell.bounds.origin.y);
    assert!(strike.bounds.layout.origin.y < cell.bounds.bottom().unwrap());
    let summary = scene
        .pages
        .last()
        .unwrap()
        .elements
        .iter()
        .find(|e| e.id.as_str() == "summary")
        .unwrap();
    assert_eq!(
        summary.text.as_deref().unwrap(),
        raw["value"]["summary"]["value"].as_str().unwrap()
    );
}

#[test]
fn oversized_auto_measured_description_fails_without_silent_loss() {
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler
        .compile_template_yaml(include_bytes!("../examples/variable-corpus.yml"))
        .unwrap();
    let plan = AdaptiveLayoutPlan::parse_yaml(
        include_bytes!("../examples/variable-corpus-plan.yml"),
        &limits,
    )
    .unwrap();
    let mut data: DataValue = serde_json::from_slice(CASES[0].1).unwrap();
    let DataValue::Object(root) = &mut data else {
        panic!("fixture object")
    };
    let DataValue::Array(rows) = root.get_mut("rows").unwrap() else {
        panic!("fixture rows")
    };
    let DataValue::Object(row) = &mut rows[1] else {
        panic!("fixture row")
    };
    row.insert("detail".into(), DataValue::String("Line\n".repeat(140)));
    let document = compiler.bind(&template, &data, &[]).unwrap();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    let report = engine.diagnose_adaptive(&document, &plan, 100);
    assert_eq!(
        report.outcome().as_ref().unwrap_err().code(),
        ErrorCode::DataType
    );
    assert_eq!(
        report.decisions().last().unwrap().rule,
        PaginationRule::Reject
    );
}

fn assert_cell_content(cell: &ResolvedTableCell) {
    assert!(cell.text_layout.diagnostics.is_empty());
    let content = cell.content_bounds().unwrap();
    let measured = cell.text_layout.outer_measured().unwrap();
    assert!(measured.height <= content.size.height);
    let logical: String = cell
        .text_layout
        .lines
        .iter()
        .flat_map(|line| &line.runs)
        .map(|run| run.text.as_str())
        .collect();
    let compact = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    assert_eq!(compact(&logical), compact(&cell.text));
}

#[test]
fn watermark_is_painted_before_body_instead_of_covering_glyphs() {
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler
        .compile_template_yaml(include_bytes!("../examples/variable-corpus.yml"))
        .unwrap();
    let plan = AdaptiveLayoutPlan::parse_yaml(
        include_bytes!("../examples/variable-corpus-plan.yml"),
        &limits,
    )
    .unwrap();
    let data = serde_json::from_slice(CASES[0].1).unwrap();
    let document = compiler.bind(&template, &data, &[]).unwrap();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve_adaptive(&document, &plan)
        .unwrap();
    let page = &scene.pages[0];
    let watermark = page
        .elements
        .iter()
        .position(|e| e.id.as_str() == "watermark")
        .unwrap();
    let table = page
        .elements
        .iter()
        .position(|e| e.id.as_str() == "entries")
        .unwrap();
    assert_eq!(page.elements[watermark].layer, "!page-background");
    assert!(watermark < table);
}
