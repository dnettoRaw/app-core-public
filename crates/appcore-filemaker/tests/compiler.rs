// =============================================================================
//        #######
//     ###       ###     F: compiler.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use appcore_filemaker::*;

    #[test]
    fn expands_sandboxed_include_and_component() {
        let root = br"filemaker: '1.0'
model: document
id: root
includes:
  - path: fragment.yaml
    namespace: extra
components:
  label:
    props: { value: Default }
    elements:
      - { id: text, type: text, text: '{{value}}' }
elements:
  - { id: greeting, type: group, component: label, props: { value: Hello } }
";
        let fragment = br"filemaker: '1.0'
model: document
id: fragment
elements:
  - { id: box, type: rect, width: 10mm, height: 10mm }
";
        let mut resolver = MemoryResolver::default();
        resolver
            .insert("fragment.yaml", "application/yaml", fragment.to_vec())
            .unwrap();
        let compiler = Compiler::builder()
            .template_resolver(Arc::new(resolver))
            .build()
            .unwrap();
        let ir = compiler.compile_template_yaml(root).unwrap();
        assert_eq!(ir.elements[0].children[0].text.as_deref(), Some("Hello"));
        assert_eq!(ir.elements[1].id.as_str(), "extra/box");
    }

    #[test]
    fn patch_transaction_rolls_back_after_locked_failure() {
        let yaml = br"filemaker: '1.0'
model: document
id: root
elements:
  - { id: open, type: text, text: before }
  - { id: locked, type: text, text: fixed, locked: true }
";
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml).unwrap();
        let mut document = compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
            .unwrap();
        let original = document.clone();
        let patch = Patch {
            sequence: 7,
            operations: vec![
                appcore_filemaker::PatchOperation::SetText {
                    id: appcore_filemaker::ElementId::new("open").unwrap(),
                    text: "changed".to_owned(),
                },
                appcore_filemaker::PatchOperation::SetHidden {
                    id: appcore_filemaker::ElementId::new("locked").unwrap(),
                    hidden: true,
                },
            ],
        };
        let error = PatchTransaction::new(&mut document, 10)
            .apply(&patch)
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::PatchLocked);
        assert_eq!(document, original);
    }

    #[test]
    fn bind_bounds_patch_operations_across_the_complete_batch() {
        let limits = ResourceLimits {
            max_patch_operations: 1,
            ..ResourceLimits::default()
        };
        let compiler = Compiler::builder().limits(limits).build().unwrap();
        let template = compiler
            .compile_template_yaml(
                br"filemaker: '1.0'
model: canvas
id: bounded-patches
page: { width: 20pt, height: 20pt }
elements: [{ id: node, type: rect, width: 5pt, height: 5pt }]
",
            )
            .unwrap();
        let patches = [1, 2].map(|sequence| Patch {
            sequence,
            operations: vec![PatchOperation::SetHidden {
                id: ElementId::new("node").unwrap(),
                hidden: sequence == 1,
            }],
        });
        let error = compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &patches)
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::LimitExceeded);
    }

    #[test]
    fn destructive_patch_cannot_bypass_a_locked_descendant() {
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler
            .compile_template_yaml(
                br"filemaker: '1.0'
model: canvas
id: locked-subtree
page: { width: 20pt, height: 20pt }
elements:
  - id: parent
    type: group
    children:
      - { id: child, type: rect, width: 5pt, height: 5pt, locked: true }
",
            )
            .unwrap();
        let original = compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
            .unwrap();
        for operation in [
            PatchOperation::Remove {
                id: ElementId::new("parent").unwrap(),
            },
            PatchOperation::Replace {
                id: ElementId::new("parent").unwrap(),
                element: original.elements[0].clone(),
            },
        ] {
            let mut document = original.clone();
            let error = PatchTransaction::new(&mut document, 1)
                .apply(&Patch {
                    sequence: 1,
                    operations: vec![operation],
                })
                .unwrap_err();
            assert_eq!(error.code(), ErrorCode::PatchLocked);
            assert_eq!(document, original);
        }
    }

    #[test]
    fn move_and_resize_are_explicit_runtime_geometry_overrides() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: override
