// =============================================================================
//        #######
//     ###       ###     F: pages.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeMap;
use std::sync::Arc;

use appcore_filemaker::{
    export, preflight, CollisionPolicy, Compiler, DataValue, ErrorCode, ExportContext,
    ExportFormat, ExportRequest, FontAsset, FontManager, LayoutEngine, LayoutOptions,
    MemoryResolver, OperationControl, PageRole, Patch, PatchOperation, PreflightOptions,
    ResourceLimits, Size, Unit,
};

const ROLE_YAML: &[u8] = br"filemaker: '1.0'
model: document
id: page-roles
collision: { policy: next_page }
page:
  width: 100pt
  height: 80pt
  master:
    background:
      - { id: master-bg, type: rect, x: 0pt, y: 0pt, width: 100pt, height: 80pt }
    header:
      - { id: master-header, type: rect, x: 0pt, y: 0pt, width: 10pt, height: 5pt }
  first:
    header:
      - { id: first-header, type: rect, x: 10pt, y: 0pt, width: 10pt, height: 5pt }
  continuation:
    header:
      - { id: continuation-header, type: rect, x: 20pt, y: 0pt, width: 10pt, height: 5pt }
  last:
    footer:
      - { id: last-footer, type: rect, x: 0pt, y: 75pt, width: 10pt, height: 5pt }
elements:
  - { id: body-1, type: rect, x: 0pt, y: 10pt, width: 20pt, height: 20pt }
  - { id: body-2, type: rect, x: 0pt, y: 10pt, width: 20pt, height: 20pt }
  - { id: body-3, type: rect, x: 0pt, y: 10pt, width: 20pt, height: 20pt }
";

const BODY_ONLY_YAML: &[u8] = br"filemaker: '1.0'
model: document
id: page-roles
collision: { policy: next_page }
page: { width: 100pt, height: 80pt }
elements:
  - { id: body-1, type: rect, x: 0pt, y: 10pt, width: 20pt, height: 20pt }
  - { id: body-2, type: rect, x: 0pt, y: 10pt, width: 20pt, height: 20pt }
  - { id: body-3, type: rect, x: 0pt, y: 10pt, width: 20pt, height: 20pt }
";

#[test]
fn master_and_role_layers_repeat_without_changing_body_pagination() {
    let scene = compile(ROLE_YAML, &FontManager::default()).unwrap();
    let baseline = compile(BODY_ONLY_YAML, &FontManager::default()).unwrap();
    assert_eq!(scene.pages.len(), 3);
    assert_eq!(scene.pages.len(), baseline.pages.len());
    assert_eq!(
        scene.pages.iter().map(|page| page.role).collect::<Vec<_>>(),
        [PageRole::First, PageRole::Continuation, PageRole::Last]
    );
    for (index, page) in scene.pages.iter().enumerate() {
        assert_eq!(page.page_template.as_ref().unwrap().role, page.role);
        assert!(has(page, "master-bg"));
        assert!(has(page, "master-header"));
        assert!(has(page, &format!("body-{}", index + 1)));
        let body = page
            .elements
            .iter()
            .find(|element| element.id.as_str().starts_with("body-"))
            .unwrap();
        let baseline_body = baseline.pages[index]
            .elements
            .iter()
            .find(|element| element.id.as_str().starts_with("body-"))
            .unwrap();
        assert_eq!(body.bounds, baseline_body.bounds);
    }
    assert!(has(&scene.pages[0], "first-header"));
    assert!(!has(&scene.pages[0], "continuation-header"));
    assert!(has(&scene.pages[1], "continuation-header"));
    assert!(has(&scene.pages[2], "last-footer"));
    assert_eq!(
        appcore_filemaker::SceneInspector::new(&scene)
            .inspect_page(2)
            .unwrap()
            .role,
        PageRole::Last
    );
    let limits = ResourceLimits::default();
    let fonts = FontManager::default();
    let report = preflight(
        &scene,
        &ExportRequest::default(),
        &ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        },
        &PreflightOptions {
            strict: true,
            ..PreflightOptions::default()
        },
        &OperationControl::default(),
    )
    .unwrap();
    assert!(report.issues.is_empty());
    assert!(!appcore_filemaker::SceneInspector::new(&scene)
        .query_free_regions(
            0,
            Size::new(Unit::points(1).unwrap(), Unit::points(1).unwrap()).unwrap(),
        )
        .unwrap()
        .is_empty());
}

