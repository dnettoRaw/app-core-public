// =============================================================================
//        #######
//     ###       ###     F: bridge.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::{BTreeMap, BTreeSet};

use appcore_ai::{AiGenerationOptions, AiToolCall};
use appcore_filemaker::{
    Compiler, DataValue, DocumentIr, ElementId, FontManager, Patch, PatchOperation, ResourceLimits,
};
use appcore_filemaker_ai::{tool_definitions, AiBridgePolicy, BridgeError, FileMakerAiSession};
use serde_json::json;

const TEMPLATE: &[u8] = br"filemaker: '1.0'
model: canvas
id: ai-bridge-test
page: { width: 100pt, height: 80pt }
ai:
  purpose: Exercise bounded editing
  editable: [box, curve, copy, transient]
  locked: [fixed]
elements:
  - id: box
    type: rect
    x: 10pt
    y: 10pt
    width: 20pt
    height: 20pt
    style: { fill: '#336699' }
  - id: fixed
    type: rect
    x: 50pt
    y: 10pt
    width: 20pt
    height: 20pt
    locked: true
";

fn document() -> DocumentIr {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler.compile_template_yaml(TEMPLATE).unwrap();
    assert!(template.ai_policy.editable.contains("box"));
    compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap()
}

fn session() -> FileMakerAiSession {
    FileMakerAiSession::new(
        document(),
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy::default(),
    )
    .unwrap()
}

struct CancelOnPhase {
    token: appcore_filemaker::CancellationToken,
    phase: appcore_filemaker::ProgressPhase,
}

impl appcore_filemaker::ProgressObserver for CancelOnPhase {
    fn report(&self, event: &appcore_filemaker::ProgressEvent) {
        if event.phase == self.phase {
            self.token.cancel();
        }
    }
}

fn cancel_control(phase: appcore_filemaker::ProgressPhase) -> appcore_filemaker::OperationControl {
    let token = appcore_filemaker::CancellationToken::default();
    appcore_filemaker::OperationControl::new(token.clone())
        .with_observer(std::sync::Arc::new(CancelOnPhase { token, phase }))
}

fn assert_cancelled(result: Result<appcore_filemaker_ai::ToolExecution, BridgeError>) {
    assert!(matches!(result, Err(BridgeError::Core(error))
        if error.code() == appcore_filemaker::ErrorCode::Cancelled));
}

