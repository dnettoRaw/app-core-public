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
use serde::Deserialize;
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
        .ok_or(BridgeError::InvalidInput("element"))?;
    if value.get("type").is_some() {
        let source = ElementSource::deserialize(value).map_err(json_error)?;
        return source.to_ir(&session.limits).map_err(BridgeError::from);
    }
    ElementIr::deserialize(value).map_err(json_error)
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
    let result = patch_result(session, patch.operations.len())?;
    session.apply_patch(&patch)?;
    Ok(result)
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
    let result = json!({"template": template, "pages": pages});
    crate::session::enforce_result_limit(&result, session.result_limit())?;
    session.commit_document(document, scene);
    session.revision = revision;
    Ok(result)
}

fn apply(session: &mut FileMakerAiSession, operations: Vec<PatchOperation>) -> BridgeResult<Value> {
    let count = operations.len();
    let result = patch_result(session, count)?;
    let sequence = session
        .revision
        .checked_add(1)
        .ok_or_else(|| BridgeError::Policy("patch sequence overflow".to_owned()))?;
    session.apply_patch(&Patch {
        sequence,
        operations,
    })?;
    Ok(result)
}

fn patch_result(session: &FileMakerAiSession, count: usize) -> BridgeResult<Value> {
    let result = json!({"applied_operations": count});
    crate::session::enforce_result_limit(&result, session.result_limit())?;
    Ok(result)
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
    T::deserialize(args.get(name).ok_or(BridgeError::InvalidInput(name))?).map_err(json_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AiBridgePolicy;
    use appcore_filemaker::{Compiler, DataValue, FontManager, ResourceLimits};

    #[test]
    fn field_deserializer_borrows_the_existing_json_string() {
        struct BorrowProbe(bool);
        impl<'de> serde::Deserialize<'de> for BorrowProbe {
            fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = BorrowProbe;
                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter<'_>,
                    ) -> std::fmt::Result {
                        formatter.write_str("a string")
                    }
                    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
                        Ok(BorrowProbe(false))
                    }
                    fn visit_borrowed_str<E: serde::de::Error>(
                        self,
                        _: &'de str,
                    ) -> Result<Self::Value, E> {
                        Ok(BorrowProbe(true))
                    }
                }
                decoder.deserialize_str(Visitor)
            }
        }
        let args = json!({"text": "日本語 العربية français".repeat(1024)});
        assert!(field::<BorrowProbe>(&args, "text").unwrap().0);
        assert!(
            !serde_json::from_value::<BorrowProbe>(args["text"].clone())
                .unwrap()
                .0
        );
        assert!(matches!(
            field::<BorrowProbe>(&args, "missing"),
            Err(BridgeError::InvalidInput("missing"))
        ));
        assert!(matches!(
            field::<BorrowProbe>(&json!({"text": 1}), "text"),
            Err(BridgeError::Json(_))
        ));
    }

    fn document() -> DocumentIr {
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler
            .compile_template_yaml(
                br"filemaker: '1.0'
model: canvas
id: result-budget
page: { width: 100pt, height: 100pt }
elements:
  - { id: box, type: rect, width: 10pt, height: 10pt }
",
            )
            .unwrap();
        compiler
            .bind(&template, &DataValue::Object(Default::default()), &[])
            .unwrap()
    }

    fn empty(limit: usize) -> FileMakerAiSession {
        FileMakerAiSession::empty(
            ResourceLimits::default(),
            FontManager::default(),
            None,
            AiBridgePolicy {
                max_result_bytes: limit,
                allow_document_replacement: true,
                ..AiBridgePolicy::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn mutation_result_exact_byte_boundary_is_accepted() {
        let doc = document();
        for (tool, args, expected) in [
            (
                "filemaker_create",
                json!({"document": doc}),
                json!({"template": "result-budget", "pages": 1}),
            ),
            (
                "filemaker_load",
                json!({"document": doc}),
                json!({"template": "result-budget", "pages": 1}),
            ),
            (
                "filemaker_set",
                json!({"id":"box", "hidden":true}),
                json!({"applied_operations":1}),
            ),
            (
                "filemaker_patch",
                json!({"patch":{"sequence":1,
                "operations":[{"op":"set_hidden", "id":"box", "hidden":true}]}}),
                json!({"applied_operations":1}),
            ),
        ] {
            let exact = serde_json::to_vec(&crate::ToolExecution {
                tool: tool.to_owned(),
                revision: 1,
                value: expected.clone(),
            })
            .unwrap()
            .len();
            for limit in [exact - 1, exact] {
                let mut session = empty(limit);
                if tool != "filemaker_create" {
                    let scene = session.validate_document(&doc).unwrap();
                    session.commit_document(doc.clone(), scene);
                }
                let result = session.execute(tool, &args.to_string());
                if limit == exact {
                    assert_eq!(result.unwrap().value, expected);
                    assert_eq!(session.revision, 1);
                } else {
                    assert!(matches!(result, Err(BridgeError::Policy(_))));
                    assert_eq!(session.revision, 0);
                }
            }
        }
    }

    #[test]
    fn result_budget_failure_does_not_commit_create_or_load() {
        let args = json!({"document": document()}).to_string();
        let mut session = empty(1);
        assert!(matches!(
            session.execute("filemaker_create", &args),
            Err(BridgeError::Policy(_))
        ));
        assert!(session.document.is_none());
        assert_eq!(session.revision, 0);
        assert_eq!(session.result_limit(), session.policy.max_result_bytes);

        let initial = document();
        let scene = session.validate_document(&initial).unwrap();
        session.commit_document(initial, scene);
        let original = session.document.clone().unwrap();
        let original_scene = session.resolve().unwrap();
        assert!(matches!(
            session.execute("filemaker_load", &args),
            Err(BridgeError::Policy(_))
        ));
        assert!(std::sync::Arc::ptr_eq(
            &original,
            session.document.as_ref().unwrap()
        ));
        assert!(std::sync::Arc::ptr_eq(
            &original_scene,
            &session.resolve().unwrap()
        ));
        assert_eq!(session.revision, 0);
        assert_eq!(session.result_limit(), session.policy.max_result_bytes);
    }

    #[test]
    fn result_budget_failure_does_not_commit_patch_or_set() {
        for (tool, args) in [
            ("filemaker_set", json!({"id":"box", "hidden":true})),
            (
                "filemaker_patch",
                json!({"patch": {"sequence":1,
                "operations": [{"op":"set_hidden", "id":"box", "hidden":true}]}}),
            ),
        ] {
            let mut session = empty(1);
            let initial = document();
            let scene = session.validate_document(&initial).unwrap();
            session.commit_document(initial, scene);
            let original = session.document.clone().unwrap();
            let original_scene = session.resolve().unwrap();
            assert!(matches!(
                session.execute(tool, &args.to_string()),
                Err(BridgeError::Policy(_))
            ));
            assert!(std::sync::Arc::ptr_eq(
                &original,
                session.document.as_ref().unwrap()
            ));
            assert!(std::sync::Arc::ptr_eq(
                &original_scene,
                &session.resolve().unwrap()
            ));
            assert_eq!(session.revision, 0);
            assert_eq!(session.result_limit(), session.policy.max_result_bytes);
        }
    }
}
