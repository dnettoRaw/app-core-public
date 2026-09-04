// =============================================================================
//        #######
//     ###       ###     F: canvas.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeMap;

use appcore_filemaker::{
    Compiler, DataValue, ElementKind, ErrorCode, FontManager, LayoutEngine, LayoutOptions,
    PathCommand, ResourceLimits, Shape, Unit,
};

const VECTOR_CANVAS: &[u8] = br"filemaker: '1.0'
model: canvas
id: semantic-canvas
collision: false
page:
  width: 120pt
  height: 80pt
  safe: { top: 5pt, right: 6pt, bottom: 7pt, left: 8pt }
elements:
  - { id: line, type: line, x: 10px, y: 2lu, width: 10mm, height: 5pt, layer: marks, z_index: 2 }
  - { id: rect, type: rect, x: 10%, y: 10pt, width: 20%, height: 8pt }
  - { id: circle, type: circle, x: 0.5normalized, y: 20pt, width: 10pt, height: 10pt }
  - { id: ellipse, type: ellipse, x: 70pt, y: 20pt, width: 20pt, height: 10pt }
  - id: polygon
    type: polygon
    x: 10pt
    y: 40pt
    width: 20pt
    height: 10pt
    path:
      - { command: move, x: 0norm, y: 1norm }
      - { command: line, x: 0.5norm, y: 0norm }
      - { command: line, x: 1norm, y: 1norm }
      - { command: close }
  - id: curve
    type: path
    x: 40pt
    y: 40pt
    width: 30pt
    height: 10pt
    transform: { rotate: 15, translate_x: 1pt }
    path:
      - { command: move, x: 0norm, y: 1norm }
      - { command: curve, x1: 0.25norm, y1: 0norm, x2: 0.75norm, y2: 0norm, x: 1norm, y: 1norm }
      - { command: close }
  - id: group
    type: group
    x: 80pt
    y: 40pt
    width: 20pt
    height: 20pt
    children:
      - { id: child, type: rect, x: 2pt, y: 3pt, width: 4pt, height: 5pt }
";

#[test]
fn canvas_resolves_units_primitives_paths_safe_area_and_transforms() {
    let scene = compile_scene(VECTOR_CANVAS).unwrap();
    let page = &scene.pages[0];
    let find = |id: &str| {
        page.elements
            .iter()
            .find(|element| element.id.as_str() == id)
            .unwrap()
    };

    assert_eq!(find("line").kind, ElementKind::Line);
    assert!(matches!(find("line").shape, Shape::Path { .. }));
    assert!(matches!(find("rect").shape, Shape::Rect { .. }));
    assert!(matches!(find("circle").shape, Shape::Ellipse { .. }));
    assert!(matches!(find("ellipse").shape, Shape::Ellipse { .. }));
    assert!(matches!(find("group").shape, Shape::Rect { .. }));
    assert!(matches!(find("child").shape, Shape::Rect { .. }));

    assert_eq!(
        find("line").bounds.layout.origin.x,
        Unit::from_raw(7_500_000)
    );
    assert_eq!(
        find("line").bounds.layout.origin.y,
        Unit::points(2).unwrap()
    );
    assert_eq!(
        find("rect").bounds.layout.origin.x,
        Unit::points(12).unwrap()
    );
    assert_eq!(
        find("rect").bounds.layout.size.width,
        Unit::points(24).unwrap()
    );
    assert_eq!(
        find("circle").bounds.layout.origin.x,
        Unit::points(60).unwrap()
    );
    assert!(!find("curve").transform.is_identity());
    assert_eq!(find("line").layer, "marks");
    assert_eq!(find("line").z_index, 2);
    assert!(!find("line").collidable);

    let Shape::Polygon { points } = &find("polygon").shape else {
        panic!("polygon must remain semantic vector geometry");
    };
    assert_eq!(points.len(), 3);
    let Shape::Path { commands, .. } = &find("curve").shape else {
        panic!("path must remain semantic vector geometry");
    };
    assert!(matches!(commands[1], PathCommand::Curve { .. }));
    assert!(matches!(commands[2], PathCommand::Close));

    let safe = page.page_template.as_ref().unwrap().safe_bounds().unwrap();
    assert_eq!(safe.origin.x, Unit::points(8).unwrap());
    assert_eq!(safe.origin.y, Unit::points(5).unwrap());
    assert_eq!(safe.size.width, Unit::points(106).unwrap());
    assert_eq!(safe.size.height, Unit::points(68).unwrap());
}

#[test]
fn circle_rejects_ellipse_geometry() {
    let yaml = br"filemaker: '1.0'
model: canvas
id: invalid-circle
page: { width: 100pt, height: 100pt }
elements:
  - { id: oval, type: circle, width: 20pt, height: 10pt }
";
    let error = compile_scene(yaml).unwrap_err();
    assert_eq!(error.code(), ErrorCode::LayoutInvalid);
    assert!(error.to_string().contains("circle requires equal"));
}

fn compile_scene(yaml: &[u8]) -> appcore_filemaker::Result<appcore_filemaker::ResolvedScene> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(yaml)?;
    let document = compiler.bind(&template, &DataValue::Object(BTreeMap::new()), &[])?;
    LayoutEngine::new(&limits, &FontManager::default(), LayoutOptions::default())?
        .resolve(&document)
}
