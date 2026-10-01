// =============================================================================
//        #######
//     ###       ###     F: examples.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 10:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 10:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================
// appcore-norm: test

use appcore_filemaker::{
    Compiler, DataValue, FontAsset, FontManager, LayoutEngine, LayoutOptions, PageRole,
    ResourceLimits,
};
use std::collections::BTreeMap;

#[test]
fn basic_example_resolves_as_a_complete_single_page() {
    let scene = resolve_example(
        include_bytes!("../examples/basic.yml"),
        include_bytes!("../examples/basic-data.json"),
    );
    assert_eq!(scene.pages.len(), 1);
    assert!(has(&scene.pages[0], "report-title"));
    assert!(has(&scene.pages[0], "sparkline"));
    assert!(has(&scene.pages[0], "metrics-table"));
}

#[test]
fn row_anchor_reserve_example_keeps_following_content_on_the_anchored_row_page() {
    let scene = resolve_example(
        include_bytes!("../examples/row-anchor-reserve.yml"),
        include_bytes!("../examples/row-anchor-reserve-data.json"),
    );
    assert_eq!(scene.pages.len(), 3);
    let row_page = scene
        .pages
        .iter()
        .position(|page| {
            page.elements
                .iter()
                .filter_map(|element| element.table.as_ref())
                .flat_map(|table| &table.rows)
                .flat_map(|row| &row.cells)
                .any(|cell| cell.field == "name" && cell.text == "Middle")
        })
        .unwrap();
    let anchored_page = scene
        .pages
        .iter()
        .position(|page| has(page, "after-row"))
        .unwrap();
    assert_eq!(row_page, 1);
    assert_eq!(anchored_page, row_page);
}

#[test]
fn intermediate_example_resolves_two_numbered_confidential_pages() {
    let scene = resolve_example(
        include_bytes!("../examples/intermediate.yml"),
        include_bytes!("../examples/intermediate-data.json"),
    );
    assert_eq!(
        scene.pages.iter().map(|page| page.role).collect::<Vec<_>>(),
        [PageRole::First, PageRole::Last]
    );
    assert!(scene
        .pages
        .iter()
        .flat_map(|page| &page.elements)
        .any(|element| element.id.as_str() == "volume-title" && element.style.underline));
    assert!(scene
        .pages
        .iter()
        .flat_map(|page| &page.elements)
        .filter(|element| element.id.as_str() == "report-table")
        .flat_map(|element| element.table.iter())
        .flat_map(|fragment| fragment.rows.iter())
        .flat_map(|row| &row.cells)
        .any(|cell| cell.text == "Watch"
            && cell.style.stroke_sides.top
            && !cell.style.stroke_sides.right
            && cell.style.stroke_sides.bottom
            && !cell.style.stroke_sides.left));
    for page in &scene.pages {
        assert!(has(page, "master-page-number"));
        assert!(has(page, "report-table"));
    }
    assert_eq!(text(&scene.pages[0], "master-page-number"), "Page 1 of 2");
    assert_eq!(text(&scene.pages[1], "master-page-number"), "Page 2 of 2");
    assert!(has(&scene.pages[0], "volume-chart"));
    assert!(has(&scene.pages[0], "confidential-watermark"));
    assert!(has(&scene.pages[1], "appendix-bar-east"));
    assert!(has(&scene.pages[1], "last-confidential-watermark"));
    let mut region_pages = BTreeMap::new();
    for (page_index, page) in scene.pages.iter().enumerate() {
        for row in page
            .elements
            .iter()
            .filter_map(|element| element.table.as_ref())
            .flat_map(|table| &table.rows)
        {
            let region = row
                .cells
                .iter()
                .find(|cell| cell.field == "region")
                .unwrap()
                .text
                .clone();
            if let Some(previous_page) = region_pages.insert(region, page_index) {
                assert_eq!(previous_page, page_index);
            }
        }
    }
}

#[test]
fn deterministic_table_data_paginates_to_three_and_four_or_more_pages() {
    let source_data =
        serde_json::from_slice::<DataValue>(include_bytes!("../examples/intermediate-data.json"))
            .unwrap();
    for (row_count, minimum_pages) in [(18, 3), (48, 4)] {
        let mut data = source_data.clone();
        let DataValue::Object(root) = &mut data else {
            panic!("example data root must be an object");
        };
        let Some(DataValue::Array(rows)) = root.get_mut("table_rows") else {
            panic!("example table_rows must be an array");
        };
        let row = rows[0].clone();
        rows.resize(row_count, row);
        let yaml = String::from_utf8_lossy(include_bytes!("../examples/intermediate.yml"))
            .replace("max_rows: 16", &format!("max_rows: {row_count}"));
        let scene = resolve_data(yaml.as_bytes(), data);
        assert!(scene.pages.len() >= minimum_pages);
        if row_count == 18 {
            assert_eq!(scene.pages.len(), 3);
        }
        assert_eq!(scene.pages.first().unwrap().role, PageRole::First);
        assert_eq!(scene.pages.last().unwrap().role, PageRole::Last);
        assert!(scene.pages[1..scene.pages.len() - 1]
            .iter()
            .all(|page| page.role == PageRole::Continuation));
        let mut source_rows: Vec<_> = scene
            .pages
            .iter()
            .flat_map(|page| &page.elements)
            .filter_map(|element| element.table.as_ref())
            .flat_map(|fragment| &fragment.rows)
            .map(|row| row.source_index)
            .collect();
        source_rows.sort_unstable();
        assert_eq!(
            source_rows,
            (0..row_count)
                .map(|index| u64::try_from(index).unwrap())
                .collect::<Vec<_>>()
        );
    }
}

fn resolve_example(yaml: &[u8], data: &[u8]) -> appcore_filemaker::ResolvedScene {
    let data: DataValue = serde_json::from_slice(data).unwrap();
    resolve_data(yaml, data)
}

fn resolve_data(yaml: &[u8], data: DataValue) -> appcore_filemaker::ResolvedScene {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let document = compiler.bind(&template, &data, &[]).unwrap();
    let mut fonts = FontManager::default();
    fonts
        .register(
            FontAsset::new(
                "NotoSans",
                include_bytes!("../examples/assets/NotoSans-Regular.ttf").to_vec(),
                0,
            )
            .unwrap(),
        )
        .unwrap();
    LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap()
}

fn has(page: &appcore_filemaker::ResolvedPage, id: &str) -> bool {
    page.elements
        .iter()
        .any(|element| element.id.as_str() == id)
}

fn text<'a>(page: &'a appcore_filemaker::ResolvedPage, id: &str) -> &'a str {
    page.elements
        .iter()
        .find(|element| element.id.as_str() == id)
        .and_then(|element| element.text.as_deref())
        .unwrap()
}
