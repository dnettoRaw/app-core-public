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
use serde_json::Value;

use crate::{BridgeError, BridgeResult};

/// Returns the complete bounded standard tool set for `appcore-ai` generation options.
#[must_use]
pub fn tool_definitions() -> Vec<AiToolDefinition> {
    TOOL_SPECS
        .iter()
        .map(|(name, description, input_schema)| AiToolDefinition {
            name: (*name).to_owned(),
            description: (*description).to_owned(),
            input_schema: input_schema.to_string(),
        })
        .collect()
}

type ToolSpec = (&'static str, &'static str, &'static str);

const EMPTY_SCHEMA: &str = r#"{"type":"object","additionalProperties":false}"#;
const DOCUMENT_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"document":{"type":"object"}},"required":["document"],"type":"object"}"#;
const ADD_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"element":{"type":"object"},"parent":{"maxLength":128,"minLength":1,"type":"string"}},"required":["element"],"type":"object"}"#;
const ID_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"id":{"maxLength":128,"minLength":1,"type":"string"}},"required":["id"],"type":"object"}"#;
const CLONE_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"id":{"maxLength":128,"minLength":1,"type":"string"},"new_id":{"maxLength":128,"minLength":1,"type":"string"}},"required":["id","new_id"],"type":"object"}"#;
const SET_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"height":{"type":["string","object"]},"hidden":{"type":"boolean"},"id":{"maxLength":128,"minLength":1,"type":"string"},"style":{"type":"object"},"text":{"type":"string"},"width":{"type":["string","object"]},"x":{"type":["string","object"]},"y":{"type":["string","object"]}},"required":["id"],"type":"object"}"#;
const PATCH_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"patch":{"type":"object"}},"required":["patch"],"type":"object"}"#;
const ALIGN_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"edge":{"enum":["left","right","top","bottom","center_x","center_y"],"type":"string"},"id":{"maxLength":128,"minLength":1,"type":"string"},"reference":{"maxLength":128,"minLength":1,"type":"string"}},"required":["id","reference"],"type":"object"}"#;
const PLACE_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"id":{"maxLength":128,"minLength":1,"type":"string"},"x":{"type":["string","object"]},"y":{"type":["string","object"]}},"required":["id","x","y"],"type":"object"}"#;
const INSPECT_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"id":{"maxLength":128,"minLength":1,"type":"string"},"page":{"minimum":0,"type":"integer"}},"type":"object"}"#;
const PREFLIGHT_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"dpi":{"maximum":9600,"minimum":1,"type":"integer"},"fidelity":{"enum":["strict","best_effort"],"type":"string"},"format":{"enum":["pdf","svg","png","jpeg","html"],"type":"string"},"html_mode":{"enum":["semantic","fixed"],"type":"string"},"jpeg_quality":{"maximum":100,"minimum":1,"type":"integer"},"page":{"minimum":0,"type":"integer"},"pdf_mode":{"enum":["editable","flattened","hybrid"],"type":"string"},"require_accessibility":{"type":"boolean"},"strict":{"type":"boolean"},"style_override":{"type":"object"}},"type":"object"}"#;
const PREVIEW_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"dpi":{"maximum":9600,"minimum":1,"type":"integer"},"page":{"minimum":0,"type":"integer"}},"type":"object"}"#;
const MASK_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"page":{"minimum":0,"type":"integer"},"view":{"enum":["collision","layout","visual","combined"],"type":"string"}},"type":"object"}"#;
const FREE_REGIONS_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"minimum_height":{"type":["string","object"]},"minimum_width":{"type":["string","object"]},"page":{"minimum":0,"type":"integer"}},"type":"object"}"#;
const EXPORT_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"dpi":{"maximum":9600,"minimum":1,"type":"integer"},"fidelity":{"enum":["strict","best_effort"],"type":"string"},"format":{"enum":["pdf","svg","png","jpeg","html","csv"],"type":"string"},"html_mode":{"enum":["semantic","fixed"],"type":"string"},"jpeg_quality":{"maximum":100,"minimum":1,"type":"integer"},"page":{"minimum":0,"type":"integer"},"pdf_mode":{"enum":["editable","flattened","hybrid"],"type":"string"},"style_override":{"type":"object"},"table":{"maxLength":128,"minLength":1,"type":"string"}},"type":"object"}"#;

