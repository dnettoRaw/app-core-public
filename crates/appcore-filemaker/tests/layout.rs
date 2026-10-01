// =============================================================================
//        #######
//     ###       ###     F: layout.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use appcore_filemaker::*;

    #[test]
    fn text_block_padding_reduces_measurement_and_insets_export_geometry() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: text-block-padding
collision: false
page: { width: 120pt, height: 80pt }
elements:
  - id: padded
    type: text
    text: 'one two three four five six'
    x: 10pt
    y: 10pt
    width: 60pt
    height: 36pt
    collision: false
    style: { font: Body, font_size: 9pt }
    text_options:
      overflow: wrap
      align_x: end
      padding: { top: 4pt, right: 5pt, bottom: 6pt, left: 7pt }
";
        let limits = ResourceLimits::default();
        let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
        let template = compiler.compile_template_yaml(yaml).unwrap();
        let document = compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
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
        let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        let element = &scene.pages[0].elements[0];
        let content = element
            .text_layout
            .as_ref()
            .unwrap()
            .content_bounds(element.bounds.layout)
            .unwrap();
        assert_eq!(content.origin.x, Unit::points(17).unwrap());
        assert_eq!(content.origin.y, Unit::points(14).unwrap());
        assert_eq!(content.size.width, Unit::points(48).unwrap());
        assert_eq!(content.size.height, Unit::points(26).unwrap());
        assert!(element.text_layout.as_ref().unwrap().lines.len() > 1);
        assert_eq!(
            element.text_layout.as_ref().unwrap().padding.right,
            Unit::points(5).unwrap()
        );
        let mut serialized = serde_json::to_value(element.text_layout.as_ref().unwrap()).unwrap();
        serialized.as_object_mut().unwrap().remove("padding");
        let legacy_layout: TextLayout = serde_json::from_value(serialized).unwrap();
        assert_eq!(legacy_layout.padding, Insets::default());
        for format in [
            ExportFormat::Pdf,
            ExportFormat::Svg,
            ExportFormat::Png,
            ExportFormat::Html,
        ] {
            let (bytes, _) = export_bytes(
                &scene,
                &ExportRequest {
                    format,
                    ..ExportRequest::default()
                },
                &ExportContext {
                    limits: &limits,
                    fonts: &fonts,
                    assets: None,
                },
            )
            .unwrap();
            assert!(!bytes.is_empty(), "{format:?} export must write content");
            if format == ExportFormat::Svg {
                let svg = std::str::from_utf8(&bytes).unwrap();
                assert!(svg.contains("y=\"23.000000\""));
            }
            if format == ExportFormat::Html {
                let html = std::str::from_utf8(&bytes).unwrap();
                assert!(html.contains("padding:4.000000pt 5.000000pt 6.000000pt 7.000000pt"));
            }
        }
    }

    #[test]
    fn vertical_keep_with_next_moves_a_fitting_block_together() {
        let yaml = br"filemaker: '1.0'
model: document
id: keep-block
collision: false
page: { width: 100pt, height: 100pt }
elements:
  - id: flow
    type: group
    width: 80pt
    height: 100pt
    layout: flow_vertical
    collision: false
    children:
      - { id: lead, type: rect, width: 50pt, height: 85pt, collision: false }
      - { id: block-a, type: rect, width: 50pt, height: 8pt, keep_with_next: true, collision: false }
      - { id: block-b, type: rect, width: 50pt, height: 8pt, collision: false }
";
        let scene = compile_scene(yaml).unwrap();
        let page_for = |id: &str| {
            scene
                .pages
                .iter()
                .position(|page| {
                    page.elements
                        .iter()
                        .any(|element| element.id.as_str() == id)
                })
                .unwrap()
        };
        assert_eq!(page_for("block-a"), page_for("block-b"));
        assert_eq!(page_for("block-a"), 1);
        assert_eq!(scene.pages[1].elements.len(), 2);
    }

    #[test]
    fn oversized_keep_block_splits_between_elements_without_losing_content() {
        let yaml = br"filemaker: '1.0'
model: document
id: split-keep-block
collision: false
page: { width: 100pt, height: 40pt }
elements:
  - id: flow
    type: group
    width: 80pt
    height: 40pt
    layout: flow_vertical
    collision: false
    children:
      - { id: segment-a, type: rect, width: 50pt, height: 20pt, keep_with_next: true, collision: false }
      - { id: segment-b, type: rect, width: 50pt, height: 20pt, keep_with_next: true, collision: false }
      - { id: segment-c, type: rect, width: 50pt, height: 20pt, collision: false }
";
        let scene = compile_scene(yaml).unwrap();
        let pages: Vec<_> = scene
            .pages
            .iter()
            .filter(|page| !page.elements.is_empty())
            .collect();
        assert!(pages.len() >= 2);
        for id in ["segment-a", "segment-b", "segment-c"] {
            assert_eq!(
                scene
                    .pages
                    .iter()
                    .flat_map(|page| &page.elements)
                    .filter(|element| element.id.as_str() == id)
                    .count(),
                1,
                "segment {id} must appear exactly once"
            );
        }
    }

    #[test]
    fn keep_with_next_rejects_an_unpaired_or_nonvertical_declaration() {
        let orphan = br"filemaker: '1.0'
model: canvas
id: keep-orphan
page: { width: 100pt, height: 100pt }
elements: [{ id: only, type: rect, width: 10pt, height: 10pt, keep_with_next: true }]
";
        assert_eq!(
            compile_scene(orphan).unwrap_err().code(),
            ErrorCode::LayoutInvalid
        );
        let absolute = br"filemaker: '1.0'
model: canvas
id: keep-absolute
page: { width: 100pt, height: 100pt }
elements:
  - { id: first, type: rect, width: 10pt, height: 10pt, keep_with_next: true }
  - { id: next, type: rect, width: 10pt, height: 10pt }
";
        assert_eq!(
            compile_scene(absolute).unwrap_err().code(),
            ErrorCode::LayoutInvalid
        );
    }

    #[test]
    fn single_element_taller_than_page_fails_before_creating_blank_continuations() {
        let yaml = br"filemaker: '1.0'
model: document
id: oversized-element
collision: false
page: { width: 100pt, height: 40pt }
elements: [{ id: too-tall, type: rect, width: 20pt, height: 41pt, collision: false }]
";
        let error = compile_scene(yaml).unwrap_err();
        assert_eq!(error.code(), ErrorCode::LayoutInvalid);
        assert!(error.message().contains("exceeds one physical page"));
    }

    #[test]
    fn expanded_text_flows_across_pages_without_losing_or_repeating_lines() {
        let scene = expanded_text_scene();
        assert_eq!(scene.pages.len(), 3);
        let fragments: Vec<_> = scene
            .pages
            .iter()
            .flat_map(|page| {
                page.elements
                    .iter()
                    .filter(|element| element.id.as_str() == "long-description")
            })
            .collect();
        assert_eq!(fragments.len(), 3);
        assert_eq!(
            fragments
                .iter()
                .map(|element| element.text.as_deref().unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n"),
            "日本語\n日本語\n日本語\n日本語\n日本語\n日本語\n日本語\n日本語\n日本語\n日本語\n日本語\n日本語"
        );
        assert_eq!(
            fragments
                .iter()
                .map(|element| element.text_layout.as_ref().unwrap().lines.len())
                .collect::<Vec<_>>(),
            [4, 4, 4]
        );
        assert!(fragments.iter().all(|element| {
            let layout = element.text_layout.as_ref().unwrap();
            layout.padding_inline == Unit::points(2).unwrap()
                && layout.padding.top == Unit::points(2).unwrap()
                && layout.padding.bottom == Unit::points(3).unwrap()
                && !layout.diagnostics.contains(&TextDiagnostic::Clipped)
                && element.bounds.layout.bottom().unwrap() <= Unit::points(70).unwrap()
        }));
        let last_page = scene
            .pages
            .iter()
            .position(|page| {
                page.elements
                    .iter()
                    .any(|element| element.id.as_str() == "following-note")
            })
            .unwrap();
        assert_eq!(last_page, 2);
        let following = scene.pages[last_page]
            .elements
            .iter()
            .find(|element| element.id.as_str() == "following-note")
            .unwrap();
        assert_eq!(following.bounds.layout.origin.y, Unit::points(63).unwrap());
    }

    fn expanded_text_scene() -> ResolvedScene {
        let yaml = r#"filemaker: '1.0'
model: document
id: multipage-text
collision: false
page: { width: 100pt, height: 80pt }
elements:
  - id: text-flow
    type: group
    x: 10pt
    y: 10pt
    width: 80pt
    height: 60pt
    layout: flow_vertical
    collision: false
    children:
      - id: long-description
        type: text
        width: 80pt
        height: auto
        text: |-
          日本語
          日本語
          日本語
          日本語
          日本語
          日本語
          日本語
          日本語
          日本語
          日本語
          日本語
          日本語
        style: { font: Japanese, font_size: 10pt }
        text_options: { overflow: expand, padding_inline: 2pt, padding: { top: 2pt, bottom: 3pt } }
        collision: false
      - { id: following-note, type: rect, width: 40pt, height: 5pt, collision: false }
"#;
        let limits = ResourceLimits::default();
        let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let document = compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(
                FontAsset::new(
                    "Japanese",
                    include_bytes!("assets/NotoSansJP-Test.ttf").to_vec(),
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

    #[test]
    fn collision_push_is_geometry_first_and_bounded() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: collision
page: { width: 100pt, height: 100pt }
elements:
  - { id: first, type: rect, x: 0pt, y: 0pt, width: 20pt, height: 20pt }
  - { id: second, type: rect, x: 0pt, y: 0pt, width: 20pt, height: 20pt }
";
        let limits = ResourceLimits::default();
        let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
        let template = compiler.compile_template_yaml(yaml).unwrap();
        let document = compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
            .unwrap();
        let scene = LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        assert_eq!(
            scene.pages[0].elements[1].bounds.layout.origin.y,
            Unit::points(20).unwrap()
        );
    }

    #[test]
    fn paint_layer_and_z_index_do_not_change_collision_order() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: independent-ordering
page: { width: 100pt, height: 100pt }
elements:
  - { id: geometry-first, type: rect, x: 0pt, y: 0pt, width: 10pt, height: 10pt, layer: zzz, z_index: 100 }
  - { id: paint-first, type: rect, x: 0pt, y: 0pt, width: 10pt, height: 10pt, layer: aaa, z_index: -100 }
";
        let scene = compile_scene(yaml).unwrap();
        let elements = &scene.pages[0].elements;
        assert_eq!(elements[0].id.as_str(), "paint-first");
        assert_eq!(elements[1].id.as_str(), "geometry-first");
        assert_eq!(elements[1].bounds.layout.origin.y, Unit::ZERO);
        assert_eq!(
            elements[0].bounds.layout.origin.y,
            Unit::points(10).unwrap()
        );
    }

    #[test]
    fn detects_anchor_cycles_before_layout() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: anchors
page: { width: 100pt, height: 100pt }
elements:
  - { id: a, type: rect, width: 10pt, height: 10pt, anchors: { left: b.right } }
  - { id: b, type: rect, width: 10pt, height: 10pt, anchors: { left: a.right } }
";
        let limits = ResourceLimits::default();
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml).unwrap();
        let document = compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
            .unwrap();
        let error = LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::LayoutNonConvergent);
    }

    #[test]
    fn constraints_alignment_aspect_and_guides_resolve_before_collision() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: constrained