#[test]
fn page_number_placeholders_are_resolved_after_total_is_known() {
    let bytes = deterministic_test_font();
    let yaml = br"filemaker: '1.0'
model: document
id: numbered-pages
collision: { policy: next_page }
page:
  width: 100pt
  height: 80pt
  master:
    footer:
      - id: page-number
        type: text
        text: 'Page {page}/{pages}'
        x: 0pt
        y: 60pt
        width: 80pt
        height: 10pt
        style: { font: Body, font_size: 8pt }
elements:
  - { id: body-1, type: rect, width: 20pt, height: 20pt }
  - { id: body-2, type: rect, width: 20pt, height: 20pt }
";
    let mut fonts = FontManager::default();
    fonts
        .register(FontAsset::new("Body", bytes, 0).unwrap())
        .unwrap();
    let scene = compile(yaml, &fonts).unwrap();
    assert_eq!(scene.pages.len(), 2);
    assert_eq!(text(&scene.pages[0], "page-number"), "Page 1/2");
    assert_eq!(text(&scene.pages[1], "page-number"), "Page 2/2");
    let limits = ResourceLimits::default();
    let mut html = Vec::new();
    export(
        &scene,
        &ExportRequest {
            format: ExportFormat::Html,
            ..ExportRequest::default()
        },
        &ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        },
        &mut html,
    )
    .unwrap();
    let html = String::from_utf8(html).unwrap();
    assert!(html.contains("Page 1/2"));
    assert!(html.contains("Page 2/2"));
}

#[test]
fn paginated_table_and_master_footer_share_the_final_page_count() {
    let bytes = deterministic_test_font();
    let yaml = br"filemaker: '1.0'
model: document
id: table-footer
page:
  width: 100pt
  height: 80pt
  master:
    footer:
      - id: footer-number
        type: text
        text: '{page}/{pages}'
        x: 0pt
        y: 70pt
        width: 40pt
        height: 8pt
        style: { font: Body, font_size: 7pt }
elements:
  - id: rows
    type: table
    binding: data.rows
    width: 80pt
    height: 50pt
    style: { font: Body, font_size: 7pt }
    table:
      columns:
        - { field: name, header: Name, width: { mode: flex, value: 1 } }
      repeat_header: true
      header_height: 10pt
      row_height: 20pt
      max_rows: 8
      max_row_fields: 2
      max_cell_bytes: 32
";
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let rows = (1..=5)
        .map(|index| {
            DataValue::Object(BTreeMap::from([(
                "name".to_owned(),
                DataValue::String(format!("Row {index}")),
            )]))
        })
        .collect();
    let document = compiler
        .bind(
            &template,
            &DataValue::Object(BTreeMap::from([(
                "rows".to_owned(),
                DataValue::Array(rows),
            )])),
            &[],
        )
        .unwrap();
    let mut fonts = FontManager::default();
    fonts
        .register(FontAsset::new("Body", bytes, 0).unwrap())
        .unwrap();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    assert_eq!(scene.pages.len(), 3);
    for (index, page) in scene.pages.iter().enumerate() {
        assert!(has(page, "rows"));
        assert_eq!(text(page, "footer-number"), format!("{}/3", index + 1));
    }
}

#[test]
fn unsafe_page_layer_features_fail_at_the_source_boundary() {
    let yaml = br"filemaker: '1.0'
model: document
id: invalid-page-layer
page:
  width: 100pt
  height: 80pt
  master:
    header:
      - { id: bad, type: rect, width: 10pt, height: 10pt, collision: true }
";
    let error = Compiler::builder()
        .build()
        .unwrap()
        .compile_template_yaml(yaml)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::SchemaField);

    let canvas = br"filemaker: '1.0'