collision: false
page: { width: 100pt, height: 100pt }
guides: { edge: 20pt }
elements:
  - id: box
    type: rect
    width: 20pt
    height: 10pt
    align_x: center
    anchors: { top: 'guide:edge' }
    constraints: { min_width: 15pt, max_width: 25pt, aspect_ratio: 2000000 }
";
        let limits = ResourceLimits::default();
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml).unwrap();
        let mut document = compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
            .unwrap();
        PatchTransaction::new(&mut document, 2)
            .apply(&Patch {
                sequence: 1,
                operations: vec![
                    PatchOperation::Move {
                        id: ElementId::new("box").unwrap(),
                        x: "5pt".parse().unwrap(),
                        y: "6pt".parse().unwrap(),
                    },
                    PatchOperation::Resize {
                        id: ElementId::new("box").unwrap(),
                        width: "30pt".parse().unwrap(),
                        height: "40pt".parse().unwrap(),
                    },
                ],
            })
            .unwrap();
        let scene = LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        assert_eq!(
            scene.pages[0].elements[0].bounds.layout,
            Rect::new(
                Unit::points(5).unwrap(),
                Unit::points(6).unwrap(),
                Unit::points(30).unwrap(),
                Unit::points(40).unwrap(),
            )
            .unwrap()
        );
    }

    #[test]
    fn applies_explicit_theme_tokens_and_cascade_order() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: themed
page: { width: 20pt, height: 20pt }
theme: dark
themes:
  base:
    tokens: { foreground: '#111111', accent: '#224466' }
    style: { fill: '$foreground' }
  dark:
    extends: base
    tokens: { foreground: '#eeeeee' }
    style: { stroke: '$accent' }
style: { opacity: 900000 }
styles:
  highlighted: { fill: '$accent' }
elements:
  - id: box
    type: rect
    width: 10pt
    height: 10pt
    styles: [highlighted]
    style: { opacity: 800000 }
";
        let template = Compiler::builder()
            .build()
            .unwrap()
            .compile_template_yaml(yaml)
            .unwrap();
        let style = &template.elements[0].style;
        assert_eq!(
            style.fill,
            Some(appcore_filemaker::Color::Rgb {
                r: 34,
                g: 68,
                b: 102
            })
        );
        assert_eq!(
            style.stroke,
            Some(appcore_filemaker::Color::Rgb {
                r: 34,
                g: 68,
                b: 102
            })
        );
        assert_eq!(style.opacity, Some(800_000));
    }

    #[test]
    fn compiles_functional_and_typed_colors_without_losing_color_space() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: colors
page: { width: 20pt, height: 20pt }
themes:
  print:
    tokens: { ink: 'cmyk(1000000, 250000, 0, 100000)' }
theme: print
elements:
  - id: box
    type: rect
    width: 10pt
    height: 10pt
    style:
      fill: { space: gray, value: 128 }
      stroke: '$ink'
      color: 'rgba(1, 2, 3, 4)'
";
        let template = Compiler::builder()
            .build()
            .unwrap()
            .compile_template_yaml(yaml)
            .unwrap();
        let style = &template.elements[0].style;
        assert_eq!(style.fill, Some(Color::Gray { value: 128 }));
        assert_eq!(
            style.stroke,
            Some(Color::Cmyk {
                c: 1_000_000,
                m: 250_000,
                y: 0,
                k: 100_000,
            })
        );
        assert_eq!(
            style.color,
            Some(Color::Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            })
        );
    }

    #[test]
    fn rejects_out_of_range_functional_and_typed_colors() {
        for yaml in [
            br"filemaker: '1.0'
model: canvas
id: invalid-rgb
page: { width: 20pt, height: 20pt }
elements:
  - { id: box, type: rect, width: 10pt, height: 10pt, style: { fill: 'rgb(256, 0, 0)' } }
"
            .as_slice(),
            br"filemaker: '1.0'
model: canvas
id: invalid-cmyk
page: { width: 20pt, height: 20pt }
elements:
  - id: box
    type: rect
    width: 10pt
    height: 10pt
    style: { fill: { space: cmyk, c: 1000001, m: 0, y: 0, k: 0 } }
"
            .as_slice(),
        ] {
            let error = Compiler::builder()
                .build()
                .unwrap()
                .compile_template_yaml(yaml)
                .unwrap_err();
            assert_eq!(error.code(), ErrorCode::SchemaField);
        }
    }

    #[test]
    fn style_cascade_reaches_data_runtime_and_paint_only_export_layers() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: full-cascade
