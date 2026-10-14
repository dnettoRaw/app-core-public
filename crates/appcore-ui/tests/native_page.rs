// =============================================================================
//        #######
//     ###       ###     F: native_page.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ui::*;
fn page() -> UiNativePageV1 {
    UiNativePageV1 {
        version: 1,
        title: "Native example".into(),
        width: 400.0,
        height: 200.0,
        nodes: vec![UiNativeNodeV1::Text {
            id: "title".into(),
            rect: UiRectangle {
                x: 20.0,
                y: 20.0,
                width: 360.0,
                height: 30.0,
            },
            text: "Ready".into(),
        }],
    }
}
#[test]
fn native_page_rejects_duplicate_identity_unknown_version_and_nonfinite_geometry() {
    let mut duplicate = page();
    duplicate.nodes.push(duplicate.nodes[0].clone());
    assert!(matches!(
        UiNativePage::new(duplicate),
        Err(UiError::InvalidRequest)
    ));
    let mut unsupported = page();
    unsupported.version = 2;
    assert!(matches!(
        UiNativePage::new(unsupported),
        Err(UiError::UnsupportedVersion)
    ));
    let mut invalid = page();
    invalid.width = f64::NAN;
    assert!(matches!(
        UiNativePage::new(invalid),
        Err(UiError::InvalidRequest)
    ));
}
#[test]
fn native_page_bounds_content_controls_and_encoded_budget() {
    let mut invalid = page();
    invalid.nodes.push(UiNativeNodeV1::Button {
        id: "button".into(),
        rect: UiRectangle {
            x: 390.0,
            y: 20.0,
            width: 30.0,
            height: 30.0,
        },
        label: "Action".into(),
        action: "state.inspect".into(),
    });
    assert!(matches!(
        UiNativePage::new(invalid),
        Err(UiError::InvalidRequest)
    ));
    let mut too_large = page();
    too_large.nodes = (0..100)
        .map(|index| UiNativeNodeV1::Text {
            id: format!("text-{index}"),
            rect: UiRectangle {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            text: "a".repeat(4096),
        })
        .collect();
    assert!(matches!(
        UiNativePage::new(too_large),
        Err(UiError::PayloadTooLarge)
    ));
    let valid = UiNativePage::new(page()).unwrap();
    assert_eq!(valid.nodes().len(), 1);
}

#[test]
fn native_page_decode_versions_and_geometry_budget_fail_at_the_boundary() {
    assert!(matches!(
        UiNativePage::decode(br#"{"version":2,"new_schema":true}"#),
        Err(UiError::UnsupportedVersion)
    ));
    let mut oversized = page();
    oversized.width = 4096.0;
    oversized.height = 4096.0;
    assert!(matches!(
        UiNativePage::new(oversized),
        Err(UiError::InvalidRequest)
    ));
    assert!(matches!(UiNativePage::decode(br#"{"version":1,"title":"example","width":400,"height":200,"nodes":[],"principal":"forged"}"#), Err(UiError::InvalidFrame)));
}

#[test]
fn label_updates_are_immutable_bounded_and_do_not_change_bindings() {
    let original = UiNativePage::new(page()).unwrap();
    let updated = original.with_label("title", "Updated").unwrap();
    assert!(matches!(&original.nodes()[0],UiNativeNodeV1::Text{text,..} if text=="Ready"));
    assert!(matches!(&updated.nodes()[0],UiNativeNodeV1::Text{text,..} if text=="Updated"));
    assert!(matches!(
        original.with_label("unknown", "data"),
        Err(UiError::InvalidRequest)
    ));
    assert!(matches!(
        original.with_label("title", "bad\0label"),
        Err(UiError::InvalidRequest)
    ));
    assert!(matches!(
        original.with_label("title", "x".repeat(4097)),
        Err(UiError::PayloadTooLarge)
    ));
    let mut source = page();
    source.nodes = (0..61)
        .map(|index| {
            elements::text(
                format!("text-{index}"),
                UiRectangle {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 20.0,
                },
                "x".repeat(4096),
            )
        })
        .collect();
    source.nodes.push(elements::text(
        "tail",
        UiRectangle {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0,
        },
        "",
    ));
    let bounded = UiNativePage::new(source).unwrap();
    assert!(matches!(
        bounded.with_label("tail", "\\".repeat(4096)),
        Err(UiError::PayloadTooLarge)
    ));
}
#[test]
fn editable_control_types_ranges_and_options_fail_closed_before_rendering() {
    let rect = UiRectangle {
        x: 10.0,
        y: 10.0,
        width: 150.0,
        height: 30.0,
    };
    let mut source = page();
    source.nodes = vec![
        elements::input("input", rect, "value", "hint", "input.submit"),
        elements::toggle("toggle", rect, "Enabled", true, "toggle.change"),
        elements::select(
            "select",
            rect,
            vec!["A".into(), "B".into()],
            0,
            "select.change",
        ),
        elements::slider("slider", rect, 0.0, 100.0, 50.0, "slider.change"),
        elements::progress("progress", rect, 0.5),
    ];
    let valid = UiNativePage::new(source.clone()).unwrap();
    assert!(valid
        .with_value("input", UiNativeValue::Text("next".into()))
        .is_ok());
    assert!(valid
        .with_value("toggle", UiNativeValue::Boolean(false))
        .is_ok());
    assert!(valid
        .with_value("select", UiNativeValue::Selection(1))
        .is_ok());
    assert!(valid
        .with_value("slider", UiNativeValue::Number(100.0))
        .is_ok());
    for (id, value) in [
        ("select", UiNativeValue::Selection(2)),
        ("slider", UiNativeValue::Number(f64::NAN)),
        ("slider", UiNativeValue::Number(101.0)),
        ("progress", UiNativeValue::Number(-0.1)),
        ("input", UiNativeValue::Boolean(true)),
    ] {
        assert!(matches!(
            valid.with_value(id, value),
            Err(UiError::InvalidRequest)
        ));
    }
    source.nodes[2] = elements::select(
        "select",
        rect,
        vec!["A".into(), "A".into()],
        0,
        "select.change",
    );
    assert!(matches!(
        UiNativePage::new(source),
        Err(UiError::InvalidRequest)
    ));
}
