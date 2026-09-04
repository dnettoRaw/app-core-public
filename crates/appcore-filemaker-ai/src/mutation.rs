// =============================================================================
//        #######
//     ###       ###     F: mutation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded mutation contracts and behavior for this crate.

use appcore_filemaker::{
    DocumentIr, ElementId, ElementIr, ElementSource, Length, Patch, PatchOperation, SceneInspector,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::error::json_error;
use crate::{BridgeError, BridgeResult, FileMakerAiSession};

pub(crate) fn create(session: &mut FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    if session.document.is_some() {
        return Err(BridgeError::Policy(
            "create requires an empty session; use load to replace state".to_owned(),
        ));
    }
    replace_document(session, args)
}

pub(crate) fn load(session: &mut FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    if session.document.is_some() && !session.policy.allow_document_replacement {
        return Err(BridgeError::Policy(
            "document replacement is disabled for this session".to_owned(),
        ));
    }
    replace_document(session, args)
}

pub(crate) fn add(session: &mut FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let element = element_field(session, args)?;
    let parent = optional_string(args, "parent")?
        .map(ElementId::new)
        .transpose()?;
    apply(session, vec![PatchOperation::Add { parent, element }])
}

fn element_field(session: &FileMakerAiSession, args: &Value) -> BridgeResult<ElementIr> {
    let value = args
        .get("element")
        .cloned()
        .ok_or(BridgeError::InvalidInput("element"))?;
    if value.get("type").is_some() {
        let source: ElementSource = serde_json::from_value(value).map_err(json_error)?;
        return source.to_ir(&session.limits).map_err(BridgeError::from);
    }
    serde_json::from_value(value).map_err(json_error)
}

pub(crate) fn remove(session: &mut FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    apply(
        session,
        vec![PatchOperation::Remove {
            id: required_id(args, "id")?,
        }],
    )
}

pub(crate) fn clone_element(session: &mut FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    apply(
        session,
        vec![PatchOperation::Clone {
            id: required_id(args, "id")?,
            new_id: required_id(args, "new_id")?,
        }],
    )
}

pub(crate) fn set(session: &mut FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let id = required_id(args, "id")?;
    let mut operations = Vec::new();
    if let Some(text) = optional_string(args, "text")? {
        operations.push(PatchOperation::SetText {
            id: id.clone(),
            text,
        });
    }
    if let Some(hidden) = args.get("hidden") {
        operations.push(PatchOperation::SetHidden {
            id: id.clone(),
            hidden: hidden
                .as_bool()
                .ok_or(BridgeError::InvalidInput("hidden must be boolean"))?,
        });
    }
    if args.get("style").is_some() {
        operations.push(PatchOperation::SetStyle {
            id: id.clone(),
            style: field(args, "style")?,
        });
    }
    if args.get("x").is_some() || args.get("y").is_some() {
        operations.push(PatchOperation::Move {
            id: id.clone(),
            x: field(args, "x")?,
            y: field(args, "y")?,
        });
    }
    if args.get("width").is_some() || args.get("height").is_some() {
        operations.push(PatchOperation::Resize {
            id,
            width: field(args, "width")?,
            height: field(args, "height")?,
        });
    }
    if operations.is_empty() {
        return Err(BridgeError::InvalidInput("set has no requested fields"));
    }
    apply(session, operations)
}

pub(crate) fn patch(session: &mut FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let patch: Patch = field(args, "patch")?;
    session.apply_patch(&patch)?;
    Ok(json!({"applied_operations": patch.operations.len()}))
}

pub(crate) fn place(session: &mut FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    apply(
        session,
        vec![PatchOperation::Move {
            id: required_id(args, "id")?,
            x: field(args, "x")?,
            y: field(args, "y")?,
        }],
    )
}

pub(crate) fn align(session: &mut FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let id = required_id(args, "id")?;
    let reference = required_id(args, "reference")?;
    let edge = optional_string(args, "edge")?.unwrap_or_else(|| "left".to_owned());
    let scene = session.resolve()?;
    let inspector = SceneInspector::new(&scene);
    let target = inspector.inspect_element(&id)?.bounds.layout;
    let reference = inspector.inspect_element(&reference)?.bounds.layout;
    let (x, y) = match edge.as_str() {
        "left" => (reference.origin.x, target.origin.y),
        "right" => (
            reference.right()?.checked_sub(target.size.width)?,
            target.origin.y,
        ),
        "top" => (target.origin.x, reference.origin.y),
        "bottom" => (
            target.origin.x,
            reference.bottom()?.checked_sub(target.size.height)?,
        ),
        "center_x" => (
            reference
                .origin
                .x
                .checked_add(reference.size.width.checked_scale(500_000)?)?
                .checked_sub(target.size.width.checked_scale(500_000)?)?,
            target.origin.y,
        ),
        "center_y" => (
            target.origin.x,
            reference
                .origin
                .y
                .checked_add(reference.size.height.checked_scale(500_000)?)?
                .checked_sub(target.size.height.checked_scale(500_000)?)?,
        ),
        _ => return Err(BridgeError::InvalidInput("unsupported alignment edge")),
    };
    apply(
        session,
        vec![PatchOperation::Move {
            id,
            x: Length::Absolute(x),
            y: Length::Absolute(y),
        }],
    )
}

fn replace_document(session: &mut FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let document: DocumentIr = field(args, "document")?;
    let scene = session.validate_document(&document)?;
    let template = document.template_id.clone();
    let revision = session
        .revision
        .checked_add(1)
        .ok_or_else(|| BridgeError::Policy("session revision overflow".to_owned()))?;
    let pages = scene.as_ref().map_or(0, |scene| scene.pages.len());
    session.commit_document(document, scene);
    session.revision = revision;
    Ok(json!({
        "template": template,
        "pages": pages,
    }))
}

fn apply(session: &mut FileMakerAiSession, operations: Vec<PatchOperation>) -> BridgeResult<Value> {
    let count = operations.len();
    let sequence = session
        .revision
        .checked_add(1)
        .ok_or_else(|| BridgeError::Policy("patch sequence overflow".to_owned()))?;
    session.apply_patch(&Patch {
        sequence,
        operations,
    })?;
    Ok(json!({"applied_operations": count}))
}

fn required_id(args: &Value, name: &'static str) -> BridgeResult<ElementId> {
    ElementId::new(required_string(args, name)?).map_err(BridgeError::from)
}

fn required_string(args: &Value, name: &'static str) -> BridgeResult<String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or(BridgeError::InvalidInput(name))
}

fn optional_string(args: &Value, name: &'static str) -> BridgeResult<Option<String>> {
    args.get(name)
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or(BridgeError::InvalidInput(name))
        })
        .transpose()
}

fn field<T: DeserializeOwned>(args: &Value, name: &'static str) -> BridgeResult<T> {
    serde_json::from_value(
        args.get(name)
            .cloned()
            .ok_or(BridgeError::InvalidInput(name))?,
    )
    .map_err(json_error)
}
