// =============================================================================
//        #######
//     ###       ###     F: patch.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded patch contracts and behavior for this crate.

use serde::{Deserialize, Serialize};

use crate::{DocumentIr, ElementId, ElementIr, ErrorCode, FileMakerError, Length, Result, Style};

/// One immutable ordered patch batch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Patch {
    /// Stable sequence used in provenance.
    pub sequence: u64,
    /// Operations applied atomically in order.
    pub operations: Vec<PatchOperation>,
}

/// Supported runtime mutation operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PatchOperation {
    /// Set literal text.
    SetText {
        /// Target ID.
        id: ElementId,
        /// New text.
        text: String,
    },
    /// Set visibility.
    SetHidden {
        /// Target ID.
        id: ElementId,
        /// Hidden state.
        hidden: bool,
    },
    /// Overlay a validated runtime style layer before layout.
    SetStyle {
        /// Target ID.
        id: ElementId,
        /// Partial style to overlay.
        style: Style,
    },
    /// Move an element.
    Move {
        /// Target ID.
        id: ElementId,
        /// New x.
        x: Length,
        /// New y.
        y: Length,
    },
    /// Resize an element.
    Resize {
        /// Target ID.
        id: ElementId,
        /// New width.
        width: Length,
        /// New height.
        height: Length,
    },
    /// Remove a node and its subtree.
    Remove {
        /// Target ID.
        id: ElementId,
    },
    /// Add a child or root node.
    Add {
        /// Optional parent ID.
        parent: Option<ElementId>,
        /// New element.
        element: ElementIr,
    },
    /// Clone a node with a new root ID.
    Clone {
        /// Source ID.
        id: ElementId,
        /// New root ID.
        new_id: ElementId,
    },
    /// Replace a node while retaining its location in source order.
    Replace {
        /// Target ID.
        id: ElementId,
        /// Replacement.
        element: ElementIr,
    },
}

/// Transactional patch executor using copy-on-write rollback semantics.
pub struct PatchTransaction<'a> {
    document: &'a mut DocumentIr,
    max_operations: usize,
}

impl<'a> PatchTransaction<'a> {
    /// Starts a transaction over a document.
    #[must_use]
    pub fn new(document: &'a mut DocumentIr, max_operations: usize) -> Self {
        Self {
            document,
            max_operations,
        }
    }

    /// Applies the complete patch or restores the exact prior document.
    pub fn apply(&mut self, patch: &Patch) -> Result<()> {
        self.apply_with_rollback_snapshot(patch).map(drop)
    }

    /// Applies a patch and returns the owned rollback snapshot on success.
    pub fn apply_with_rollback_snapshot(&mut self, patch: &Patch) -> Result<DocumentIr> {
        if patch.operations.len() > self.max_operations {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "patch operation limit exceeded",
            ));
        }
        let original = self.document.clone();
        for operation in &patch.operations {
            if let Err(error) = apply_operation(self.document, operation, patch.sequence) {
                *self.document = original;
                return Err(error);
            }
        }
        if let Err(error) = Self::validate(self.document) {
            *self.document = original;
            return Err(error);
        }
        Ok(original)
    }

    /// Validates patch-sensitive document invariants without cloning it.
    pub fn validate(document: &DocumentIr) -> Result<()> {
        validate_document_ids(&document.elements)?;
        crate::layout_page::validate_page_layer_ir(document)
    }
}

fn apply_operation(
    document: &mut DocumentIr,
    operation: &PatchOperation,
    sequence: u64,
) -> Result<()> {
    match operation {
        PatchOperation::SetText { id, text } => mutate(document, id, sequence, |element| {
            element.text = Some(text.clone());
        }),
        PatchOperation::SetHidden { id, hidden } => {
            mutate(document, id, sequence, |element| element.hidden = *hidden)
        }
        PatchOperation::SetStyle { id, style } => {
            style.validate()?;
            mutate(document, id, sequence, |element| {
                element.style.overlay(style)
            })
        }
        PatchOperation::Move { id, x, y } => mutate(document, id, sequence, |element| {
            element.geometry.x = Some(*x);
            element.geometry.y = Some(*y);
            element.geometry.align_x = None;
            element.geometry.align_y = None;
            element.geometry.anchors.clear();
        }),
        PatchOperation::Resize { id, width, height } => mutate(document, id, sequence, |element| {
            element.geometry.width = Some(*width);
            element.geometry.height = Some(*height);
            element.geometry.constraints = crate::LayoutConstraints::default();
        }),
        PatchOperation::Remove { id } => remove(&mut document.elements, id),
        PatchOperation::Add { parent, element } => {
            add(document, parent.as_ref(), element.clone(), sequence)
        }
        PatchOperation::Clone { id, new_id } => clone_element(document, id, new_id, sequence),
        PatchOperation::Replace { id, element } => {
            replace(&mut document.elements, id, element.clone(), sequence)
        }
    }
}