const TOOL_SPECS: [ToolSpec; 20] = [
    (
        "filemaker_capabilities",
        "Describe formats, limits, and bridge policy",
        EMPTY_SCHEMA,
    ),
    (
        "filemaker_schema",
        "Describe the FileMaker 1.0 schema and IR boundary",
        EMPTY_SCHEMA,
    ),
    (
        "filemaker_create",
        "Create a session from a complete typed DocumentIr",
        DOCUMENT_SCHEMA,
    ),
    (
        "filemaker_load",
        "Replace session state with a validated DocumentIr when host policy allows it",
        DOCUMENT_SCHEMA,
    ),
    (
        "filemaker_add",
        "Add one simple source element or complete ElementIr through a transactional patch",
        ADD_SCHEMA,
    ),
    ("filemaker_remove", "Remove one editable element", ID_SCHEMA),
    (
        "filemaker_clone",
        "Clone one editable element under a new ID",
        CLONE_SCHEMA,
    ),
    (
        "filemaker_set",
        "Set text, visibility, style, position, or size on one element",
        SET_SCHEMA,
    ),
    (
        "filemaker_patch",
        "Apply one bounded typed patch whose sequence is the next session revision",
        PATCH_SCHEMA,
    ),
    (
        "filemaker_align",
        "Align one element edge to another resolved element",
        ALIGN_SCHEMA,
    ),
    (
        "filemaker_place",
        "Move one element to explicit fixed-point lengths",
        PLACE_SCHEMA,
    ),
    (
        "filemaker_inspect",
        "Inspect one element or page",
        INSPECT_SCHEMA,
    ),
    (
        "filemaker_explain",
        "Explain layout decisions and provenance",
        ID_SCHEMA,
    ),
    (
        "filemaker_measure",
        "Return intrinsic, layout, collision, and visual bounds",
        ID_SCHEMA,
    ),
    (
        "filemaker_validate",
        "Resolve and validate the current document",
        EMPTY_SCHEMA,
    ),
    (
        "filemaker_preflight",
        "Run exporter-aware preflight",
        PREFLIGHT_SCHEMA,
    ),
    (
        "filemaker_preview",
        "Return a bounded base64 PNG preview",
        PREVIEW_SCHEMA,
    ),
    (
        "filemaker_debug_mask",
        "Return compact collision-mask geometry",
        MASK_SCHEMA,
    ),
    (
        "filemaker_query_free_regions",
        "Query free page rectangles above a minimum size",
        FREE_REGIONS_SCHEMA,
    ),
    (
        "filemaker_export",
        "Return one bounded base64 export artifact",
        EXPORT_SCHEMA,
    ),
];
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

type ArgumentFields = (&'static [&'static str], &'static [&'static str]);

// Name admission shares the argument contract without building public JSON schemas.
pub(crate) fn is_known_tool(name: &str) -> bool {
    argument_fields(name).is_ok()
}

fn argument_fields(name: &str) -> BridgeResult<ArgumentFields> {
    Ok(match name {
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
    })
}

pub(crate) fn validate_arguments(name: &str, arguments: &Value) -> BridgeResult<()> {
    let (allowed, required) = argument_fields(name)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    #[test]
    fn name_admission_and_fields_match_every_public_schema() {
        let definitions = tool_definitions();
        let mut names = BTreeSet::new();
        for definition in definitions {
            assert!(names.insert(definition.name.clone()));
            assert!(is_known_tool(&definition.name));
            let (allowed, required) = argument_fields(&definition.name).unwrap();
            let schema: Value = serde_json::from_str(&definition.input_schema).unwrap();
            let properties = schema["properties"]
                .as_object()
                .map(|fields| fields.keys().map(String::as_str).collect::<BTreeSet<_>>())
                .unwrap_or_default();
            assert_eq!(
                properties,
                allowed.iter().copied().collect(),
                "{}",
                definition.name
            );
            let required_schema = schema["required"]
                .as_array()
                .map(|fields| {
                    fields
                        .iter()
                        .map(|value| value.as_str().unwrap())
                        .collect::<BTreeSet<_>>()
                })
                .unwrap_or_default();
            assert_eq!(
                required_schema,
                required.iter().copied().collect(),
                "{}",
                definition.name
            );
        }
        assert_eq!(names.len(), 20);
        for name in [
            "",
            "filemaker_unknown",
            "filemaker_inspect ",
            "FILEMAKER_INSPECT",
        ] {
            assert!(!is_known_tool(name));
            assert!(matches!(
                validate_arguments(name, &json!({})),
                Err(BridgeError::InvalidInput("unknown tool name"))
            ));
        }
    }
}