collision: false
page: { width: 100pt, height: 80pt }
guides: { middle: 50%, top: 10pt }
elements:
  - id: preferred
    type: rect
    constraints:
      min_width: 30pt
      preferred_width: 40pt
      max_width: 50pt
      preferred_height: 20pt
    align_x: center
    align_y: end
  - id: ratio
    type: rect
    height: 20pt
    constraints: { aspect_ratio: 2000000 }
    align_x: end
  - id: guided
    type: rect
    width: 10pt
    height: 10pt
    anchors: { left: 'guide:middle+2pt', top: 'guide:top' }
";
        let scene = compile_scene(yaml).unwrap();
        let elements = &scene.pages[0].elements;
        assert_eq!(
            elements[0].bounds.layout,
            Rect::new(
                Unit::points(30).unwrap(),
                Unit::points(60).unwrap(),
                Unit::points(40).unwrap(),
                Unit::points(20).unwrap(),
            )
            .unwrap()
        );
        assert_eq!(
            elements[1].bounds.layout.size.width,
            Unit::points(40).unwrap()
        );
        assert_eq!(
            elements[1].bounds.layout.origin.x,
            Unit::points(60).unwrap()
        );
        assert_eq!(
            elements[2].bounds.layout.origin,
            Point {
                x: Unit::points(52).unwrap(),
                y: Unit::points(10).unwrap(),
            }
        );
    }

    #[test]
    fn invalid_or_conflicting_constraints_fail_explicitly() {
        let invalid_range = br"filemaker: '1.0'
model: canvas
id: invalid-range
page: { width: 100pt, height: 80pt }
elements:
  - { id: box, type: rect, constraints: { min_width: 60pt, max_width: 40pt } }
";
        assert_eq!(
            compile_scene(invalid_range).unwrap_err().code(),
            ErrorCode::LayoutInvalid
        );
        let invalid_ratio = br"filemaker: '1.0'
model: canvas
id: invalid-ratio
page: { width: 100pt, height: 80pt }
elements:
  - { id: box, type: rect, width: 30pt, height: 20pt, constraints: { aspect_ratio: 2000000 } }
";
        assert_eq!(
            compile_scene(invalid_ratio).unwrap_err().code(),
            ErrorCode::LayoutInvalid
        );
    }

    #[test]
    fn flow_distribution_uses_resolved_fixed_sizes() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: distribution
