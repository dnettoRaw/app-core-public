// =============================================================================
//        #######
//     ###       ###     F: session.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded session contracts and behavior for this crate.

use std::io::{self, Write};
use std::sync::Arc;

use appcore_ai::AiToolCall;
use appcore_filemaker::{
    AssetResolver, DocumentIr, ElementId, ElementIr, ExportContext, FontManager, LayoutEngine,
    LayoutOptions, ModelKind, Patch, PatchOperation, PatchTransaction, ResolvedScene,
    ResourceLimits,
};
use serde::Serialize;
use serde_json::Value;

use crate::error::json_error;
use crate::{AiBridgePolicy, BridgeError, BridgeResult};

/// One compact machine-readable tool result.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolExecution {
    /// Exact tool name.
    pub tool: String,
    /// Session revision after successful mutation.
    pub revision: u64,
    /// Bounded result value.
    pub value: Value,
}

/// Stateful, in-memory AI edit session over deterministic `FileMaker` IR.
pub struct FileMakerAiSession {
    pub(crate) document: Option<Arc<DocumentIr>>,
    scene: Option<Arc<ResolvedScene>>,
    pub(crate) limits: ResourceLimits,
    pub(crate) fonts: FontManager,
    pub(crate) assets: Option<Arc<dyn AssetResolver>>,
    pub(crate) policy: AiBridgePolicy,
    calls: usize,
    pub(crate) revision: u64,
}

impl FileMakerAiSession {
    /// Creates an empty session; `create` or `load` must run before stateful tools.
    pub fn empty(
        limits: ResourceLimits,
        fonts: FontManager,
        assets: Option<Arc<dyn AssetResolver>>,
        policy: AiBridgePolicy,
    ) -> BridgeResult<Self> {
        limits.validate()?;
        policy.validate()?;
        Ok(Self {
            document: None,
            scene: None,
            limits,
            fonts,
            assets,
            policy,
            calls: 0,
            revision: 0,
        })
    }

    /// Creates a session after the bound IR validates and renderable models resolve.
    pub fn new(
        document: DocumentIr,
        limits: ResourceLimits,
        fonts: FontManager,
        assets: Option<Arc<dyn AssetResolver>>,
        policy: AiBridgePolicy,
    ) -> BridgeResult<Self> {
        let mut session = Self::empty(limits, fonts, assets, policy)?;
        let scene = session.validate_document(&document)?;
        session.commit_document(document, scene);
        Ok(session)
    }

    /// Validates and executes one `appcore-ai` tool call.
    pub fn execute_call(&mut self, call: &AiToolCall) -> BridgeResult<ToolExecution> {
        self.execute(&call.name, &call.arguments_json)
    }