#[test]
fn cancelled_cached_query_still_consumes_call_budget() {
    let token = appcore_filemaker::CancellationToken::default();
    token.cancel();
    let mut session = session().with_control(appcore_filemaker::OperationControl::new(token));
    assert_cancelled(session.execute("filemaker_inspect", r#"{"id":"box"}"#));
    session = session.with_control(appcore_filemaker::OperationControl::default());
    let result = session.execute("filemaker_capabilities", "{}").unwrap();
    assert_eq!(result.revision, 0);
    assert_eq!(result.value["calls_used"], 2);
    // A fresh control changes neither document ownership nor edit policy.
    assert!(matches!(
        session.execute("filemaker_remove", r#"{"id":"fixed"}"#),
        Err(BridgeError::Policy(_))
    ));
}

#[test]
fn cancellation_during_candidate_layout_preserves_document_and_scene() {
    let mut session = session();
    let before = session
        .execute("filemaker_inspect", r#"{"id":"box"}"#)
        .unwrap();
    session = session.with_control(cancel_control(appcore_filemaker::ProgressPhase::Layout));
    assert_cancelled(session.execute("filemaker_set", r#"{"id":"box","hidden":true}"#));
    session = session.with_control(appcore_filemaker::OperationControl::default());
    let after = session
        .execute("filemaker_inspect", r#"{"id":"box"}"#)
        .unwrap();
    assert_eq!(before.value, after.value);
    assert_eq!(before.revision, after.revision);
    let applied = session
        .execute("filemaker_set", r#"{"id":"box","hidden":true}"#)
        .unwrap();
    assert_eq!(applied.revision, 1);
}

#[test]
fn query_controls_reach_preflight_and_graphical_export() {
    for (phase, tool, args) in [
        (
            appcore_filemaker::ProgressPhase::Preflight,
            "filemaker_query_free_regions",
            "{}",
        ),
        (
            appcore_filemaker::ProgressPhase::Preflight,
            "filemaker_preflight",
            "{}",
        ),
        (
            appcore_filemaker::ProgressPhase::Export,
            "filemaker_export",
            r#"{"format":"svg"}"#,
        ),
    ] {
        let mut session = session().with_control(cancel_control(phase));
        assert_cancelled(session.execute(tool, args));
    }
}

#[test]
fn cancelled_initial_layout_keeps_session_empty() {
    let mut session = FileMakerAiSession::empty(
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy::default(),
    )
    .unwrap()
    .with_control(cancel_control(appcore_filemaker::ProgressPhase::Layout));
    let args = json!({"document": document()}).to_string();
    assert_cancelled(session.execute("filemaker_create", &args));
    session = session.with_control(appcore_filemaker::OperationControl::default());
    let result = session.execute("filemaker_capabilities", "{}").unwrap();
    assert!(result.value["document_context"].is_null());
    assert_eq!(result.revision, 0);
    assert_eq!(
        session.execute("filemaker_create", &args).unwrap().revision,
        1
    );
}

#[test]
fn definitions_are_accepted_by_appcore_ai() {
    let tools = tool_definitions();
    assert_eq!(tools.len(), 20);
    let inspect = tools
        .iter()
        .find(|tool| tool.name == "filemaker_inspect")
        .unwrap();
    let inspect_schema: serde_json::Value = serde_json::from_str(&inspect.input_schema).unwrap();
    assert!(inspect_schema["required"].is_null());
    let mask = tools
        .iter()
        .find(|tool| tool.name == "filemaker_debug_mask")
        .unwrap();
    let mask_schema: serde_json::Value = serde_json::from_str(&mask.input_schema).unwrap();
    assert_eq!(mask_schema["properties"]["view"]["enum"][3], "combined");
    let preflight = tools
        .iter()
        .find(|tool| tool.name == "filemaker_preflight")
        .unwrap();
    let preflight_schema: serde_json::Value =
        serde_json::from_str(&preflight.input_schema).unwrap();
    assert_eq!(preflight_schema["properties"]["strict"]["type"], "boolean");
    let preview = tools
        .iter()
        .find(|tool| tool.name == "filemaker_preview")
        .unwrap();
    let preview_schema: serde_json::Value = serde_json::from_str(&preview.input_schema).unwrap();
    assert_eq!(preview_schema["properties"]["dpi"]["maximum"], 9600);
    let export = tools
        .iter()
        .find(|tool| tool.name == "filemaker_export")
        .unwrap();
    let export_schema: serde_json::Value = serde_json::from_str(&export.input_schema).unwrap();
    assert_eq!(export_schema["properties"]["format"]["enum"][4], "html");
    assert_eq!(export_schema["properties"]["format"]["enum"][5], "csv");
    for tool in &tools {
        let schema: serde_json::Value = serde_json::from_str(&tool.input_schema).unwrap();
        assert_eq!(schema["additionalProperties"], false, "{}", tool.name);
    }
    AiGenerationOptions {
        tools,
        ..AiGenerationOptions::default()
    }
    .validate()
    .unwrap();

    let mut session = session();
    let execution = session
        .execute_call(&AiToolCall {
            id: "call-1".to_owned(),
            name: "filemaker_capabilities".to_owned(),
            arguments_json: "{}".to_owned(),
        })
        .unwrap();
    assert_eq!(execution.tool, "filemaker_capabilities");
}

#[test]
fn complete_recommended_loop_executes_all_twenty_tools() {
    let mut session = FileMakerAiSession::empty(
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy {
            allow_document_replacement: true,
            ..AiBridgePolicy::default()
        },
    )
    .unwrap();
    let document = document();
    let document_args = json!({"document": document}).to_string();
    let mut executed = BTreeSet::new();
    let mut run = |name: &str, arguments: &str| {
        let result = session.execute(name, arguments).unwrap();
        executed.insert(name.to_owned());
        result
    };

    run("filemaker_capabilities", "{}");
    run("filemaker_schema", "{}");
    run("filemaker_create", &document_args);
    let context = run("filemaker_capabilities", "{}");
    assert_eq!(
        context.value["document_context"]["purpose"],
        "Exercise bounded editing"
    );
    run("filemaker_load", &document_args);
    run(
        "filemaker_add",
        r#"{"element":{"id":"transient","type":"rect","x":"70pt","y":"50pt","width":"5pt","height":"5pt"}}"#,
    );
    run("filemaker_remove", r#"{"id":"transient"}"#);
    run("filemaker_clone", r#"{"id":"box","new_id":"copy"}"#);
    run(
        "filemaker_set",
        r#"{"id":"box","text":"bounded","hidden":false}"#,
    );
    let patch = json!({
        "patch": Patch {
            sequence: 7,
            operations: vec![PatchOperation::SetHidden {
                id: ElementId::new("box").unwrap(),
                hidden: false,
            }],
        }
    })
    .to_string();
    run("filemaker_patch", &patch);
    run(
        "filemaker_align",
        r#"{"id":"copy","reference":"box","edge":"right"}"#,
    );
    run("filemaker_place", r#"{"id":"copy","x":"30pt","y":"10pt"}"#);
    run("filemaker_inspect", r#"{"id":"box"}"#);
    run("filemaker_explain", r#"{"id":"box"}"#);
    run("filemaker_measure", r#"{"id":"box"}"#);
    run("filemaker_validate", "{}");
    run(
        "filemaker_preflight",
        r#"{"format":"svg","fidelity":"strict"}"#,
    );
    run("filemaker_preview", r#"{"page":0,"dpi":96}"#);
    run("filemaker_debug_mask", r#"{"page":0,"view":"combined"}"#);
    run(
        "filemaker_query_free_regions",
        r#"{"page":0,"minimum_width":"1pt","minimum_height":"1pt"}"#,
    );
    run(
        "filemaker_export",
        r#"{"format":"svg","fidelity":"strict"}"#,
    );

    let declared: BTreeSet<String> = tool_definitions()
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert_eq!(executed, declared);
}

#[test]
fn schema_reports_text_overflow_writing_modes_and_prepared_capabilities() {
    let mut session = session();
    let schema = session.execute("filemaker_schema", "{}").unwrap();
    assert_eq!(schema.value["text_overflow"][2], "ellipsis");
    assert_eq!(schema.value["writing_modes"][1], "vertical_rl");
    assert_eq!(schema.value["prepared_text_capabilities"][0], "color_emoji");
    assert_eq!(schema.value["color_sources"][5], "cmyk");
    assert_eq!(schema.value["resolvers"][1], "font_sandbox");
    assert_eq!(schema.value["style_cascade"][4], "data_rules");
    assert_eq!(schema.value["export_style_override"][4], "paint_only");
    assert_eq!(schema.value["paint_order"][3], "collision_independent");
    assert_eq!(schema.value["patch_operations"][2], "set_style");
    assert_eq!(schema.value["image_fit"][4], "scale_down");
    assert_eq!(schema.value["image_contract"][6], "effective_dpi");
    assert_eq!(schema.value["coordinate_units"][7], "normalized_0_to_1");
    assert_eq!(schema.value["canvas_primitives"][7], "path");
    assert_eq!(schema.value["path_commands"][2], "curve");
    assert_eq!(schema.value["simple_add"][0], "element.type");
    assert_eq!(schema.value["validation_stages"][3], "preflight");
    assert_eq!(schema.value["fingerprint_inputs"][4], "patches");
    assert_eq!(schema.value["cache_contract"][1], "resolve_on_miss");
    assert_eq!(schema.value["export_contract"][5], "raster_dpi_only");
    assert_eq!(schema.value["debug_overlay"][8], "collision");
    assert_eq!(schema.value["mask_views"][3], "combined");
    assert_eq!(schema.value["mask_json"][1], "free");
    assert_eq!(schema.value["inspect_trace"][5], "page_reflow");
    assert_eq!(
        schema.value["layout_constraints"][3],
        "aspect_ratio_millionths"
    );
    assert_eq!(schema.value["guide_anchor"], "guide:name[+offset]");
    assert_eq!(schema.value["flow_distribution"][3], "space_between");
    assert_eq!(schema.value["exclusions"][1], "non_painted");
    assert_eq!(schema.value["page_layers"][7], "page_n_of_m");
    assert_eq!(schema.value["table_planning"][8], "streaming");
    assert_eq!(schema.value["table_planning"][9], "resolved_fragments");
    assert_eq!(schema.value["table_source"][2], "typed_rows");
    assert_eq!(schema.value["table_rendering"][0], "pdf_editable");
    assert_eq!(schema.value["table_rendering"][2], "pdf_hybrid");

    let capabilities = session.execute("filemaker_capabilities", "{}").unwrap();
    assert_eq!(capabilities.value["pdf_modes"][2], "hybrid");
    assert_eq!(capabilities.value["prepared_pdf"][0], "links");

    let validation = session.execute("filemaker_validate", "{}").unwrap();
    assert_eq!(validation.value["valid"], true);
    assert_eq!(validation.value["truncated"], false);
}

#[test]
fn small_patch_changes_revision_and_locked_target_is_rejected() {
    let mut session = session();
    let changed = session
        .execute("filemaker_set", r#"{"id":"box","text":"bounded"}"#)
        .unwrap();
    assert_eq!(changed.revision, 1);
    let styled = session
        .execute(
            "filemaker_set",
            r#"{"id":"box","style":{"fill":{"space":"gray","value":64}}}"#,
        )
        .unwrap();
    assert_eq!(styled.revision, 2);
    let inspected = session
        .execute("filemaker_inspect", r#"{"id":"box"}"#)
        .unwrap();
    assert_eq!(inspected.revision, 2);
    assert_eq!(inspected.value["provenance"]["patches"][1], 2);

    let error = session
        .execute("filemaker_remove", r#"{"id":"fixed"}"#)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("outside the template edit policy"));
}

#[test]
fn add_accepts_the_compact_canvas_source_language() {
    let mut session = session();
    let added = session
        .execute(
            "filemaker_add",
            r##"{
                "element": {
                    "id": "curve",
                    "type": "path",
                    "x": "0.1normalized",
                    "y": "10px",
                    "width": "25%",
                    "height": "10lu",
                    "collision": false,
                    "layer": "art",
                    "z_index": 4,
                    "transform": {"rotate": 15},
                    "style": {"stroke": "#112233", "stroke_width": "1pt"},
                    "path": [
                        {"command": "move", "x": "0norm", "y": "1norm"},
                        {
                            "command": "curve",
                            "x1": "0.25norm",
                            "y1": "0norm",
                            "x2": "0.75norm",
                            "y2": "0norm",
                            "x": "1norm",
                            "y": "1norm"
                        },
                        {"command": "close"}
                    ]
                }
            }"##,
        )
        .unwrap();
    assert_eq!(added.revision, 1);

    let inspection = session
        .execute("filemaker_inspect", r#"{"id":"curve"}"#)
        .unwrap();
    assert_eq!(
        inspection.value["bounds"]["layout"]["origin"]["x"],
        10_000_000
    );
    assert_eq!(
        inspection.value["bounds"]["layout"]["origin"]["y"],
        7_500_000
    );
    assert_eq!(
        inspection.value["bounds"]["layout"]["size"]["width"],
        25_000_000
    );
    assert_eq!(inspection.value["layer"], "art");
    assert_eq!(inspection.value["z_index"], 4);
    assert_eq!(inspection.value["collidable"], false);
}

#[test]
fn compact_add_rejects_fields_that_require_the_compiler_pipeline() {
    let mut session = session();
    let error = session
        .execute(
            "filemaker_add",
            r#"{"element":{"id":"curve","type":"rect","styles":["named"]}}"#,
        )
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("does not run compiler expansion or data binding"));
}

#[test]
fn page_inspection_preserves_shape_and_rejects_before_owned_result_tree() {
    let mut inspected = session();
    let page = inspected
        .execute("filemaker_inspect", r#"{"page":0}"#)
        .unwrap();
    assert_eq!(page.value["page"], 0);
    assert!(page.value["role"].is_string());
    assert!(page.value["elements"].is_number());
    assert!(page.value["overflow"].is_array());
    assert!(page.value["occupied"].is_object());
    assert!(page.value["exclusions"].is_array());
    assert!(page.value["regions"].is_array());

    let mut limited = FileMakerAiSession::new(
        document(),
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy {
            max_result_bytes: 256,
            ..AiBridgePolicy::default()
        },
    )
    .unwrap();
    assert!(matches!(
        limited.execute("filemaker_inspect", r#"{"page":0}"#),
        Err(BridgeError::Policy(message))
            if message == "serialized tool result exceeds policy byte limit"
    ));
}

#[test]
fn exact_arguments_and_bridge_budgets_fail_closed() {
    assert!(FileMakerAiSession::empty(
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy {
            allowed_tools: BTreeSet::from(["filemaker_unknown".to_owned()]),
            ..AiBridgePolicy::default()
        },
    )
    .is_err());
    let mut restricted = FileMakerAiSession::new(
        document(),
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy {
            max_tool_calls: 1,
            max_argument_bytes: 32,
            allow_artifact_bytes: false,
            ..AiBridgePolicy::default()
        },
    )
    .unwrap();
    assert!(restricted
        .execute("filemaker_schema", r#"{"unknown":true}"#)
        .unwrap_err()
        .to_string()
        .contains("unknown tool argument"));
    assert!(restricted
        .execute("filemaker_preview", "{}")
        .unwrap_err()
        .to_string()
        .contains("tool call budget exhausted"));

    let mut artifacts_disabled = FileMakerAiSession::new(
        document(),
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy {
            allow_artifact_bytes: false,
            ..AiBridgePolicy::default()
        },
    )
    .unwrap();
    assert!(artifacts_disabled
        .execute("filemaker_preview", "{}")
        .unwrap_err()
        .to_string()
        .contains("artifact bytes are disabled"));
    let oversized = format!(r#"{{"id":"{}"}}"#, "x".repeat(64 * 1024));
    assert!(artifacts_disabled
        .execute("filemaker_measure", &oversized)
        .unwrap_err()
        .to_string()
        .contains("argument"));
    assert!(artifacts_disabled
        .execute("filemaker_inspect", r#"{"id":"box","page":0}"#)
        .unwrap_err()
        .to_string()
        .contains("either id or page"));
}

#[test]
fn malformed_unicode_element_returns_error_without_changing_session() {
    let mut session = session();
    let before = session
        .execute("filemaker_inspect", r#"{"id":"box"}"#)
        .unwrap();
    for prefix in 0..4 {
        let arguments =
            json!({"element": {"type": format!("{}{}", "x".repeat(prefix), "日".repeat(200))}})
                .to_string();
        let error = session.execute("filemaker_add", &arguments).unwrap_err();
        let BridgeError::Json(message) = error else {
            panic!("expected JSON conversion failure")
        };
        assert!(message.len() <= 512);
    }
    let after = session
        .execute("filemaker_inspect", r#"{"id":"box"}"#)
        .unwrap();
    assert_eq!(before, after);
    let capabilities = session.execute("filemaker_capabilities", "{}").unwrap();
    assert_eq!(capabilities.value["calls_used"], 7);
}

#[test]
fn capabilities_preserve_context_and_exact_result_budget() {
    let mut document = document();
    document.ai_policy.purpose = "é日\"\\\n".repeat(32);
    document.ai_policy.rules = vec!["bounded \"rule\" 日本語".repeat(8); 64];
    for loaded in [None, Some(document)] {
        let make_session = |limit| {
            let policy = AiBridgePolicy {
                max_result_bytes: limit,
                ..AiBridgePolicy::default()
            };
            match &loaded {
                Some(document) => FileMakerAiSession::new(
                    document.clone(),
                    ResourceLimits::default(),
                    FontManager::default(),
                    None,
                    policy,
                )
                .unwrap(),
                None => FileMakerAiSession::empty(
                    ResourceLimits::default(),
                    FontManager::default(),
                    None,
                    policy,
                )
                .unwrap(),
            }
        };
        let mut expected = make_session(64 * 1024)
            .execute("filemaker_capabilities", "{}")
            .unwrap()
            .value;
        assert_eq!(
            expected["document_context"],
            loaded
                .as_ref()
                .map_or(serde_json::Value::Null, |document| json!({
                    "template": document.template_id,
                    "model": document.model,
                    "purpose": document.ai_policy.purpose,
                    "rules": document.ai_policy.rules,
                    "editable": document.ai_policy.editable,
                    "locked": document.ai_policy.locked,
                    "root_elements": document.elements.len(),
                }))
        );
        // The budget is itself in the response; account for its decimal width.
        let mut exact = serde_json::to_vec(&expected).unwrap().len();
        for _ in 0..4 {
            expected["bridge_policy"]["max_result_bytes"] = json!(exact);
            exact = serde_json::to_vec(&appcore_filemaker_ai::ToolExecution {
                tool: "filemaker_capabilities".to_owned(),
                revision: 0,
                value: expected.clone(),
            })
            .unwrap()
            .len();
        }
        assert_eq!(expected["bridge_policy"]["max_result_bytes"], exact);
        assert_eq!(
            make_session(exact)
                .execute("filemaker_capabilities", "{}")
                .unwrap()
                .value,
            expected
        );
        assert_eq!((exact - 1).to_string().len(), exact.to_string().len());
        assert!(matches!(
            make_session(exact - 1).execute("filemaker_capabilities", "{}"),
            Err(BridgeError::Policy(_))
        ));
    }
}

#[test]
fn serialized_result_limit_accepts_exact_size_and_rejects_one_byte_less() {
    let mut reference = FileMakerAiSession::empty(
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy::default(),
    )
    .unwrap();
    let execution = reference.execute("filemaker_schema", "{}").unwrap();
    let exact_size = serde_json::to_vec(&execution).unwrap().len();

    let mut exact = FileMakerAiSession::empty(
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy {
            max_result_bytes: exact_size,
            ..AiBridgePolicy::default()
        },
    )
    .unwrap();
    exact.execute("filemaker_schema", "{}").unwrap();

    let mut undersized = FileMakerAiSession::empty(
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy {
            max_result_bytes: exact_size - 1,
            ..AiBridgePolicy::default()
        },
    )
    .unwrap();
    let error = undersized.execute("filemaker_schema", "{}").unwrap_err();
    assert!(matches!(
        error,
        BridgeError::Policy(message)
            if message == "serialized tool result exceeds policy byte limit"
    ));
}

#[test]
fn failed_load_preserves_document_and_revision() {
    let mut session = FileMakerAiSession::new(
        document(),
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy {
            allow_document_replacement: true,
            ..AiBridgePolicy::default()
        },
    )
    .unwrap();
    let mut invalid = document();
    invalid.page_size = None;
    let error = session
        .execute("filemaker_load", &json!({"document": invalid}).to_string())
        .unwrap_err();
    assert!(error.to_string().contains("explicit page size"));
    let inspected = session
        .execute("filemaker_inspect", r#"{"id":"box"}"#)
        .unwrap();
    assert_eq!(inspected.revision, 0);
    assert_eq!(inspected.value["id"], "box");
}

#[test]
fn trusted_document_cannot_be_replaced_without_explicit_bridge_authority() {
    let mut session = session();
    let error = session
        .execute(
            "filemaker_load",
            &json!({"document": document()}).to_string(),
        )
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("document replacement is disabled"));
}

#[test]
fn invalid_patch_budget_or_sequence_does_not_change_state() {
    let limits = ResourceLimits {
        max_text_bytes: 4,
        ..ResourceLimits::default()
    };
    let mut session = FileMakerAiSession::new(
        document(),
        limits,
        FontManager::default(),
        None,
        AiBridgePolicy::default(),
    )
    .unwrap();
    assert!(session
        .execute("filemaker_set", r#"{"id":"box","text":"12345"}"#)
        .is_err());
    let wrong_sequence = json!({
        "patch": Patch {
            sequence: 9,
            operations: vec![PatchOperation::SetHidden {
                id: ElementId::new("box").unwrap(),
                hidden: true,
            }],
        }
    })
    .to_string();
    assert!(session
        .execute("filemaker_patch", &wrong_sequence)
        .unwrap_err()
        .to_string()
        .contains("next session revision"));
    let changed = session
        .execute("filemaker_set", r#"{"id":"box","text":"ok"}"#)
        .unwrap();
    assert_eq!(changed.revision, 1);
}

#[test]
fn destructive_patch_cannot_remove_a_locked_descendant() {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler
        .compile_template_yaml(
            br"filemaker: '1.0'
model: canvas
id: locked-subtree
page: { width: 40pt, height: 40pt }
ai:
  editable: [container]
  locked: [sealed]
elements:
  - id: container
    type: group
    width: 20pt
    height: 20pt
    children:
      - { id: sealed, type: rect, width: 10pt, height: 10pt, locked: true }
",
        )
        .unwrap();
    let document = compiler
        .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
        .unwrap();
    let mut session = FileMakerAiSession::new(
        document,
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy::default(),
    )
    .unwrap();
    let error = session
        .execute("filemaker_remove", r#"{"id":"container"}"#)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("outside the template edit policy"));
    assert_eq!(
        session
            .execute("filemaker_inspect", r#"{"id":"sealed"}"#)
            .unwrap()
            .value["id"],
        "sealed"
    );
}

#[test]
fn export_returns_bounded_base64_without_filesystem_output() {
    let mut session = session();
    let result = session
        .execute(
            "filemaker_export",
            r#"{"format":"svg","fidelity":"strict"}"#,
        )
        .unwrap();
    assert_eq!(result.value["media_type"], "image/svg+xml");
    assert!(result.value["base64"].as_str().unwrap().len() < 64 * 1024);

    let hybrid = session
        .execute(
            "filemaker_export",
            r#"{"format":"pdf","pdf_mode":"hybrid","fidelity":"strict"}"#,
        )
        .unwrap();
    assert_eq!(hybrid.value["media_type"], "application/pdf");
    assert!(hybrid.value["base64"].as_str().unwrap().len() < 64 * 1024);
}

#[test]
fn dataset_csv_executes_through_the_ai_bridge() {
    let compiler = Compiler::builder().build().unwrap();
    let template = compiler
        .compile_template_yaml(
            br"filemaker: '1.0'
model: dataset
id: ai-csv
elements:
  - id: rows
    type: table
    binding: data.rows
    table:
      columns:
        - { field: name, header: Name, width: { mode: fixed, value: 40pt } }
      max_rows: 2
      max_row_fields: 1
      max_cell_bytes: 32
",
        )
        .unwrap();
    let data = DataValue::Object(BTreeMap::from([(
        "rows".to_owned(),
        DataValue::Array(vec![DataValue::Object(BTreeMap::from([(
            "name".to_owned(),
            DataValue::String("Alpha".to_owned()),
        )]))]),
    )]));
    let document = compiler.bind(&template, &data, &[]).unwrap();
    let mut session = FileMakerAiSession::new(
        document,
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy::default(),
    )
    .unwrap();
    let result = session
        .execute("filemaker_export", r#"{"format":"csv","table":"rows"}"#)
        .unwrap();
    assert_eq!(result.value["media_type"], "text/csv; charset=utf-8");
    assert_eq!(result.value["table"], "rows");
    assert_eq!(result.value["base64"], "TmFtZQ0KQWxwaGENCg==");
    let validation = session.execute("filemaker_validate", "{}").unwrap();
    assert_eq!(validation.value["valid"], true);
    assert_eq!(validation.value["pages"], 0);
    session = session.with_control(cancel_control(appcore_filemaker::ProgressPhase::Export));
    assert_cancelled(session.execute("filemaker_export", r#"{"format":"csv","table":"rows"}"#));
}
