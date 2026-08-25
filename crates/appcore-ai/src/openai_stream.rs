// =============================================================================
//        #######
//     ###       ###     F: openai_stream.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/25 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/25 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use crate::{
    AiError, AiLimits, AiMetadata, AiOutput, AiResponse, AiResult, AiStreamEvent, AiStreamSink,
    AiToolCall, CancellationToken, LimitKind, OpenAiTransportChunkSink,
};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

pub(crate) struct OpenAiSseDecoder<'a> {
    sink: &'a dyn AiStreamSink,
    cancellation: &'a CancellationToken,
    maximum: usize,
    received: usize,
    pending: Vec<u8>,
    text: String,
    calls: BTreeMap<usize, PartialToolCall>,
    metadata: Vec<AiMetadata>,
    done: bool,
}

impl<'a> OpenAiSseDecoder<'a> {
    pub(crate) fn new(
        sink: &'a dyn AiStreamSink,
        cancellation: &'a CancellationToken,
        maximum: usize,
    ) -> Self {
        Self {
            sink,
            cancellation,
            maximum,
            received: 0,
            pending: Vec::new(),
            text: String::new(),
            calls: BTreeMap::new(),
            metadata: Vec::new(),
            done: false,
        }
    }

    pub(crate) fn finish(mut self) -> AiResult<AiResponse> {
        if !self.pending.is_empty() {
            self.process_frame(self.pending.clone())?;
            self.pending.clear();
        }
        if self.text.is_empty() && self.calls.is_empty() {
            return Err(AiError::Integrity("OpenAI-compatible empty stream"));
        }
        let output = if self.calls.is_empty() {
            AiOutput::Text(self.text)
        } else {
            AiOutput::ToolCalls(self.complete_calls()?)
        };
        AiResponse::new(
            output,
            self.metadata,
            None,
            AiLimits {
                max_output_bytes: self.maximum,
                ..AiLimits::default()
            },
        )
    }

    fn process_available_frames(&mut self) -> AiResult<()> {
        while let Some((end, delimiter_length)) = next_frame(&self.pending) {
            let frame = self
                .pending
                .drain(..end + delimiter_length)
                .collect::<Vec<_>>();
            self.process_frame(frame)?;
        }
        Ok(())
    }

    fn process_frame(&mut self, frame: Vec<u8>) -> AiResult<()> {
        let text = std::str::from_utf8(&frame)
            .map_err(|_| AiError::Integrity("OpenAI-compatible stream UTF-8"))?;
        let mut data = String::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if let Some(value) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value.trim_start());
            }
        }
        if data.is_empty() {
            return Ok(());
        }
        if data == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        if self.done {
            return Err(AiError::Integrity(
                "OpenAI-compatible data after stream end",
            ));
        }
        let root = serde_json::from_str::<Value>(&data)
            .map_err(|_| AiError::Integrity("OpenAI-compatible stream JSON"))?;
        self.read_usage(&root);
        let Some(choices) = root.get("choices").and_then(Value::as_array) else {
            return Ok(());
        };
        if choices.is_empty() {
            return Ok(());
        }
        let choice = choices
            .first()
            .and_then(Value::as_object)
            .ok_or(AiError::Integrity("OpenAI-compatible stream choice"))?;
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            if !reason.is_empty() && reason.len() <= 64 {
                set_metadata(&mut self.metadata, "finish_reason", reason.to_string());
            }
        }
        let Some(delta) = choice.get("delta").and_then(Value::as_object) else {
            return Ok(());
        };
        if let Some(content) = delta.get("content").and_then(Value::as_str) {
            self.append_text(content)?;
        }
        if let Some(calls) = delta.get("tool_calls") {
            self.append_tool_calls(calls)?;
        }
        Ok(())
    }

    fn append_text(&mut self, delta: &str) -> AiResult<()> {
        check_output_bound(self.text.len(), delta.len(), self.maximum)?;
        self.text.push_str(delta);
        self.sink
            .event(&AiStreamEvent::TextDelta(delta.to_string()))
    }

    fn append_tool_calls(&mut self, value: &Value) -> AiResult<()> {
        let calls = value
            .as_array()
            .filter(|calls| !calls.is_empty() && calls.len() <= 32)
            .ok_or(AiError::Integrity("OpenAI-compatible stream tool calls"))?;
        for value in calls {
            let index = value
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value < 32)
                .ok_or(AiError::Integrity("OpenAI-compatible tool call index"))?;
            let id = optional_string(value, "id")?;
            let function = value.get("function").and_then(Value::as_object);
            let name = function
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str);
            let arguments = function
                .and_then(|value| value.get("arguments"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let current = self.calls.entry(index).or_default();
            merge_once(&mut current.id, id.as_deref(), "tool call id")?;
            merge_once(&mut current.name, name, "tool call name")?;
            check_output_bound(current.arguments.len(), arguments.len(), self.maximum)?;
            current.arguments.push_str(arguments);
            self.sink.event(&AiStreamEvent::ToolCallDelta {
                index,
                id,
                name: name.map(str::to_string),
                arguments_delta: arguments.to_string(),
            })?;
        }
        Ok(())
    }

    fn read_usage(&mut self, root: &Value) {
        let Some(usage) = root.get("usage") else {
            return;
        };
        for (source, target) in [
            ("prompt_tokens", "usage.input_tokens"),
            ("completion_tokens", "usage.output_tokens"),
            ("total_tokens", "usage.total_tokens"),
        ] {
            if let Some(value) = usage.get(source).and_then(Value::as_u64) {
                set_metadata(&mut self.metadata, target, value.to_string());
            }
        }
    }

    fn complete_calls(&mut self) -> AiResult<Vec<AiToolCall>> {
        let count = self.calls.len();
        let mut invalid = 0usize;
        let mut calls = Vec::with_capacity(count);
        for index in 0..count {
            let call = self
                .calls
                .remove(&index)
                .ok_or(AiError::Integrity("OpenAI-compatible tool call order"))?;
            if !serde_json::from_str::<Value>(&call.arguments).is_ok_and(|value| value.is_object())
            {
                invalid = invalid.saturating_add(1);
            }
            calls.push(AiToolCall {
                id: call.id.ok_or(AiError::Integrity("tool call id"))?,
                name: call.name.ok_or(AiError::Integrity("tool call name"))?,
                arguments_json: call.arguments,
            });
        }
        if invalid > 0 {
            set_metadata(
                &mut self.metadata,
                "tool_calls.invalid_arguments",
                invalid.to_string(),
            );
        }
        Ok(calls)
    }
}

