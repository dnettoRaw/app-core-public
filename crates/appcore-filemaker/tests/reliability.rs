// =============================================================================
//        #######
//     ###       ###     F: reliability.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeMap;
use std::sync::Arc;

use appcore_filemaker::{
    export_controlled, preflight, validate_data, validate_layout, validate_template,
    CancellationToken, Compiler, DataValue, DocumentFingerprint, ElementId, ErrorCode,
    ExportContext, ExportFormat, ExportRequest, Fidelity, FontAsset, FontManager, LayoutEngine,
    LayoutOptions, OperationControl, Patch, PatchOperation, PreflightOptions, ProgressEvent,
    ProgressObserver, ProgressPhase, ResourceLimits, SceneCache, ValidationCode,
};

const OVERLAY_TEMPLATE: &[u8] = br"filemaker: '1.0'
model: canvas
id: reliability
page: { width: 100pt, height: 100pt }
elements:
  - id: first
    type: rect
    x: 10pt
    y: 10pt
    width: 30pt
    height: 30pt
    collision: { policy: overlay }
  - id: second
    type: rect
    x: 20pt
    y: 20pt
    width: 30pt
    height: 30pt
    collision: { policy: overlay }
";

#[test]
fn fingerprint_is_stable_and_data_sensitive() {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(OVERLAY_TEMPLATE).unwrap();
    let fonts = FontManager::default();
    let first = DocumentFingerprint::compute(
        &template,
        &DataValue::Object(BTreeMap::new()),
        None,
        &fonts,
        &limits,
    )
    .unwrap();
    let repeated = DocumentFingerprint::compute(
        &template,
        &DataValue::Object(BTreeMap::new()),
        None,
        &fonts,
        &limits,
    )
    .unwrap();
    let changed = DocumentFingerprint::compute(
        &template,
        &DataValue::Object(BTreeMap::from([(
            "value".to_owned(),
            DataValue::String("changed".to_owned()),
        )])),
        None,
        &fonts,
        &limits,
    )
    .unwrap();
    assert_eq!(first, repeated);
    assert_ne!(first, changed);
    assert_eq!(first.to_hex().len(), 64);

    let patched = DocumentFingerprint::compute_with_patches(
        &template,
        &DataValue::Object(BTreeMap::new()),
        &[Patch {
            sequence: 1,
            operations: vec![PatchOperation::SetHidden {
                id: ElementId::new("first").unwrap(),
                hidden: true,
            }],
        }],
        None,
        &fonts,
        &limits,
    )
    .unwrap();
    assert_ne!(first, patched);

    let mut added_image = template.elements[0].clone();
    added_image.id = ElementId::new("patched-image").unwrap();
    added_image.kind = appcore_filemaker::ElementKind::Image;
    added_image.asset = Some("patched.png".to_owned());
    let asset_patch = Patch {
        sequence: 2,
        operations: vec![PatchOperation::Add {
            parent: None,
            element: added_image,
        }],
    };
    assert!(DocumentFingerprint::compute_with_patches(
        &template,
        &DataValue::Object(BTreeMap::new()),
        &[asset_patch],
        None,
        &fonts,
        &limits,
    )
    .is_err());

    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default()).unwrap();
    let mut cache = SceneCache::new(2).unwrap();
    let cached = engine.resolve_cached(&document, first, &mut cache).unwrap();
    let repeated = engine.resolve_cached(&document, first, &mut cache).unwrap();
    assert!(std::sync::Arc::ptr_eq(&cached, &repeated));
}

#[test]
fn fingerprint_uses_the_configured_aggregate_output_budget() {
    let limits = ResourceLimits {
        max_output_bytes: 64,
        ..ResourceLimits::default()
    };
    let template = Compiler::builder()
        .build()
        .unwrap()
        .compile_template_yaml(OVERLAY_TEMPLATE)
        .unwrap();
    let error = DocumentFingerprint::compute(
        &template,
        &DataValue::Object(BTreeMap::new()),
        None,
        &FontManager::default(),
        &limits,
    )
    .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LimitExceeded);
    assert!(error.message().contains("aggregate byte limit"));
}