collision: false
page: { width: 20pt, height: 20pt }
themes:
  base:
    tokens: { alert: 'gray(128)' }
    style: { fill: red, stroke: black }
theme: base
style: { opacity: 900000 }
components:
  card:
    elements:
      - id: body
        type: rect
        width: 10pt
        height: 10pt
        style: { fill: blue }
        style_rules:
          - { when: 'data.highlight == true', style: { fill: '$alert' } }
elements:
  - { id: card, type: group, component: card, width: 20pt, height: 20pt }
";
        let limits = ResourceLimits::default();
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml).unwrap();
        let compiled = &template.elements[0].children[0];
        assert_eq!(compiled.style.fill, Some(Color::parse("blue").unwrap()));
        assert_eq!(compiled.style_rules.len(), 1);

        let data = DataValue::Object(BTreeMap::from([(
            "highlight".to_owned(),
            DataValue::Boolean(true),
        )]));
        let mut document = compiler.bind(&template, &data, &[]).unwrap();
        let body_id = ElementId::new("card/body").unwrap();
        let data_style = &document.elements[0].children[0].style;
        assert_eq!(data_style.fill, Some(Color::Gray { value: 128 }));
        assert!(document.elements[0].children[0].style_rules.is_empty());

        PatchTransaction::new(&mut document, 1)
            .apply(&Patch {
                sequence: 1,
                operations: vec![PatchOperation::SetStyle {
                    id: body_id,
                    style: Style {
                        fill: Some(Color::parse("green").unwrap()),
                        ..Style::default()
                    },
                }],
            })
            .unwrap();
        let scene = LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        let body = scene
            .pages
            .iter()
            .flat_map(|page| &page.elements)
            .find(|element| element.id.as_str() == "card/body")
            .unwrap();
        assert_eq!(body.style.fill, Some(Color::parse("green").unwrap()));

        let mut svg = Vec::new();
        export(
            &scene,
            &ExportRequest {
                style_override: Some(ExportStyleOverride {
                    fill: Some(Color::Rgb { r: 9, g: 8, b: 7 }),
                    ..ExportStyleOverride::default()
                }),
                ..ExportRequest::default()
            },
            &ExportContext {
                limits: &limits,
                fonts: &FontManager::default(),
                assets: None,
            },
            &mut svg,
        )
        .unwrap();
        assert!(String::from_utf8(svg).unwrap().contains("fill=\"#090807\""));
        assert_eq!(body.style.fill, Some(Color::parse("green").unwrap()));
    }

    #[test]
    fn invalid_runtime_style_rolls_back_the_complete_patch() {
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler
            .compile_template_yaml(
                br"filemaker: '1.0'
model: canvas
id: rollback-style
page: { width: 20pt, height: 20pt }
elements:
  - { id: box, type: rect, width: 10pt, height: 10pt }
",
            )
            .unwrap();
        let mut document = compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
            .unwrap();
        let original = document.clone();
        let error = PatchTransaction::new(&mut document, 2)
            .apply(&Patch {
                sequence: 2,
                operations: vec![
                    PatchOperation::SetHidden {
                        id: ElementId::new("box").unwrap(),
                        hidden: true,
                    },
                    PatchOperation::SetStyle {
                        id: ElementId::new("box").unwrap(),
                        style: Style {
                            opacity: Some(1_000_001),
                            ..Style::default()
                        },
                    },
                ],
            })
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::SchemaField);
        assert_eq!(document, original);
    }

    #[test]
    fn rejects_theme_inheritance_cycles() {
        let yaml = br"filemaker: '1.0'
model: canvas
id: themed
page: { width: 20pt, height: 20pt }
theme: a
themes:
  a: { extends: b }
  b: { extends: a }
";
        let error = Compiler::builder()
            .build()
            .unwrap()
            .compile_template_yaml(yaml)
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::DataCycle);
    }
}
