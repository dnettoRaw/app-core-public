// =============================================================================
//        #######
//     ###       ###     F: openai_codec.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiContent, AiError, AiLimits, AiMessageRole, AiMetadata, AiOutput, AiRequest, AiResponse,
    AiResult, AiStructuredOutput, AiStructuredOutputFallback, AiToolCall, AiToolChoice,
    OpenAiCompatibilityProfile,
};
use base64::Engine;
use serde_json::{json, Map, Value};

pub(crate) fn encode(
    request: &AiRequest,
    server_model: &str,
    compatibility: &OpenAiCompatibilityProfile,
    structured_output_supported: bool,
    streaming: bool,
) -> AiResult<Vec<u8>> {
    let generation = &request.options.generation;
    let text_fallback = generation
        .structured_output
        .as_ref()
        .filter(|_| !structured_output_supported)
        .filter(|output| output.fallback == AiStructuredOutputFallback::JsonText);
    let mut root = Map::new();
    root.insert("model".to_string(), Value::String(server_model.to_string()));
    root.insert(
        "messages".to_string(),
        Value::Array(messages(request, text_fallback)?),
    );
    root.insert("stream".to_string(), Value::Bool(streaming));
    if streaming {
        root.insert(
            "stream_options".to_string(),
            json!({ "include_usage": true }),
        );
    }
    root.insert(
        compatibility.token_limit_field.name().to_string(),
        Value::from(u64::try_from(generation.max_output_tokens).unwrap_or(u64::MAX)),
    );
    if compatibility.send_temperature {
        root.insert(
            "temperature".to_string(),
            json!(f64::from(generation.temperature_milli) / 1_000.0),
        );
    }
    if compatibility.send_top_p {
        root.insert(
            "top_p".to_string(),
            json!(f64::from(generation.top_p_milli) / 1_000.0),
        );
    }
    if let Some(seed) = generation.seed {
        root.insert("seed".to_string(), Value::from(seed));
    }
    if !generation.stop_sequences.is_empty() {
        root.insert("stop".to_string(), json!(generation.stop_sequences));
    }
    if !generation.tools.is_empty() {
        root.insert("tools".to_string(), tools(request)?);
        root.insert(
            "tool_choice".to_string(),
            tool_choice(&generation.tool_choice),
        );
    }
    if structured_output_supported {
        if let Some(output) = &generation.structured_output {
            root.insert("response_format".to_string(), structured_output(output)?);
        }
    }
    for parameter in &compatibility.extra_parameters {
        let value = serde_json::from_str::<Value>(&parameter.value_json)
            .map_err(|_| AiError::InvalidInput("OpenAI compatibility parameter JSON"))?;
        root.insert(parameter.name.clone(), value);
    }
    serde_json::to_vec(&Value::Object(root))
        .map_err(|_| AiError::InvalidInput("OpenAI-compatible request JSON"))
}

fn messages(
    request: &AiRequest,
    text_fallback: Option<&AiStructuredOutput>,
) -> AiResult<Vec<Value>> {
    let mut messages = Vec::with_capacity(
        request
            .input
            .parts()
            .len()
            .saturating_add(usize::from(text_fallback.is_some())),
    );
    if let Some(output) = text_fallback {
        messages.push(json!({
            "role": "system",
            "content": format!(
                "Return only one JSON object matching the schema named {}. No markdown or commentary. JSON Schema: {}",
                output.name, output.schema
            ),
        }));
    }
    for part in request.input.parts() {
        match part {
            AiContent::Text(content) => messages.push(json!({
                "role": "user",
                "content": content,
            })),
            AiContent::Message(message) => {
                let mut value = Map::new();
                value.insert(
                    "role".to_string(),
                    Value::String(role(message.role).to_string()),
                );
                value.insert(
                    "content".to_string(),
                    Value::String(message.content.clone()),
                );
                if let Some(tool_call_id) = &message.tool_call_id {
                    value.insert(
                        "tool_call_id".to_string(),
                        Value::String(tool_call_id.clone()),
                    );
                }
                messages.push(Value::Object(value));
            }
            AiContent::Binary { media_type, bytes } if media_type.starts_with("image/") => {
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:{media_type};base64,{encoded}"),
                        },
                    }],
                }));
            }
            AiContent::Binary { .. } => {
                return Err(AiError::Unsupported("OpenAI chat adapter content modality"));
            }
        }
    }
    Ok(messages)
}

fn structured_output(output: &AiStructuredOutput) -> AiResult<Value> {
    let schema = serde_json::from_str::<Value>(&output.schema)
        .map_err(|_| AiError::InvalidInput("structured output JSON Schema"))?;
    if !schema.is_object() {
        return Err(AiError::InvalidInput(
            "structured output JSON Schema object",
        ));
    }
    Ok(json!({
        "type": "json_schema",
        "json_schema": {
            "name": output.name,
            "strict": output.strict,
            "schema": schema,
        },
    }))
}