    /// Executes one declared tool with its exact closed, bounded JSON object arguments.
    pub fn execute(&mut self, name: &str, arguments_json: &str) -> BridgeResult<ToolExecution> {
        if !crate::tools::tool_definitions()
            .iter()
            .any(|definition| definition.name == name)
        {
            return Err(BridgeError::InvalidInput("unknown tool name"));
        }
        if !self.policy.allows(name) {
            return Err(BridgeError::Policy(format!("tool `{name}` is not allowed")));
        }
        self.calls = self
            .calls
            .checked_add(1)
            .ok_or_else(|| BridgeError::Policy("tool call accounting overflow".to_owned()))?;
        if self.calls > self.policy.max_tool_calls {
            return Err(BridgeError::Policy(
                "session tool call budget exhausted".to_owned(),
            ));
        }
        if arguments_json.len() > self.policy.max_argument_bytes {
            return Err(BridgeError::Policy(
                "tool arguments exceed the session byte budget".to_owned(),
            ));
        }
        let arguments: Value = serde_json::from_str(arguments_json).map_err(json_error)?;
        if !arguments.is_object() {
            return Err(BridgeError::InvalidInput(
                "tool arguments must be an object",
            ));
        }
        crate::tools::validate_arguments(name, &arguments)?;
        let value = match name {
            "filemaker_capabilities" => crate::query::capabilities(self),
            "filemaker_schema" => crate::query::schema(),
            "filemaker_create" => crate::mutation::create(self, &arguments),
            "filemaker_load" => crate::mutation::load(self, &arguments),
            "filemaker_add" => crate::mutation::add(self, &arguments),
            "filemaker_remove" => crate::mutation::remove(self, &arguments),
            "filemaker_clone" => crate::mutation::clone_element(self, &arguments),
            "filemaker_set" => crate::mutation::set(self, &arguments),
            "filemaker_patch" => crate::mutation::patch(self, &arguments),
            "filemaker_align" => crate::mutation::align(self, &arguments),
            "filemaker_place" => crate::mutation::place(self, &arguments),
            "filemaker_inspect" => crate::query::inspect(self, &arguments),
            "filemaker_explain" => crate::query::explain(self, &arguments),
            "filemaker_measure" => crate::query::measure(self, &arguments),
            "filemaker_validate" => crate::query::validate(self),
            "filemaker_preflight" => crate::query::preflight(self, &arguments),
            "filemaker_preview" => crate::query::preview(self, &arguments),
            "filemaker_debug_mask" => crate::query::debug_mask(self, &arguments),
            "filemaker_query_free_regions" => crate::query::free_regions(self, &arguments),
            "filemaker_export" => crate::query::export_artifact(self, &arguments),
            _ => return Err(BridgeError::InvalidInput("unknown tool name")),
        }?;
        enforce_result_limit(&value, self.policy.max_result_bytes)?;
        Ok(ToolExecution {
            tool: name.to_owned(),
            revision: self.revision,
            value,
        })
    }

    pub(crate) fn document(&self) -> BridgeResult<&DocumentIr> {
        self.document.as_deref().ok_or(BridgeError::NoDocument)
    }

    pub(crate) fn calls_used(&self) -> usize {
        self.calls
    }

    pub(crate) fn resolve(&self) -> BridgeResult<Arc<ResolvedScene>> {
        self.scene.clone().ok_or(BridgeError::InvalidInput(
            "document model has no resolved scene",
        ))
    }

    pub(crate) fn validate_document(
        &self,
        document: &DocumentIr,
    ) -> BridgeResult<Option<ResolvedScene>> {
        validate_ai_policy(document)?;
        validate_document_resources(document, &self.limits)?;
        PatchTransaction::validate(document)?;
        if document.model == ModelKind::Dataset {
            Ok(None)
        } else {
            self.resolve_document(document).map(Some)
        }
    }

    fn resolve_document(&self, document: &DocumentIr) -> BridgeResult<ResolvedScene> {
        let engine = LayoutEngine::new(&self.limits, &self.fonts, LayoutOptions::default())?;
        let scene = if let Some(assets) = self.assets.as_deref() {
            engine.with_assets(assets).resolve(document)
        } else {
            engine.resolve(document)
        }?;
        Ok(scene)
    }