collision: false
page: { width: 120pt, height: 120pt }
elements:
  - id: vertical
    type: group
    width: 20pt
    height: 100pt
    layout: flow_vertical
    distribute: space_between
    children:
      - { id: top, type: rect, width: 20pt, height: 10pt }
      - { id: bottom, type: rect, width: 20pt, height: 10pt }
  - id: horizontal
    type: group
    y: 100pt
    width: 100pt
    height: 20pt
    layout: flow_horizontal
    distribute: center
    gap: 10pt
    children:
      - { id: left, type: rect, width: 10pt, height: 20pt }
      - { id: right, type: rect, width: 10pt, height: 20pt }
";
        let scene = compile_scene(yaml).unwrap();
        let by_id = |id: &str| {
            scene.pages[0]
                .elements
                .iter()
                .find(|element| element.id.as_str() == id)
                .unwrap()
                .bounds
                .layout
        };
        assert_eq!(by_id("top").origin.y, Unit::ZERO);
        assert_eq!(by_id("bottom").origin.y, Unit::points(90).unwrap());
        assert_eq!(by_id("left").origin.x, Unit::points(35).unwrap());
        assert_eq!(by_id("right").origin.x, Unit::points(55).unwrap());
    }

    #[test]
    fn distributed_flow_rejects_auto_primary_measurement() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: distribution-auto
