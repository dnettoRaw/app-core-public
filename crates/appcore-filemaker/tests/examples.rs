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
fn intermediate_example_resolves_two_numbered_confidential_pages() {
    let scene = resolve_example(
        include_bytes!("../examples/intermediate.yml"),
        include_bytes!("../examples/intermediate-data.json"),
    );
    assert_eq!(
        scene.pages.iter().map(|page| page.role).collect::<Vec<_>>(),
        [PageRole::First, PageRole::Last]
    );
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
}

fn resolve_example(yaml: &[u8], data: &[u8]) -> appcore_filemaker::ResolvedScene {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let data: DataValue = serde_json::from_slice(data).unwrap();
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