#[test]
fn fingerprint_includes_the_ordered_font_fallback_contract() {
    let limits = ResourceLimits::default();
    let template = Compiler::builder()
        .build()
        .unwrap()
        .compile_template_yaml(OVERLAY_TEMPLATE)
        .unwrap();
    let bytes = include_bytes!("../examples/assets/NotoSans-Regular.ttf").to_vec();
    let mut first = FontManager::default();
    first
        .register(FontAsset::new("Primary", bytes.clone(), 0).unwrap())
        .unwrap();
    first
        .register(FontAsset::new("Fallback", bytes.clone(), 0).unwrap())
        .unwrap();
    first
        .set_fallback(vec!["Primary".to_owned(), "Fallback".to_owned()])
        .unwrap();
    let mut reversed = FontManager::default();
    reversed
        .register(FontAsset::new("Primary", bytes.clone(), 0).unwrap())
        .unwrap();
    reversed
        .register(FontAsset::new("Fallback", bytes, 0).unwrap())
        .unwrap();
    reversed
        .set_fallback(vec!["Fallback".to_owned(), "Primary".to_owned()])
        .unwrap();

    let data = DataValue::Object(BTreeMap::new());
    let first = DocumentFingerprint::compute(&template, &data, None, &first, &limits).unwrap();
    let reversed =
        DocumentFingerprint::compute(&template, &data, None, &reversed, &limits).unwrap();
    assert_ne!(first, reversed);
}

#[test]
fn schema_and_data_validation_are_separate_first_class_reports() {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().build().unwrap();
    let mut template = compiler
        .compile_template_yaml(
            br"filemaker: '1.0'
model: canvas
id: validation-stages
data_schema:
  name: { type: string }
elements:
  - id: label
    type: text
    binding: data.name
    width: 20pt
    height: 10pt
",
        )
        .unwrap();
    template.elements[0].binding = Some("data.name()".to_owned());
    let schema = validate_template(&template, &limits);
    assert!(schema
        .issues
        .iter()
        .any(|issue| issue.code == ValidationCode::Binding));

    let data = validate_data(
        &template,
        &DataValue::Object(BTreeMap::from([(
            "name".to_owned(),
            DataValue::Boolean(true),
        )])),
        &limits,
    );
    assert!(data.has_errors());
    assert!(data
        .issues
        .iter()
        .any(|issue| issue.code == ValidationCode::Data));
}

#[test]
fn cancellation_stops_before_template_parse() {
    let token = CancellationToken::default();
    token.cancel();
    let compiler = Compiler::builder()
        .control(OperationControl::new(token))
        .build()
        .unwrap();
    assert_eq!(
        compiler
            .compile_template_yaml(OVERLAY_TEMPLATE)
            .unwrap_err()
            .code(),
        ErrorCode::Cancelled
    );
}

struct CancelDuringBinding(CancellationToken);

impl ProgressObserver for CancelDuringBinding {
    fn report(&self, event: &ProgressEvent) {
        if event.phase == ProgressPhase::BindElements {
            self.0.cancel();
        }
    }
}

struct CancelDuringExport(CancellationToken);

impl ProgressObserver for CancelDuringExport {
    fn report(&self, event: &ProgressEvent) {
        if event.phase == ProgressPhase::Export && event.completed == 1 {
            self.0.cancel();
        }
    }
}

#[test]
fn binding_has_a_global_element_budget_and_cooperative_progress() {
    let yaml = br"filemaker: '1.0'
model: canvas
id: bounded-binding
elements:
  - { id: first, type: rect, repeat: data.entries, width: 1pt, height: 1pt }
  - { id: second, type: rect, repeat: data.entries, width: 1pt, height: 1pt }
";
    let data = DataValue::Object(BTreeMap::from([(
        "entries".to_owned(),
        DataValue::Array(vec![DataValue::Null; 3]),
    )]));
    let limits = ResourceLimits {
        max_elements: 5,
        ..ResourceLimits::default()
    };
    let compiler = Compiler::builder().limits(limits).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    assert_eq!(
        compiler.bind(&template, &data, &[]).unwrap_err().code(),
        ErrorCode::LimitExceeded
    );

    let token = CancellationToken::default();
    let compiler = Compiler::builder()
        .control(
            OperationControl::new(token.clone())
                .with_observer(Arc::new(CancelDuringBinding(token))),
        )
        .build()
        .unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    assert_eq!(
        compiler.bind(&template, &data, &[]).unwrap_err().code(),
        ErrorCode::Cancelled
    );
}