page: { width: 100pt, height: 100pt }
elements:
  - id: flow
    type: group
    width: 100pt
    height: 100pt
    layout: flow_vertical
    distribute: center
    children:
      - { id: auto, type: rect, width: 10pt }
";
        assert_eq!(
            compile_scene(yaml).unwrap_err().code(),
            ErrorCode::LayoutInvalid
        );
    }

    #[test]
    fn page_margin_safe_bleed_and_crop_survive_resolution() {
        let yaml = br"filemaker: '1.0'
model: document
id: page-boxes
page:
  width: 100pt
  height: 80pt
  margin: { top: 10pt, right: 10pt, bottom: 10pt, left: 10pt }
  safe: { top: 5pt, right: 5pt, bottom: 5pt, left: 5pt }
  bleed: { top: 3pt, right: 3pt, bottom: 3pt, left: 3pt }
  crop_marks: true
elements:
  - { id: content, type: rect, width: 100%, height: 10pt }
";
        let limits = ResourceLimits::default();
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml).unwrap();
        let document = compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
            .unwrap();
        let scene = LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        let page = &scene.pages[0];
        assert_eq!(
            page.elements[0].bounds.layout.origin.x,
            Unit::points(10).unwrap()
        );
        assert_eq!(
            page.elements[0].bounds.layout.size.width,
            Unit::points(80).unwrap()
        );
        assert!(page.page_template.as_ref().unwrap().crop_marks);
        assert_eq!(
            page.page_template
                .as_ref()
                .unwrap()
                .safe_bounds()
                .unwrap()
                .origin
                .x,
            Unit::points(5).unwrap()
        );
    }

    #[test]
    fn collision_false_is_inherited_from_document_and_page() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: disabled-collision
