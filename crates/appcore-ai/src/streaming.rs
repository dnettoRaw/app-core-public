// =============================================================================
//        #######
//     ###       ###     F: streaming.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/25 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/25 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use crate::{AiError, AiOutput, AiResponse, AiResult, CancellationToken};
use std::fmt::{Debug, Formatter};

/// One bounded incremental generative output event.
#[derive(Clone, PartialEq)]
pub enum AiStreamEvent {
    /// UTF-8 text appended to the response.
    TextDelta(String),
    /// Incremental fields for one tool call at a stable response index.
    ToolCallDelta {
        /// Zero-based tool-call index.
        index: usize,
        /// Backend correlation ID when first supplied.
        id: Option<String>,
        /// Declared tool name when first supplied.
        name: Option<String>,
        /// Raw UTF-8 arguments fragment; it may not be complete JSON yet.
        arguments_delta: String,
    },
}

impl Debug for AiStreamEvent {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TextDelta(value) => formatter
                .debug_struct("TextDelta")
                .field("redacted_text_bytes", &value.len())
                .finish(),
            Self::ToolCallDelta {
                index,
                id,
                name,
                arguments_delta,
            } => formatter
                .debug_struct("ToolCallDelta")
                .field("index", index)
                .field("id_bytes", &id.as_ref().map(String::len))
                .field("name", name)
                .field("redacted_argument_bytes", &arguments_delta.len())
                .finish(),
        }
    }
}

/// Synchronous bounded stream consumer.
///
/// Returning from `event` grants the producer permission to read and decode
/// the next chunk, which provides explicit backpressure without an executor or
/// unbounded channel dependency.
pub trait AiStreamSink: Send + Sync {
    /// Consumes one event without retaining references into the decoder.
    fn event(&self, event: &AiStreamEvent) -> AiResult<()>;
}

pub(crate) fn emit_complete(
    response: &AiResponse,
    cancellation: &CancellationToken,
    sink: &dyn AiStreamSink,
) -> AiResult<()> {
    if cancellation.is_cancelled() {
        return Err(AiError::Cancelled);
    }
    match &response.output {
        AiOutput::Text(value) => sink.event(&AiStreamEvent::TextDelta(value.clone())),
        AiOutput::ToolCalls(calls) => {
            for (index, call) in calls.iter().enumerate() {
                if cancellation.is_cancelled() {
                    return Err(AiError::Cancelled);
                }
                sink.event(&AiStreamEvent::ToolCallDelta {
                    index,
                    id: Some(call.id.clone()),
                    name: Some(call.name.clone()),
                    arguments_delta: call.arguments_json.clone(),
                })?;
            }
            Ok(())
        }
        _ => Err(AiError::Unsupported("streaming output type")),
    }
}