model: canvas
id: invalid-canvas-layer
page:
  width: 100pt
  height: 80pt
  master:
    background:
      - { id: background, type: rect, width: 10pt, height: 10pt }
";
    let error = Compiler::builder()
        .build()
        .unwrap()
        .compile_template_yaml(canvas)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::SchemaField);
}

#[test]
fn page_layers_participate_in_component_expansion_and_binding() {
    let yaml = br"filemaker: '1.0'
model: document
id: component-page-layer
page:
  width: 100pt
  height: 80pt
  master:
    header:
      - { id: badge-instance, type: group, component: badge, styles: [accent] }
components:
  badge:
    elements:
      - { id: mark, type: rect, width: 10pt, height: 5pt }
styles:
  accent: { fill: '#336699' }
";
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let header = template
        .elements
        .iter()
        .find(|element| element.id.as_str() == "badge-instance")
        .unwrap();
    assert_eq!(header.page_placement.unwrap().role, PageRole::Master);
    assert!(header.style.fill.is_some());
    assert_eq!(header.children[0].id.as_str(), "badge-instance/mark");
    assert_eq!(header.children[0].provenance.components, ["badge"]);
    assert_eq!(header.children[0].layer, "~page-content");
    assert!(!header.children[0].collision.as_ref().unwrap().enabled);
}

#[test]
fn included_fragment_cannot_take_ownership_of_physical_page_layers() {
    let root = br"filemaker: '1.0'
model: document
id: root
page: { width: 100pt, height: 80pt }
includes: [{ path: fragment.yaml }]
";
    let fragment = br"filemaker: '1.0'
model: document
id: fragment
page:
  master:
    header:
      - { id: foreign-header, type: rect, width: 10pt, height: 5pt }
";
    let mut resolver = MemoryResolver::default();
    resolver
        .insert("fragment.yaml", "application/yaml", fragment.to_vec())
        .unwrap();
    let error = Compiler::builder()
        .template_resolver(Arc::new(resolver))
        .build()
        .unwrap()
        .compile_template_yaml(root)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::SchemaField);
}

#[test]
fn patch_cannot_break_the_page_layer_collision_contract() {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(ROLE_YAML).unwrap();
    let mut document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let original = document.clone();
    let mut replacement = document
        .elements
        .iter()
        .find(|element| element.id.as_str() == "master-header")
        .unwrap()
        .clone();
    replacement.collision = Some(CollisionPolicy::default());
    let error = appcore_filemaker::PatchTransaction::new(&mut document, 1)
        .apply(&Patch {
            sequence: 1,
            operations: vec![PatchOperation::Replace {
                id: appcore_filemaker::ElementId::new("master-header").unwrap(),
                element: replacement,
            }],
        })
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LayoutInvalid);
    assert_eq!(document, original);

    let mut replacement = document
        .elements
        .iter()
        .find(|element| element.id.as_str() == "master-header")
        .unwrap()
        .clone();
    replacement.page_placement = None;
    let error = appcore_filemaker::PatchTransaction::new(&mut document, 1)
        .apply(&Patch {
            sequence: 2,
            operations: vec![PatchOperation::Replace {
                id: appcore_filemaker::ElementId::new("master-header").unwrap(),
                element: replacement,
            }],
        })
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PatchInvalid);
    assert_eq!(document, original);
}

fn compile(
    yaml: &[u8],
    fonts: &FontManager,
) -> appcore_filemaker::Result<appcore_filemaker::ResolvedScene> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(yaml)?;
    let document = compiler.bind(&template, &DataValue::Object(BTreeMap::new()), &[])?;
    LayoutEngine::new(&limits, fonts, LayoutOptions::default())?.resolve(&document)
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

fn deterministic_test_font() -> Vec<u8> {
    include_bytes!("../examples/assets/NotoSans-Regular.ttf").to_vec()
}