collision: { policy: error }
page:
  width: 100pt
  height: 100pt
  collision: false
elements:
  - { id: first, type: rect, x: 0pt, y: 0pt, width: 20pt, height: 20pt }
  - { id: second, type: rect, x: 0pt, y: 0pt, width: 20pt, height: 20pt }
";
        let scene = compile_scene(yaml).unwrap();
        assert_eq!(
            scene.pages[0].elements[0].bounds.layout.origin.y,
            Unit::ZERO
        );
        assert_eq!(
            scene.pages[0].elements[1].bounds.layout.origin.y,
            Unit::ZERO
        );
    }

    #[test]
    fn region_and_element_collision_override_in_inheritance_order() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: collision-inheritance
collision: false
page: { width: 100pt, height: 100pt }
regions:
  active:
    x: 0pt
    y: 0pt
    width: 50pt
    height: 50pt
    collision: true
elements:
  - { id: first, type: rect, region: active, width: 20pt, height: 20pt }
  - { id: second, type: rect, region: active, width: 20pt, height: 20pt }
  - id: third
    type: rect
    region: active
    width: 20pt
    height: 20pt
    collision: false
";
        let scene = compile_scene(yaml).unwrap();
        let elements = &scene.pages[0].elements;
        assert_eq!(elements[0].bounds.layout.origin.y, Unit::ZERO);
        assert_eq!(
            elements[1].bounds.layout.origin.y,
            Unit::points(20).unwrap()
        );
        assert_eq!(elements[2].bounds.layout.origin.y, Unit::ZERO);
    }

    #[test]
    fn visual_collision_bounds_are_measured_before_spatial_query() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: visual-collision
collision: { bounds: visual, policy: push }
page: { width: 100pt, height: 100pt }
elements:
  - id: first
    type: rect
    x: 0pt
    y: 0pt
    width: 10pt
    height: 10pt
    style: { stroke: '#000000', stroke_width: 4pt }
  - id: second
    type: rect
    x: 11pt
    y: 0pt
    width: 10pt
    height: 10pt
    style: { stroke: '#000000', stroke_width: 4pt }
";
        let scene = compile_scene(yaml).unwrap();
        let second = &scene.pages[0].elements[1];
        assert_eq!(second.bounds.collision.origin.y, Unit::points(12).unwrap());
        assert_eq!(second.bounds.layout.origin.y, Unit::points(14).unwrap());
    }

    #[test]
    fn transforms_resolve_before_collision_and_compose_through_groups() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: transformed
collision: false
page: { width: 100pt, height: 100pt }
elements:
  - id: rotated
    type: rect
    x: 10pt
    y: 10pt
    width: 20pt
    height: 10pt
    transform:
      rotate: 90
      translate_x: 5pt
      origin_x: 0%
      origin_y: 0%
  - id: parent
    type: group
    x: 10pt
    y: 40pt
    width: 20pt
    height: 20pt
    transform: { translate_x: 5pt }
    children:
      - { id: child, type: rect, x: 0pt, y: 0pt, width: 5pt, height: 5pt }
  - id: mirrored
    type: rect
    x: 70pt
    y: 40pt
    width: 10pt
    height: 5pt
    transform: { mirror: horizontal }
  - id: rotated-parent
    type: group
    x: 40pt
    y: 40pt
    width: 20pt
    height: 20pt
    collision: false
    transform: { rotate: 90, origin_x: 0%, origin_y: 0% }
    children:
      - { id: rotated-first, type: rect, width: 5pt, height: 5pt, collision: true }
      - { id: rotated-second, type: rect, width: 5pt, height: 5pt, collision: true }