    pub(crate) fn export_context(&self) -> ExportContext<'_> {
        ExportContext {
            limits: &self.limits,
            fonts: &self.fonts,
            assets: self.assets.as_deref(),
        }
    }

    pub(crate) fn apply_patch(&mut self, patch: &Patch) -> BridgeResult<()> {
        let maximum = self
            .policy
            .max_patch_operations
            .min(self.limits.max_patch_operations);
        if patch.operations.len() > maximum {
            return Err(BridgeError::Policy(
                "patch exceeds the AI small-patch operation budget".to_owned(),
            ));
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| BridgeError::Policy("session revision overflow".to_owned()))?;
        if patch.sequence != revision {
            return Err(BridgeError::Policy(
                "patch sequence must equal the next session revision".to_owned(),
            ));
        }
        self.enforce_edit_policy(&patch.operations)?;
        let mut candidate = self.take_document()?;
        let original = match PatchTransaction::new(&mut candidate, maximum)
            .apply_with_rollback_snapshot(patch)
        {
            Ok(original) => original,
            Err(error) => {
                self.document = Some(Arc::new(candidate));
                return Err(error.into());
            }
        };
        let scene = match self.validate_document(&candidate) {
            Ok(scene) => scene,
            Err(error) => {
                self.document = Some(Arc::new(original));
                return Err(error);
            }
        };
        self.commit_document(candidate, scene);
        self.revision = revision;
        Ok(())
    }

    pub(crate) fn commit_document(&mut self, document: DocumentIr, scene: Option<ResolvedScene>) {
        self.document = Some(Arc::new(document));
        self.scene = scene.map(Arc::new);
    }

    fn take_document(&mut self) -> BridgeResult<DocumentIr> {
        let document = self.document.take().ok_or(BridgeError::NoDocument)?;
        Ok(Arc::try_unwrap(document).unwrap_or_else(|document| (*document).clone()))
    }

    fn enforce_edit_policy(&self, operations: &[PatchOperation]) -> BridgeResult<()> {
        let document = self.document()?;
        let policy = &document.ai_policy;
        for operation in operations {
            match operation {
                PatchOperation::Remove { id } => {
                    enforce_id(policy, id.as_str())?;
                    let element = find_element(&document.elements, id.as_str())
                        .ok_or(BridgeError::InvalidInput("patch target was not found"))?;
                    enforce_destructive_subtree(policy, element)?;
                }
                PatchOperation::Replace { id, element } => {
                    enforce_id(policy, id.as_str())?;
                    let target = find_element(&document.elements, id.as_str())
                        .ok_or(BridgeError::InvalidInput("patch target was not found"))?;
                    enforce_destructive_subtree(policy, target)?;
                    enforce_new_subtree(policy, element)?;
                }
                PatchOperation::Add { parent, element } => {
                    if let Some(parent) = parent {
                        enforce_id(policy, parent.as_str())?;
                    }
                    enforce_new_subtree(policy, element)?;
                }
                PatchOperation::Clone { id, new_id } => {
                    enforce_id(policy, id.as_str())?;
                    let source = find_element(&document.elements, id.as_str())
                        .ok_or(BridgeError::InvalidInput("clone source was not found"))?;
                    enforce_clone_subtree(policy, source, id.as_str(), new_id.as_str())?;
                }
                PatchOperation::SetText { id, .. }
                | PatchOperation::SetHidden { id, .. }
                | PatchOperation::SetStyle { id, .. }
                | PatchOperation::Move { id, .. }
                | PatchOperation::Resize { id, .. } => {
                    enforce_id(policy, id.as_str())?;
                }
            }
        }
        Ok(())
    }
}

struct LimitedJsonCounter {
    remaining: usize,
    exceeded: bool,
}

impl LimitedJsonCounter {
    const fn new(limit: usize) -> Self {
        Self {
            remaining: limit,
            exceeded: false,
        }
    }
}

impl Write for LimitedJsonCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            self.exceeded = true;
            return Err(io::Error::other("serialized tool result exceeded limit"));
        }
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn enforce_result_limit(value: &Value, limit: usize) -> BridgeResult<()> {
    let mut counter = LimitedJsonCounter::new(limit);
    match serde_json::to_writer(&mut counter, value) {
        Ok(()) => Ok(()),
        Err(_) if counter.exceeded => Err(BridgeError::Policy(
            "serialized tool result exceeds policy byte limit".to_owned(),
        )),
        Err(error) => Err(json_error(error)),
    }
}

fn enforce_id(policy: &appcore_filemaker::AiPolicy, id: &str) -> BridgeResult<()> {
    if policy.locked.contains(id) || (!policy.editable.is_empty() && !policy.editable.contains(id))
    {
        return Err(BridgeError::Policy(format!(
            "element `{id}` is outside the template edit policy"
        )));
    }
    Ok(())
}

