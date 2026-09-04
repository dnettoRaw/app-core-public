// =============================================================================
//        #######
//     ###       ###     F: tools.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded tools contracts and behavior for this crate.

use appcore_ai::AiToolDefinition;
use serde_json::{json, Value};

use crate::{BridgeError, BridgeResult};

/// Returns the complete bounded standard tool set for `appcore-ai` generation options.
#[must_use]
pub fn tool_definitions() -> Vec<AiToolDefinition> {
    mutation_tools()
        .into_iter()
        .chain(observation_tools())
        .map(|(name, description, input_schema)| AiToolDefinition {
            name: name.to_owned(),
            description: description.to_owned(),
            input_schema,
        })
        .collect()
}

fn mutation_tools() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "filemaker_capabilities",
            "Describe formats, limits, and bridge policy",
            empty_schema(),
        ),
        (
            "filemaker_schema",
            "Describe the FileMaker 1.0 schema and IR boundary",
            empty_schema(),
        ),
        (
            "filemaker_create",
            "Create a session from a complete typed DocumentIr",
            document_schema(),
        ),
        (
            "filemaker_load",
            "Replace session state with a validated DocumentIr when host policy allows it",
            document_schema(),
        ),
        (
            "filemaker_add",
            "Add one simple source element or complete ElementIr through a transactional patch",
            add_schema(),
        ),
        (
            "filemaker_remove",
            "Remove one editable element",
            id_schema(),
        ),
        (
            "filemaker_clone",
            "Clone one editable element under a new ID",
            clone_schema(),
        ),
        (
            "filemaker_set",
            "Set text, visibility, style, position, or size on one element",
            set_schema(),
        ),
        (
            "filemaker_patch",
            "Apply one bounded typed patch whose sequence is the next session revision",
            patch_schema(),
        ),
        (
            "filemaker_align",
            "Align one element edge to another resolved element",
            align_schema(),
        ),
        (
            "filemaker_place",
            "Move one element to explicit fixed-point lengths",
            place_schema(),
        ),
    ]
}

fn observation_tools() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "filemaker_inspect",
            "Inspect one element or page",
            inspect_schema(),
        ),
        (
            "filemaker_explain",
            "Explain layout decisions and provenance",
            id_schema(),
        ),
        (
            "filemaker_measure",
            "Return intrinsic, layout, collision, and visual bounds",
            id_schema(),
        ),
        (
            "filemaker_validate",
            "Resolve and validate the current document",
            empty_schema(),
        ),
        (
            "filemaker_preflight",
            "Run exporter-aware preflight",
            preflight_schema(),
        ),
        (
            "filemaker_preview",
            "Return a bounded base64 PNG preview",
            preview_schema(),
        ),
        (
            "filemaker_debug_mask",
            "Return compact collision-mask geometry",
            mask_schema(),
        ),
        (
            "filemaker_query_free_regions",
            "Query free page rectangles above a minimum size",
            free_regions_schema(),
        ),
        (
            "filemaker_export",
            "Return one bounded base64 export artifact",
            export_schema(),
        ),
    ]
}
/// Recommended deterministic orchestration loop for an application-level agent.
#[must_use]
pub fn recommended_tool_loop() -> &'static [&'static str] {
    &[
        "plan",
        "tools",
        "validate",
        "small_patch",
        "preview_or_inspect",
        "preflight",
        "export",
    ]
}

fn empty_schema() -> String {
    r#"{"type":"object","additionalProperties":false}"#.to_owned()
}

fn id_schema() -> String {
    object_schema(json!({"id": id_property()}), &["id"])
}

fn inspect_schema() -> String {
    object_schema(json!({"id": id_property(), "page": page_property()}), &[])
}

fn mask_schema() -> String {
    object_schema(
        json!({
            "page": page_property(),
            "view": {"type":"string","enum":["collision","layout","visual","combined"]}
        }),
        &[],
    )
}

fn free_regions_schema() -> String {
    object_schema(
        json!({
            "page": page_property(),
            "minimum_width": length_property(),
            "minimum_height": length_property()
        }),
        &[],
    )
}

fn preflight_schema() -> String {
    let mut properties = document_export_properties();
    properties["strict"] = json!({"type":"boolean"});
    properties["require_accessibility"] = json!({"type":"boolean"});
    object_schema(properties, &[])
}

fn document_schema() -> String {
    object_schema(json!({"document":{"type":"object"}}), &["document"])
}

fn add_schema() -> String {
    object_schema(
        json!({"parent": id_property(), "element":{"type":"object"}}),
        &["element"],
    )
}

fn clone_schema() -> String {
    object_schema(
        json!({"id": id_property(), "new_id": id_property()}),
        &["id", "new_id"],
    )
}

