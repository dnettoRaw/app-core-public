// =============================================================================
//        #######
//     ###       ###     F: variable_content.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

use appcore_filemaker::*;

fn resolve(yaml: &str) -> ResolvedScene {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
    let ir = compiler
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
    LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&ir)
        .unwrap()
}

fn audit(scene: &ResolvedScene) -> LayoutSafetyReport {
    audit_layout(
        scene,
        &ResourceLimits::default(),
        &LayoutSafetyOptions::default(),
        &OperationControl::default(),
    )
    .unwrap()
}

#[test]
fn expanded_text_with_explicit_line_cap_reports_truncation() {
    let scene = resolve(include_str!("../wiki/audit/expand-cap-silent.yml"));
    let text = scene.pages[0].elements[0].text_layout.as_ref().unwrap();
    assert_eq!(text.lines.len(), 4, "the explicit cap remains supported");
    assert!(text.diagnostics.contains(&TextDiagnostic::Clipped));
    assert_eq!(audit(&scene).overflow_count, 1);
    assert!(audit(&scene).validation.enforce(true).is_err());
}

#[test]
fn next_page_restarts_inside_content_margins() {
    let scene = resolve(include_str!("../wiki/audit/next-page-margin.yml"));
    assert_eq!(scene.pages.len(), 2);
    assert_eq!(
        scene.pages[1].elements[0].bounds.layout.origin.y,
        Unit::points(20).unwrap()
    );
}

#[test]
fn layout_audit_respects_groups_and_still_detects_unapproved_overlap() {
    let mut scene = resolve(include_str!("../wiki/audit/allowed-groups.yml"));
    assert_eq!(audit(&scene).collision_count, 0);
    for element in &mut scene.pages[0].elements {
        element.layout_trace.collision_policy.collides_with.clear();
    }
    assert_eq!(audit(&scene).collision_count, 1);
    scene.pages[0].elements[0]
        .layout_trace
        .collision_policy
        .ignore
        .insert("right".into());
    assert_eq!(audit(&scene).collision_count, 0);
}

#[test]
fn kept_expanded_text_counts_block_padding_before_deferring() {
    let scene = resolve(
        r#"filemaker: '1.0'
model: document
id: padded-keep
collision: false
page: { width: 100pt, height: 80pt }
elements:
  - id: flow
    type: group
    width: 100pt
    height: 80pt
    layout: flow_vertical
    children:
      - { id: lead, type: rect, width: 80pt, height: 40pt }
      - id: padded
        type: text
        width: 80pt
        height: 14pt
        text: "One\nTwo"
        keep_with_next: true
        style: { font: Body, font_size: 10pt }
        text_options:
          overflow: expand
          padding: { top: 6pt, bottom: 6pt }
      - { id: tail, type: rect, width: 80pt, height: 8pt }
"#,
    );
    let page = |id| {
        scene
            .pages
            .iter()
            .position(|p| p.elements.iter().any(|e| e.id.as_str() == id))
            .unwrap()
    };
    assert_eq!(page("padded"), 1);
    assert_eq!(page("padded"), page("tail"));
}

#[test]
fn pushed_content_and_anchors_stay_inside_the_continuation_region() {
    for (page, region, expected_y) in [
        (
            "{ width: 160pt, height: 160pt, margin: { top: 20pt, bottom: 20pt } }",
            "",
            20,
        ),
        (
            "{ width: 160pt, height: 160pt }",
            "regions:\n  panel: { x: 0pt, y: 20pt, width: 100pt, height: 120pt }\n",
            20,
        ),
    ] {
        let named = if region.is_empty() {
            ""
        } else {
            ", region: panel"
        };
        let yaml = format!("filemaker: '1.0'\nmodel: document\nid: push-region\npage: {page}\n{region}elements:\n  - {{ id: blocker, type: rect, width: 50pt, height: 110pt{named} }}\n  - {{ id: pushed, type: rect, width: 50pt, height: 20pt{named} }}\n  - {{ id: follower, type: rect, width: 50pt, height: 5pt, anchors: {{ top: pushed.bottom }}{named} }}\n");
        let scene = resolve(&yaml);
        assert_eq!(scene.pages.len(), 2);
        let p = &scene.pages[1];
        let get = |id| p.elements.iter().find(|e| e.id.as_str() == id).unwrap();
        assert_eq!(
            get("pushed").bounds.layout.origin.y,
            Unit::points(expected_y).unwrap()
        );
        assert_eq!(
            get("follower").bounds.layout.origin.y,
            get("pushed").bounds.layout.bottom().unwrap()
        );
    }
}