fn mutate(
    document: &mut DocumentIr,
    id: &ElementId,
    sequence: u64,
    action: impl FnOnce(&mut ElementIr),
) -> Result<()> {
    let element = find_mut(&mut document.elements, id)
        .ok_or_else(|| patch_error("patch target was not found"))?;
    ensure_unlocked(element)?;
    action(element);
    element.provenance.patches.push(sequence);
    Ok(())
}

fn add(
    document: &mut DocumentIr,
    parent: Option<&ElementId>,
    mut element: ElementIr,
    sequence: u64,
) -> Result<()> {
    if find(&document.elements, &element.id).is_some() {
        return Err(patch_error("added element ID already exists"));
    }
    element.provenance.patches.push(sequence);
    if let Some(parent) = parent {
        let parent = find_mut(&mut document.elements, parent)
            .ok_or_else(|| patch_error("add parent was not found"))?;
        ensure_unlocked(parent)?;
        parent.children.push(element);
    } else {
        document.elements.push(element);
    }
    Ok(())
}

fn clone_element(
    document: &mut DocumentIr,
    id: &ElementId,
    new_id: &ElementId,
    sequence: u64,
) -> Result<()> {
    if find(&document.elements, new_id).is_some() {
        return Err(patch_error("clone ID already exists"));
    }
    let mut cloned = find(&document.elements, id)
        .ok_or_else(|| patch_error("clone source was not found"))?
        .clone();
    ensure_unlocked(&cloned)?;
    remap_clone_ids(&mut cloned, id.as_str(), new_id.as_str())?;
    cloned.provenance.patches.push(sequence);
    document.elements.push(cloned);
    Ok(())
}

fn remove(elements: &mut Vec<ElementIr>, id: &ElementId) -> Result<()> {
    let target = find(elements, id).ok_or_else(|| patch_error("remove target was not found"))?;
    ensure_subtree_unlocked(target)?;
    remove_unchecked(elements, id);
    Ok(())
}

fn remove_unchecked(elements: &mut Vec<ElementIr>, id: &ElementId) -> bool {
    if let Some(position) = elements.iter().position(|element| &element.id == id) {
        elements.remove(position);
        return true;
    }
    for element in elements {
        if remove_unchecked(&mut element.children, id) {
            return true;
        }
    }
    false
}

fn replace(
    elements: &mut [ElementIr],
    id: &ElementId,
    mut replacement: ElementIr,
    sequence: u64,
) -> Result<()> {
    let target = find(elements, id).ok_or_else(|| patch_error("replace target was not found"))?;
    ensure_subtree_unlocked(target)?;
    if target.page_placement != replacement.page_placement {
        return Err(patch_error("replace cannot change page-layer ownership"));
    }
    if replace_unchecked(elements, id, &mut replacement, sequence) {
        Ok(())
    } else {
        Err(patch_error("replace target was not found"))
    }
}

fn replace_unchecked(
    elements: &mut [ElementIr],
    id: &ElementId,
    replacement: &mut ElementIr,
    sequence: u64,
) -> bool {
    for element in elements {
        if &element.id == id {
            replacement.provenance.patches.push(sequence);
            *element = replacement.clone();
            return true;
        }
        if replace_unchecked(&mut element.children, id, replacement, sequence) {
            return true;
        }
    }
    false
}

fn find<'a>(elements: &'a [ElementIr], id: &ElementId) -> Option<&'a ElementIr> {
    for element in elements {
        if &element.id == id {
            return Some(element);
        }
        if let Some(found) = find(&element.children, id) {
            return Some(found);
        }
    }
    None
}