fn enforce_destructive_subtree(
    policy: &appcore_filemaker::AiPolicy,
    root: &ElementIr,
) -> BridgeResult<()> {
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        enforce_id(policy, element.id.as_str())?;
        if element.locked {
            return Err(BridgeError::Policy(format!(
                "element `{}` is locked inside the requested subtree",
                element.id.as_str()
            )));
        }
        stack.extend(element.children.iter().rev());
    }
    Ok(())
}

fn enforce_new_subtree(policy: &appcore_filemaker::AiPolicy, root: &ElementIr) -> BridgeResult<()> {
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        enforce_id(policy, element.id.as_str())?;
        stack.extend(element.children.iter().rev());
    }
    Ok(())
}

fn enforce_clone_subtree(
    policy: &appcore_filemaker::AiPolicy,
    root: &ElementIr,
    old_root: &str,
    new_root: &str,
) -> BridgeResult<()> {
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        let current = element.id.as_str();
        let suffix = current.strip_prefix(old_root).unwrap_or(current);
        enforce_id(policy, &format!("{new_root}{suffix}"))?;
        stack.extend(element.children.iter().rev());
    }
    Ok(())
}

fn find_element<'a>(elements: &'a [ElementIr], id: &str) -> Option<&'a ElementIr> {
    let mut stack: Vec<&ElementIr> = elements.iter().rev().collect();
    while let Some(element) = stack.pop() {
        if element.id.as_str() == id {
            return Some(element);
        }
        stack.extend(element.children.iter().rev());
    }
    None
}

fn validate_ai_policy(document: &DocumentIr) -> BridgeResult<()> {
    let policy = &document.ai_policy;
    if policy.purpose.len() > 1_024
        || policy.rules.len() > 64
        || policy.rules.iter().any(|rule| rule.len() > 1_024)
        || policy.editable.len() > 10_000
        || policy.locked.len() > 10_000
        || policy.editable.iter().any(|id| policy.locked.contains(id))
    {
        return Err(BridgeError::Policy(
            "document AI policy is oversized or contradictory".to_owned(),
        ));
    }
    for id in policy.editable.iter().chain(&policy.locked) {
        ElementId::new(id)?;
    }
    Ok(())
}

fn validate_document_resources(document: &DocumentIr, limits: &ResourceLimits) -> BridgeResult<()> {
    let mut stack: Vec<&ElementIr> = document.elements.iter().rev().collect();
    let mut elements = 0_usize;
    let mut path_commands = 0_usize;
    let mut rows = 0_u64;
    while let Some(element) = stack.pop() {
        elements = elements
            .checked_add(1)
            .ok_or_else(|| BridgeError::Policy("document element count overflow".to_owned()))?;
        path_commands = path_commands
            .checked_add(element.path.len())
            .ok_or_else(|| BridgeError::Policy("document path count overflow".to_owned()))?;
        if elements > limits.max_elements
            || path_commands > limits.max_path_commands
            || element
                .text
                .as_ref()
                .is_some_and(|text| text.len() > limits.max_text_bytes)
        {
            return Err(BridgeError::Policy(
                "document exceeds the session element, path, or text budget".to_owned(),
            ));
        }
        if let Some(table) = &element.table {
            rows = rows
                .checked_add(u64::try_from(table.rows.len()).unwrap_or(u64::MAX))
                .ok_or_else(|| BridgeError::Policy("document row count overflow".to_owned()))?;
            if rows > limits.max_rows {
                return Err(BridgeError::Policy(
                    "document exceeds the session row budget".to_owned(),
                ));
            }
            for row in &table.rows {
                for value in row.values() {
                    value.validate(64, limits.max_elements, limits.max_text_bytes)?;
                }
            }
        }
        stack.extend(element.children.iter().rev());
    }
    Ok(())
}