#[test]
fn footer_band_may_paint_inside_the_physical_bottom_margin() {
    let scene = resolve("filemaker: '1.0'\nmodel: document\nid: footer-margin\npage:\n  width: 160pt\n  height: 160pt\n  margin: { top: 20pt, bottom: 20pt }\n  master:\n    footer:\n      - { id: footer, type: rect, x: 20pt, y: 145pt, width: 80pt, height: 10pt }\nelements:\n  - { id: body, type: rect, width: 50pt, height: 110pt }\n");
    assert_eq!(scene.pages.len(), 1);
    assert_eq!(
        scene.pages[0]
            .elements
            .iter()
            .find(|e| e.id.as_str() == "footer")
            .unwrap()
            .bounds
            .layout
            .origin
            .y,
        Unit::points(145).unwrap()
    );
}

#[test]
fn explicit_overlay_is_accepted_by_audit_even_after_paint_sorting() {
    let scene = resolve("filemaker: '1.0'\nmodel: canvas\nid: local-overlay\npage: { width: 100pt, height: 100pt }\nelements:\n  - { id: body, type: rect, width: 20pt, height: 20pt, z_index: 10 }\n  - { id: stamp, type: rect, width: 20pt, height: 20pt, collision: { policy: overlay }, z_index: 0 }\n");
    assert_eq!(scene.pages[0].elements[0].id.as_str(), "stamp");
    assert_eq!(audit(&scene).collision_count, 0);
}

#[test]
fn shrink_updates_a_dependent_anchor_using_final_geometry() {
    let scene = resolve("filemaker: '1.0'\nmodel: canvas\nid: shrink-anchor\npage: { width: 100pt, height: 100pt }\nelements:\n  - { id: obstacle, type: rect, x: 20pt, y: 20pt, width: 10pt, height: 10pt }\n  - { id: shrinking, type: rect, width: 25pt, height: 25pt, collision: { policy: shrink } }\n  - { id: dependent, type: rect, width: 5pt, height: 5pt, anchors: { left: shrinking.right }, collision: false }\n");
    let get = |id| {
        scene.pages[0]
            .elements
            .iter()
            .find(|e| e.id.as_str() == id)
            .unwrap()
    };
    assert!(get("shrinking").bounds.layout.size.width < Unit::points(20).unwrap());
    assert_eq!(
        get("dependent").bounds.layout.origin.x,
        get("shrinking").bounds.layout.right().unwrap()
    );
    assert_eq!(audit(&scene).collision_count, 0);
}

#[test]
fn paginated_expanded_text_retains_the_source_truncation_diagnostic() {
    let scene = resolve(
        r#"filemaker: '1.0'
model: document
id: capped-fragments
collision: false
page: { width: 100pt, height: 50pt }
elements:
  - id: flow
    type: group
    width: 100pt
    height: 50pt
    layout: flow_vertical
    children:
      - id: capped
        type: text
        width: 90pt
        height: 12pt
        text: "One\nTwo\nThree\nFour\nFive\nSix\nSeven\nEight\nNine"
        style: { font: Body, font_size: 9pt }
        text_options: { overflow: expand, max_lines: 8 }
"#,
    );
    assert!(scene.pages.len() > 1);
    assert!(audit(&scene).overflow_count > 0);
}

#[test]
fn transformed_named_region_uses_page_space_for_continuation() {
    let scene = resolve("filemaker: '1.0'\nmodel: document\nid: translated-region\npage: { width: 160pt, height: 160pt }\nregions:\n  panel: { x: 0pt, y: 0pt, width: 100pt, height: 100pt }\nelements:\n  - id: panel\n    type: group\n    x: 10pt\n    y: 10pt\n    width: 100pt\n    height: 100pt\n    collision: false\n    transform: { translate_y: 20pt }\n    children:\n      - { id: obstacle, type: rect, region: panel, width: 50pt, height: 90pt, collision: { policy: push } }\n      - { id: next, type: rect, region: panel, width: 50pt, height: 20pt, collision: { policy: next_page } }\n");
    assert!(scene.pages[0]
        .elements
        .iter()
        .any(|e| e.id.as_str() == "obstacle"));
    assert_eq!(scene.pages.len(), 2);
    assert_eq!(
        scene.pages[1].elements[0].bounds.visual.origin.y,
        Unit::points(30).unwrap()
    );
}

#[test]
fn audit_detects_a_reflowed_body_that_reenters_the_margin() {
    let mut scene = resolve(include_str!("../wiki/audit/next-page-margin.yml"));
    let element = &mut scene.pages[1].elements[0];
    element.bounds.layout.origin.y = Unit::ZERO;
    element.bounds.collision.origin.y = Unit::ZERO;
    element.bounds.visual.origin.y = Unit::ZERO;
    assert_eq!(audit(&scene).overflow_count, 1);
}