fn find_mut<'a>(elements: &'a mut [ElementIr], id: &ElementId) -> Option<&'a mut ElementIr> {
    for element in elements {
        if &element.id == id {
            return Some(element);
        }
        if let Some(found) = find_mut(&mut element.children, id) {
            return Some(found);
        }
    }
    None
}

fn ensure_unlocked(element: &ElementIr) -> Result<()> {
    if element.locked {
        Err(
            FileMakerError::new(ErrorCode::PatchLocked, "element is locked")
                .at(element.id.as_str()),
        )
    } else {
        Ok(())
    }
}

fn ensure_subtree_unlocked(root: &ElementIr) -> Result<()> {
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        ensure_unlocked(element)?;
        stack.extend(element.children.iter().rev());
    }
    Ok(())
}

fn remap_clone_ids(element: &mut ElementIr, old_root: &str, new_root: &str) -> Result<()> {
    let current = element.id.as_str();
    let suffix = current.strip_prefix(old_root).unwrap_or(current);
    element.id = ElementId::new(format!("{new_root}{suffix}"))?;
    for child in &mut element.children {
        remap_clone_ids(child, old_root, new_root)?;
    }
    Ok(())
}

fn validate_document_ids(elements: &[ElementIr]) -> Result<()> {
    let mut ids = std::collections::BTreeSet::new();
    let mut stack: Vec<&ElementIr> = elements.iter().collect();
    while let Some(element) = stack.pop() {
        if !ids.insert(element.id.as_str()) {
            return Err(patch_error(format!(
                "patch produced duplicate element ID `{}`",
                element.id.as_str()
            )));
        }
        stack.extend(&element.children);
    }
    Ok(())
}

fn patch_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::PatchInvalid, message)
}

#[cfg(test)]
mod operation_log_tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{Compiler, DataValue, OperationLog};

    fn document() -> DocumentIr {
        let yaml = br"filemaker: '1.0'
model: canvas
id: log
page: { width: 10pt, height: 10pt }
elements: [{ id: node, type: rect, width: 2pt, height: 2pt }]
";
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml).unwrap();
        compiler
            .bind(&template, &DataValue::Object(BTreeMap::new()), &[])
            .unwrap()
    }

    fn visibility_patch(sequence: u64, hidden: bool) -> Patch {
        Patch {
            sequence,
            operations: vec![PatchOperation::SetHidden {
                id: ElementId::new("node").unwrap(),
                hidden,
            }],
        }
    }

    #[test]
    fn successful_patches_can_be_undone_and_redone() {
        let mut document = document();
        let patch = visibility_patch(1, true);
        let mut log = OperationLog::new(2).unwrap();
        log.apply(&mut document, &patch, 4).unwrap();
        assert!(document.elements[0].hidden);
        log.undo(&mut document).unwrap();
        assert!(!document.elements[0].hidden);
        log.redo(&mut document).unwrap();
        assert!(document.elements[0].hidden);
    }

    #[test]
    fn rejects_a_snapshot_before_mutating_the_document() {
        let mut document = document();
        let original = document.clone();
        let mut log = OperationLog::new_bounded(2, 1).unwrap();
        let error = log
            .apply(&mut document, &visibility_patch(1, true), 4)
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::LimitExceeded);
        assert_eq!(document, original);
        assert_eq!(log.used_bytes(), 0);
    }

    #[test]
    fn evicts_old_snapshots_to_honor_the_aggregate_byte_budget() {
        let mut document = document();
        let original_bytes = crate::memory::serialized_size(&document).unwrap();
        let mut changed = document.clone();
        PatchTransaction::new(&mut changed, 1)
            .apply(&visibility_patch(1, true))
            .unwrap();
        let changed_bytes = crate::memory::serialized_size(&changed).unwrap();
        let mut log = OperationLog::new_bounded(4, original_bytes.max(changed_bytes)).unwrap();
        log.apply(&mut document, &visibility_patch(1, true), 1)
            .unwrap();
        log.apply(&mut document, &visibility_patch(2, false), 1)
            .unwrap();
        assert_eq!(log.undo_len(), 1);
        assert!(log.used_bytes() <= log.max_bytes());
    }
}
