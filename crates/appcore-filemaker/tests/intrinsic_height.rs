// =============================================================================
//        #######
//     ###       ###     F: intrinsic_height.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

use appcore_filemaker::*;

fn document(yaml: &str) -> DocumentIr {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
    compiler
        .bind(&template, &DataValue::Object(Default::default()), &[])
        .unwrap()
}

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

fn intrinsic(yaml: &str) -> Result<ResolvedScene> {
    LayoutEngine::new(
        &ResourceLimits::default(),
        &fonts(),
        LayoutOptions::default(),
    )
    .unwrap()
    .resolve_intrinsic(&document(yaml))
}

const COLUMNS: &str = include_str!("../wiki/audit/intrinsic-columns.yml");

#[test]
fn automatic_columns_move_the_dependent_anchor() {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler
        .compile_template_yaml(include_bytes!("../wiki/audit/intrinsic-columns.yml"))
        .unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(Default::default()), &[])
        .unwrap();
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
    let limits = ResourceLimits::default();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    let ordinary = engine.resolve(&document).unwrap();
    let scene = engine.resolve_intrinsic(&document).unwrap();
    let find = |id: &str| {
        scene.pages[0]
            .elements
            .iter()
            .find(|e| e.id.as_str() == id)
            .unwrap()
    };
    assert_eq!(
        find("columns").bounds.layout.size.height,
        find("left").bounds.layout.size.height
    );
    assert_eq!(
        find("dependent").bounds.layout.origin.y,
        find("columns").bounds.layout.bottom().unwrap()
    );
    assert!(
        find("columns").bounds.layout.size.height
            > ordinary.pages[0].elements[0].bounds.layout.size.height
    );
    assert_eq!(engine.resolve_intrinsic(&document).unwrap(), scene);
    assert_eq!(document.elements[0].geometry.height, Some(Length::Auto));
}

#[test]
fn minimum_height_preserves_short_header_geometry() {
    let yaml = COLUMNS.replace(
        "height: auto\n    layout",
        "height: auto\n    constraints: { min_height: 60pt }\n    layout",
    );
    let scene = intrinsic(&yaml).unwrap();
    assert_eq!(
        scene.pages[0].elements[0].bounds.layout.size.height,
        Unit::points(60).unwrap()
    );
    assert_eq!(
        scene.pages[0]
            .elements
            .last()
            .unwrap()
            .bounds
            .layout
            .origin
            .y,
        Unit::points(60).unwrap()
    );
}

#[test]
fn vertical_flow_sums_children_and_gap() {
    let yaml = COLUMNS.replace("flow_horizontal", "flow_vertical\n    gap: 7pt");
    let scene = intrinsic(&yaml).unwrap();
    let nodes = &scene.pages[0].elements;
    assert_eq!(
        nodes[0].bounds.layout.size.height,
        nodes[1]
            .bounds
            .layout
            .size
            .height
            .checked_add(nodes[2].bounds.layout.size.height)
            .unwrap()
            .checked_add(Unit::points(7).unwrap())
            .unwrap()
    );
}

#[test]
fn text_uses_padding_line_height_and_same_export_measurement() {
    let yaml = COLUMNS.replace("overflow: expand", "overflow: error, line_height: 1500000, padding: {top: 3pt, bottom: 5pt, left: 2pt, right: 2pt}");
    let scene = intrinsic(&yaml).unwrap();
    let node = &scene.pages[0].elements[1];
    assert_eq!(node.bounds.layout.size.height, Unit::points(62).unwrap());
    assert!(node.text_layout.as_ref().unwrap().diagnostics.is_empty());
}

#[test]
fn fixed_geometry_and_ordinary_resolution_are_unchanged() {
    let yaml = COLUMNS.replace("height: auto", "height: 70pt");
    let document = document(&yaml);
    let fonts = fonts();
    let limits = ResourceLimits::default();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    assert_eq!(
        engine.resolve(&document).unwrap(),
        engine.resolve_intrinsic(&document).unwrap()
    );
}

#[test]
fn intrinsic_rejects_loss_and_circular_or_unsupported_geometry() {
    for yaml in [
        COLUMNS.replace("overflow: expand", "overflow: expand, max_lines: 2"),
        COLUMNS.replace("overflow: expand", "overflow: clip"),
        COLUMNS.replace("overflow: expand", "overflow: expand, padding: {top: 10%}"),
        COLUMNS.replace(
            "height: auto\n    layout",
            "height: auto\n    constraints: {max_height: 20pt}\n    layout",
        ),
        COLUMNS.replace("flow_horizontal", "absolute"),
        COLUMNS.replace(
            "height: auto\n    layout",
            "height: auto\n    distribute: center\n    layout",
        ),
        COLUMNS.replace("id: left,", "id: left, y: 10pt,"),
        COLUMNS.replace("id: left,", "id: left, align_x: center,"),
        COLUMNS.replace("id: left,", "id: left, transform: {rotate: 15},"),
        COLUMNS.replace("width: 130pt, height: auto", "width: 130pt, height: 50%"),
        COLUMNS.replace("width: 130pt", "width: 160pt"),
    ] {
        assert!(intrinsic(&yaml).is_err(), "unexpectedly accepted {yaml}");
    }
}