fn set_schema() -> String {
    object_schema(
        json!({
            "id": id_property(),
            "text": {"type":"string"},
            "hidden": {"type":"boolean"},
            "style": {"type":"object"},
            "x": length_property(),
            "y": length_property(),
            "width": length_property(),
            "height": length_property()
        }),
        &["id"],
    )
}

fn patch_schema() -> String {
    object_schema(json!({"patch":{"type":"object"}}), &["patch"])
}

fn align_schema() -> String {
    object_schema(
        json!({
            "id": id_property(),
            "reference": id_property(),
            "edge": {"type":"string","enum":["left","right","top","bottom","center_x","center_y"]}
        }),
        &["id", "reference"],
    )
}

fn place_schema() -> String {
    object_schema(
        json!({"id": id_property(), "x": length_property(), "y": length_property()}),
        &["id", "x", "y"],
    )
}

fn preview_schema() -> String {
    object_schema(json!({"page": page_property(), "dpi": dpi_property()}), &[])
}

fn export_schema() -> String {
    let mut properties = document_export_properties();
    properties["format"] = json!({"type":"string","enum":["pdf","svg","png","jpeg","html","csv"]});
    properties["table"] = id_property();
    object_schema(properties, &[])
}

fn document_export_properties() -> Value {
    json!({
        "format":{"type":"string","enum":["pdf","svg","png","jpeg","html"]},
        "fidelity":{"type":"string","enum":["strict","best_effort"]},
        "pdf_mode":{"type":"string","enum":["editable","flattened","hybrid"]},
        "html_mode":{"type":"string","enum":["semantic","fixed"]},
        "page": page_property(),
        "dpi": dpi_property(),
        "jpeg_quality":{"type":"integer","minimum":1,"maximum":100},
        "style_override":{"type":"object"}
    })
}

fn id_property() -> Value {
    json!({"type":"string","minLength":1,"maxLength":128})
}

fn page_property() -> Value {
    json!({"type":"integer","minimum":0})
}

fn dpi_property() -> Value {
    json!({"type":"integer","minimum":1,"maximum":9600})
}

fn length_property() -> Value {
    json!({"type":["string","object"]})
}

fn object_schema(properties: Value, required: &[&str]) -> String {
    let mut schema = json!({
        "type":"object",
        "properties": properties,
        "additionalProperties": false
    });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    schema.to_string()
}

pub(crate) fn validate_arguments(name: &str, arguments: &Value) -> BridgeResult<()> {
    let (allowed, required): (&[&str], &[&str]) = match name {
        "filemaker_capabilities" | "filemaker_schema" | "filemaker_validate" => (&[], &[]),
        "filemaker_create" | "filemaker_load" => (&["document"], &["document"]),
        "filemaker_add" => (&["parent", "element"], &["element"]),
        "filemaker_remove" | "filemaker_explain" | "filemaker_measure" => (&["id"], &["id"]),
        "filemaker_clone" => (&["id", "new_id"], &["id", "new_id"]),
        "filemaker_set" => (
            &["id", "text", "hidden", "style", "x", "y", "width", "height"],
            &["id"],
        ),
        "filemaker_patch" => (&["patch"], &["patch"]),
        "filemaker_align" => (&["id", "reference", "edge"], &["id", "reference"]),
        "filemaker_place" => (&["id", "x", "y"], &["id", "x", "y"]),
        "filemaker_inspect" => (&["id", "page"], &[]),
        "filemaker_preflight" => (
            &[
                "format",
                "fidelity",
                "pdf_mode",
                "html_mode",
                "page",
                "dpi",
                "jpeg_quality",
                "style_override",
                "strict",
                "require_accessibility",
            ],
            &[],
        ),
        "filemaker_preview" => (&["page", "dpi"], &[]),
        "filemaker_debug_mask" => (&["page", "view"], &[]),
        "filemaker_query_free_regions" => (&["page", "minimum_width", "minimum_height"], &[]),
        "filemaker_export" => (
            &[
                "format",
                "fidelity",
                "pdf_mode",
                "html_mode",
                "page",
                "dpi",
                "jpeg_quality",
                "style_override",
                "table",
            ],
            &[],
        ),
        _ => return Err(BridgeError::InvalidInput("unknown tool name")),
    };
    let object = arguments.as_object().ok_or(BridgeError::InvalidInput(
        "tool arguments must be an object",
    ))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(BridgeError::InvalidInput("unknown tool argument"));
    }
    if required.iter().any(|key| !object.contains_key(*key)) {
        return Err(BridgeError::InvalidInput(
            "required tool argument is missing",
        ));
    }
    if name == "filemaker_inspect" && object.contains_key("id") && object.contains_key("page") {
        return Err(BridgeError::InvalidInput(
            "inspect accepts either id or page, not both",
        ));
    }
    Ok(())
}