impl OpenAiTransportChunkSink for OpenAiSseDecoder<'_> {
    fn chunk(&mut self, bytes: &[u8]) -> AiResult<()> {
        if self.cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        self.received = self.received.saturating_add(bytes.len());
        if self.received > self.maximum {
            return Err(AiError::LimitExceeded {
                kind: LimitKind::OutputBytes,
                actual: u64::try_from(self.received).unwrap_or(u64::MAX),
                limit: u64::try_from(self.maximum).unwrap_or(u64::MAX),
            });
        }
        self.pending.extend_from_slice(bytes);
        self.process_available_frames()
    }
}

fn next_frame(bytes: &[u8]) -> Option<(usize, usize)> {
    let line_feed = bytes.windows(2).position(|window| window == b"\n\n");
    let carriage_return = bytes.windows(4).position(|window| window == b"\r\n\r\n");
    match (line_feed, carriage_return) {
        (Some(left), Some(right)) if left < right => Some((left, 2)),
        (Some(_), Some(right)) | (None, Some(right)) => Some((right, 4)),
        (Some(left), None) => Some((left, 2)),
        (None, None) => None,
    }
}

fn optional_string(value: &Value, key: &'static str) -> AiResult<Option<String>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(AiError::Integrity(key)),
    }
}

fn merge_once(
    target: &mut Option<String>,
    value: Option<&str>,
    field: &'static str,
) -> AiResult<()> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if target.as_deref().is_some_and(|current| current != value) {
        return Err(AiError::Integrity(field));
    }
    if target.is_none() {
        *target = Some(value.to_string());
    }
    Ok(())
}

fn set_metadata(metadata: &mut Vec<AiMetadata>, key: &str, value: String) {
    if let Some(existing) = metadata.iter_mut().find(|entry| entry.key == key) {
        existing.value = value;
    } else {
        metadata.push(AiMetadata {
            key: key.to_string(),
            value,
        });
    }
}

fn check_output_bound(current: usize, added: usize, maximum: usize) -> AiResult<()> {
    let actual = current.saturating_add(added);
    if actual > maximum {
        return Err(AiError::LimitExceeded {
            kind: LimitKind::OutputBytes,
            actual: u64::try_from(actual).unwrap_or(u64::MAX),
            limit: u64::try_from(maximum).unwrap_or(u64::MAX),
        });
    }
    Ok(())
}