#[test]
fn hidden_children_do_not_contribute_to_height() {
    let yaml = COLUMNS.replace("id: left,", "id: left, when: 'false',");
    let scene = intrinsic(&yaml).unwrap();
    assert_eq!(
        scene.pages[0].elements[0].bounds.layout.size.height,
        scene.pages[0].elements[1].bounds.layout.size.height
    );
}

#[test]
fn intrinsic_walk_is_bounded_even_for_manually_built_ir() {
    let document = document(COLUMNS);
    let fonts = fonts();
    let limits = ResourceLimits {
        max_elements: 2,
        ..ResourceLimits::default()
    };
    let error = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve_intrinsic(&document)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LimitExceeded);
}

#[test]
fn cancellation_is_observed_before_intrinsic_measurement() {
    let token = CancellationToken::default();
    token.cancel();
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let engine = LayoutEngine::new_controlled(
        &limits,
        &fonts,
        LayoutOptions::default(),
        OperationControl::new(token),
    )
    .unwrap();
    assert_eq!(
        engine
            .resolve_intrinsic(&document(COLUMNS))
            .unwrap_err()
            .code(),
        ErrorCode::Cancelled
    );
}

#[test]
fn automatic_group_cannot_silently_span_pages() {
    let yaml = COLUMNS.replace("height: 200pt", "height: 30pt");
    let error = intrinsic(&yaml).unwrap_err();
    assert!(error.to_string().contains("one useful page"));
}

#[test]
fn auto_page_band_is_rejected_and_fixed_band_preserved() {
    let yaml = "filemaker: '1.0'\nmodel: document\nid: bands\npage:\n  width: 300pt\n  height: 200pt\n  first:\n    header:\n      - {id: band, type: text, width: 100pt, height: auto, text: Header, style: {font: Body, font_size: 9pt}}\nelements: [{id: body, type: rect, width: 10pt, height: 10pt}]\n";
    let error = intrinsic(yaml).unwrap_err();
    assert!(error.to_string().contains("page bands"));
    assert!(intrinsic(&yaml.replace("height: auto", "height: 20pt")).is_ok());
}

#[test]
fn intrinsic_opt_in_preserves_one_two_five_twenty_page_tables_and_row_order() {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler
        .compile_template_yaml(include_bytes!("../wiki/audit/matrix.yml"))
        .unwrap();
    let limits = ResourceLimits::default();
    let fonts = fonts();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    for (count, bytes) in [
        (
            1,
            include_bytes!("../wiki/audit/matrix-1.data.json").as_slice(),
        ),
        (
            2,
            include_bytes!("../wiki/audit/matrix-2.data.json").as_slice(),
        ),
        (
            5,
            include_bytes!("../wiki/audit/matrix-5.data.json").as_slice(),
        ),
        (
            20,
            include_bytes!("../wiki/audit/matrix-20.data.json").as_slice(),
        ),
    ] {
        let data: DataValue = serde_json::from_slice(bytes).unwrap();
        let document = compiler.bind(&template, &data, &[]).unwrap();
        let scene = engine.resolve_intrinsic(&document).unwrap();
        assert_eq!(scene.pages.len(), count);
        assert_eq!(scene, engine.resolve(&document).unwrap());
        let indices: Vec<_> = scene
            .pages
            .iter()
            .flat_map(|p| &p.elements)
            .filter_map(|e| e.table.as_ref())
            .flat_map(|t| t.rows.iter().map(|r| r.source_index))
            .collect();
        assert_eq!(indices, (0..count as u64 * 6).collect::<Vec<_>>());
    }
}

#[test]
fn automatic_container_does_not_push_its_own_children() {
    let scene = intrinsic(&COLUMNS.replace("collision: false\n", "")).unwrap();
    let nodes = &scene.pages[0].elements;
    assert_eq!(
        nodes[1].bounds.layout.origin.y,
        nodes[0].bounds.layout.origin.y
    );
    assert_eq!(
        nodes[2].bounds.layout.origin.y,
        nodes[0].bounds.layout.origin.y
    );
    assert_eq!(
        nodes[3].bounds.layout.origin.y,
        nodes[0].bounds.layout.bottom().unwrap()
    );
}
