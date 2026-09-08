// =============================================================================
//        #######
//     ###       ###     F: capabilities.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Borrow discovery context until its complete serialized result fits policy.

use appcore_filemaker::{AiPolicy, ModelKind, ResourceLimits};
use serde::Serialize;
use serde_json::Value;

use crate::{AiBridgePolicy, BridgeResult, FileMakerAiSession};

#[derive(Serialize)]
struct DocumentContext<'a> {
    template: &'a str,
    model: ModelKind,
    #[serde(flatten)]
    policy: &'a AiPolicy,
    root_elements: usize,
}

#[derive(Serialize)]
struct Capabilities<'a> {
    schema_version: &'static str,
    engine_version: &'static str,
    formats: &'static [&'static str],
    pdf_modes: &'static [&'static str],
    prepared_formats: &'static [&'static str],
    prepared_pdf: &'static [&'static str],
    mask_formats: &'static [&'static str],
    limits: &'a ResourceLimits,
    bridge_policy: &'a AiBridgePolicy,
    calls_used: usize,
    calls_remaining: usize,
    revision: u64,
    document_context: Option<DocumentContext<'a>>,
    r#loop: &'static [&'static str],
}

fn view(session: &FileMakerAiSession) -> Capabilities<'_> {
    Capabilities {
        schema_version: appcore_filemaker::FILEMAKER_SCHEMA_V1,
        engine_version: appcore_filemaker::ENGINE_VERSION,
        formats: &["pdf", "svg", "png", "jpeg", "html", "csv"],
        pdf_modes: &["editable", "flattened", "hybrid"],
        prepared_formats: &["webp", "xlsx", "zpl", "esc_pos", "pdf_a"],
        prepared_pdf: &["links", "bookmarks", "tagged_accessibility", "pdf_a"],
        mask_formats: &["json", "svg", "png", "pdf"],
        limits: &session.limits,
        bridge_policy: &session.policy,
        calls_used: session.calls_used(),
        calls_remaining: session
            .policy
            .max_tool_calls
            .saturating_sub(session.calls_used()),
        revision: session.revision,
        document_context: session.document.as_ref().map(|document| DocumentContext {
            template: &document.template_id,
            model: document.model,
            policy: &document.ai_policy,
            root_elements: document.elements.len(),
        }),
        r#loop: crate::recommended_tool_loop(),
    }
}

pub(crate) fn query(session: &FileMakerAiSession) -> BridgeResult<Value> {
    let result = view(session);
    crate::session::enforce_result_limit(&result, session.result_limit())?;
    serde_json::to_value(result).map_err(crate::error::json_error)
}