#[test]
fn layout_collision_comparisons_are_bounded() {
    let limits = ResourceLimits {
        max_collision_comparisons: 1,
        ..ResourceLimits::default()
    };
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler
        .compile_template_yaml(
            br"filemaker: '1.0'
model: canvas
id: collision-budget
page: { width: 100pt, height: 20pt }
elements:
  - { id: a, type: rect, x: 0pt, y: 0pt, width: 10pt, height: 10pt }
  - { id: b, type: rect, x: 20pt, y: 0pt, width: 10pt, height: 10pt }
  - { id: c, type: rect, x: 40pt, y: 0pt, width: 10pt, height: 10pt }
",
        )
        .unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let error = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LimitExceeded);
}

#[test]
fn export_cancellation_prevents_partial_output() {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(OVERLAY_TEMPLATE).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    for format in [ExportFormat::Svg, ExportFormat::Html, ExportFormat::Pdf] {
        let token = CancellationToken::default();
        let control =
            OperationControl::new(token.clone()).with_observer(Arc::new(CancelDuringExport(token)));
        let mut bytes = Vec::new();
        let error = export_controlled(
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
            &control,
            &mut bytes,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Cancelled);
        assert!(bytes.is_empty());
    }
}

#[test]
fn diagnostic_truncation_is_visible_and_fails_closed() {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler
        .compile_template_yaml(
            &[
                OVERLAY_TEMPLATE,
                br"  - id: third
    type: rect
    x: 15pt
    y: 15pt
    width: 30pt
    height: 30pt
    collision: { policy: overlay }
",
            ]
            .concat(),
        )
        .unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    let report = validate_layout(&scene, &limits, 1, &OperationControl::default()).unwrap();
    assert_eq!(report.issues.len(), 1);
    assert!(report.truncated);
    assert_eq!(
        report.enforce(false).unwrap_err().code(),
        ErrorCode::Validation
    );
}

#[test]
fn preflight_retains_collision_warning_and_strict_rejects_it() {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(OVERLAY_TEMPLATE).unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let fonts = FontManager::default();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    let context = ExportContext {
        limits: &limits,
        fonts: &fonts,
        assets: None,
    };
    let request = ExportRequest {
        format: ExportFormat::Pdf,
        fidelity: Fidelity::Strict,
        ..ExportRequest::default()
    };
    let report = preflight(
        &scene,
        &request,
        &context,
        &PreflightOptions::default(),
        &OperationControl::default(),
    )
    .unwrap();
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == ValidationCode::Collision));
    assert!(report.has_warnings());
    let layout_report =
        validate_layout(&scene, &limits, 1_000, &OperationControl::default()).unwrap();
    assert!(layout_report
        .issues
        .iter()
        .any(|issue| issue.code == ValidationCode::Collision));
    let mask = appcore_filemaker::CollisionMask::derive(
        &scene,
        0,
        appcore_filemaker::MaskView::CollisionMask,
    )
    .unwrap()
    .to_json()
    .unwrap();
    assert_eq!(
        std::str::from_utf8(&mask).unwrap(),
        std::str::from_utf8(include_bytes!("snapshots/collision-mask.json"))
            .unwrap()
            .trim_end()
    );

    let error = preflight(
        &scene,
        &request,
        &context,
        &PreflightOptions {
            strict: true,
            ..PreflightOptions::default()
        },
        &OperationControl::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), ErrorCode::Validation);

    let accessibility = preflight(
        &scene,
        &request,
        &context,
        &PreflightOptions {
            require_accessibility: true,
            ..PreflightOptions::default()
        },
        &OperationControl::default(),
    )
    .unwrap();
    assert!(accessibility
        .issues
        .iter()
        .any(|issue| issue.code == ValidationCode::Accessibility));
}
#[test]
fn element_id_deserialization_preserves_constructor_validation() {
    let valid: appcore_filemaker::ElementId = serde_json::from_str("\"safe/id-1\"").unwrap();
    assert_eq!(valid.as_str(), "safe/id-1");
    assert!(serde_json::from_str::<appcore_filemaker::ElementId>("\"../unsafe?\"").is_err());
}