fn role(role: AiMessageRole) -> &'static str {
    match role {
        AiMessageRole::System => "system",
        AiMessageRole::User => "user",
        AiMessageRole::Assistant => "assistant",
        AiMessageRole::Tool => "tool",
    }
}

fn tools(request: &AiRequest) -> AiResult<Value> {
    let values = request
        .options
        .generation
        .tools
        .iter()
        .map(|tool| {
            let schema = serde_json::from_str::<Value>(&tool.input_schema)
                .map_err(|_| AiError::InvalidInput("tool JSON Schema"))?;
            if !schema.is_object() {
                return Err(AiError::InvalidInput("tool JSON Schema object"));
            }
            Ok(json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": schema,
                },
            }))
        })
        .collect::<AiResult<Vec<_>>>()?;
    Ok(Value::Array(values))
}

fn tool_choice(choice: &AiToolChoice) -> Value {
    match choice {
        AiToolChoice::Auto => Value::String("auto".to_string()),
        AiToolChoice::None => Value::String("none".to_string()),
        AiToolChoice::Required => Value::String("required".to_string()),
        AiToolChoice::Named(name) => json!({
            "type": "function",
            "function": { "name": name },
        }),
    }
}

pub(crate) fn decode(body: &[u8], max_output_bytes: usize) -> AiResult<AiResponse> {
    let root = serde_json::from_slice::<Value>(body)
        .map_err(|_| AiError::Integrity("OpenAI-compatible response JSON"))?;
    let choices = root
        .get("choices")
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty() && values.len() <= 16)
        .ok_or(AiError::Integrity("OpenAI-compatible choices"))?;
    let choice = choices
        .first()
        .and_then(Value::as_object)
        .ok_or(AiError::Integrity("OpenAI-compatible choice"))?;
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or(AiError::Integrity("OpenAI-compatible message"))?;
    let calls = decode_tool_calls(message)?;
    let invalid_arguments = calls
        .iter()
        .filter(|call| !valid_argument_object(&call.arguments_json))
        .count();
    let output = if calls.is_empty() {
        AiOutput::Text(decode_text(message.get("content"))?)
    } else {
        AiOutput::ToolCalls(calls)
    };
    let mut metadata = metadata(&root, choice);
    if invalid_arguments > 0 {
        metadata.push(AiMetadata {
            key: "tool_calls.invalid_arguments".to_string(),
            value: invalid_arguments.to_string(),
        });
    }
    AiResponse::new(
        output,
        metadata,
        None,
        AiLimits {
            max_output_bytes,
            ..AiLimits::default()
        },
    )
}

fn decode_text(content: Option<&Value>) -> AiResult<String> {
    if let Some(text) = content.and_then(Value::as_str) {
        return Ok(text.to_string());
    }
    let parts = content
        .and_then(Value::as_array)
        .ok_or(AiError::Integrity("OpenAI-compatible text"))?;
    let mut text = String::new();
    for part in parts {
        let value = part
            .get("text")
            .and_then(Value::as_str)
            .ok_or(AiError::Integrity("OpenAI-compatible text part"))?;
        text.push_str(value);
    }
    Ok(text)
}

fn decode_tool_calls(message: &Map<String, Value>) -> AiResult<Vec<AiToolCall>> {
    let Some(values) = message.get("tool_calls") else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .filter(|values| !values.is_empty() && values.len() <= 32)
        .ok_or(AiError::Integrity("OpenAI-compatible tool calls"))?;
    values
        .iter()
        .map(|value| {
            let function = value
                .get("function")
                .ok_or(AiError::Integrity("OpenAI-compatible tool function"))?;
            Ok(AiToolCall {
                id: required_string(value, "id")?,
                name: required_string(function, "name")?,
                arguments_json: required_string(function, "arguments")?,
            })
        })
        .collect()
}

fn valid_argument_object(arguments: &str) -> bool {
    serde_json::from_str::<Value>(arguments).is_ok_and(|value| value.is_object())
}

fn required_string(value: &Value, key: &'static str) -> AiResult<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(AiError::Integrity(key))
}

fn metadata(root: &Value, choice: &Map<String, Value>) -> Vec<AiMetadata> {
    let mut metadata = Vec::new();
    if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
        if !reason.is_empty() && reason.len() <= 64 {
            metadata.push(AiMetadata {
                key: "finish_reason".to_string(),
                value: reason.to_string(),
            });
        }
    }
    if let Some(usage) = root.get("usage") {
        for (source, target) in [
            ("prompt_tokens", "usage.input_tokens"),
            ("completion_tokens", "usage.output_tokens"),
            ("total_tokens", "usage.total_tokens"),
        ] {
            if let Some(value) = usage.get(source).and_then(Value::as_u64) {
                metadata.push(AiMetadata {
                    key: target.to_string(),
                    value: value.to_string(),
                });
            }
        }
    }
    metadata
}