";
        let scene = compile_scene(yaml).unwrap();
        let rotated = &scene.pages[0].elements[0];
        assert_eq!(
            rotated.bounds.collision,
            Rect::new(
                Unit::points(5).unwrap(),
                Unit::points(10).unwrap(),
                Unit::points(10).unwrap(),
                Unit::points(20).unwrap(),
            )
            .unwrap()
        );
        let child = scene.pages[0]
            .elements
            .iter()
            .find(|element| element.id.as_str() == "child")
            .unwrap();
        assert_eq!(child.bounds.collision.origin.x, Unit::points(15).unwrap());
        assert_eq!(child.bounds.collision.origin.y, Unit::points(40).unwrap());
        let mirrored = scene.pages[0]
            .elements
            .iter()
            .find(|element| element.id.as_str() == "mirrored")
            .unwrap();
        assert_eq!(mirrored.transform.a, -1_000_000);
        assert_eq!(
            mirrored.bounds.collision.origin.x,
            Unit::points(70).unwrap()
        );
        let rotated_second = scene.pages[0]
            .elements
            .iter()
            .find(|element| element.id.as_str() == "rotated-second")
            .unwrap();
        assert_eq!(
            rotated_second.bounds.layout.origin.x,
            Unit::points(45).unwrap()
        );
        assert_eq!(
            rotated_second.bounds.collision.origin.y,
            Unit::points(45).unwrap()
        );
    }

    #[test]
    fn collision_groups_ignore_priority_and_movable_are_enforced() {
        let groups = br"filemaker: '1.0'
model: canvas
id: collision-groups
page: { width: 100pt, height: 100pt }
elements:
  - id: background
    type: rect
    width: 10pt
    height: 10pt
    collision: { group: background }
  - id: content
    type: rect
    width: 10pt
    height: 10pt
    collision: { group: content, collides_with: [content] }
  - id: ignored
    type: rect
    x: 20pt
    width: 10pt
    height: 10pt
    collision: { ignore: [ignored-base] }
  - id: ignored-base
    type: rect
    x: 20pt
    width: 10pt
    height: 10pt
";
        let scene = compile_scene(groups).unwrap();
        for id in ["background", "content", "ignored", "ignored-base"] {
            assert_eq!(
                scene.pages[0]
                    .elements
                    .iter()
                    .find(|element| element.id.as_str() == id)
                    .unwrap()
                    .bounds
                    .layout
                    .origin
                    .y,
                Unit::ZERO
            );
        }

        for collision in ["{ priority: 1 }", "{ movable: false }"] {
            let yaml = format!(
                "filemaker: '1.0'\nmodel: canvas\nid: blocked-candidate\npage: {{ width: 100pt, height: 100pt }}\nelements:\n  - {{ id: first, type: rect, width: 10pt, height: 10pt }}\n  - id: blocked\n    type: rect\n    width: 10pt\n    height: 10pt\n    collision: {collision}\n"
            );
            assert_eq!(
                compile_scene(yaml.as_bytes()).unwrap_err().code(),
                ErrorCode::LayoutInvalid
            );
        }
    }

    fn compile_scene(yaml: &[u8]) -> Result<ResolvedScene> {
        let limits = ResourceLimits::default();
        let compiler = Compiler::builder().limits(limits.clone()).build()?;
        let template = compiler.compile_template_yaml(yaml)?;
        let document = compiler.bind(&template, &DataValue::Object(BTreeMap::new()), &[])?;
        LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())?
            .resolve(&document)
    }
}
