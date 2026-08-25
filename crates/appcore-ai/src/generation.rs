// =============================================================================
//        #######
//     ###       ###     F: generation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiError, AiResult};

/// Role assigned to one text message in a generative conversation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiMessageRole {
    /// Instructions owned by the application.
    System,
    /// Content supplied by the user or calling application.
    User,
    /// Prior model output supplied as conversation context.
    Assistant,
    /// Result of a previously requested tool invocation.
    Tool,
}

/// One bounded role-aware text message.
#[derive(Clone, PartialEq)]
pub struct AiMessage {
    /// Conversation role.
    pub role: AiMessageRole,
    /// UTF-8 message content.
    pub content: String,
    /// Correlation ID required for a tool-result message.
    pub tool_call_id: Option<String>,
}

impl AiMessage {
    /// Creates a system, user, or assistant message.
    pub fn new(role: AiMessageRole, content: impl Into<String>) -> AiResult<Self> {
        let message = Self {
            role,
            content: content.into(),
            tool_call_id: None,
        };
        message.validate()?;
        Ok(message)
    }

    /// Creates a result correlated with one prior tool call.
    pub fn tool_result(
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
    ) -> AiResult<Self> {
        let message = Self {
            role: AiMessageRole::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
        };
        message.validate()?;
        Ok(message)
    }

    pub(crate) fn validate(&self) -> AiResult<()> {
        if self.content.is_empty()
            || self
                .tool_call_id
                .as_ref()
                .is_some_and(|value| value.is_empty() || value.len() > 128)
            || (self.role == AiMessageRole::Tool) != self.tool_call_id.is_some()
        {
            return Err(AiError::InvalidInput("chat message"));
        }
        Ok(())
    }
}

impl std::fmt::Debug for AiMessage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AiMessage")
            .field("role", &self.role)
            .field("redacted_content_bytes", &self.content.len())
            .field(
                "tool_call_id_bytes",
                &self.tool_call_id.as_ref().map(String::len),
            )
            .finish()
    }
}

/// Bounded tool declaration transported to a capable generative backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiToolDefinition {
    /// Stable ASCII tool name.
    pub name: String,
    /// Bounded human-readable purpose.
    pub description: String,
    /// JSON Schema object encoded as UTF-8 JSON.
    pub input_schema: String,
}

impl AiToolDefinition {
    fn validate(&self) -> AiResult<()> {
        if !valid_name(&self.name)
            || self.description.is_empty()
            || self.description.len() > 1_024
            || self.input_schema.is_empty()
            || self.input_schema.len() > 16 * 1_024
        {
            return Err(AiError::InvalidInput("generative tool"));
        }
        Ok(())
    }
}

/// Tool-selection policy requested from a capable generative backend.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum AiToolChoice {
    /// Let the model choose between text and a declared tool.
    #[default]
    Auto,
    /// Do not allow a tool invocation for this request.
    None,
    /// Require one of the declared tools without forcing its name.
    Required,
    /// Require one exact declared tool.
    Named(String),
}

/// Caller-selected behavior when exact JSON Schema output is unavailable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AiStructuredOutputFallback {
    /// Reject the request instead of weakening its output contract.
    #[default]
    Reject,
    /// Request bounded JSON text and leave schema validation to the application.
    JsonText,
}

/// Bounded JSON Schema output requested from a generative backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiStructuredOutput {
    /// Stable schema name transported to compatible providers.
    pub name: String,
    /// JSON Schema object encoded as UTF-8 JSON.
    pub schema: String,
    /// Whether a supporting provider must enforce strict schema adherence.
    pub strict: bool,
    /// Explicit behavior when the selected provider lacks JSON Schema support.
    pub fallback: AiStructuredOutputFallback,
}

impl AiStructuredOutput {
    fn validate(&self) -> AiResult<()> {
        if !valid_name(&self.name) || self.schema.is_empty() || self.schema.len() > 16 * 1_024 {
            return Err(AiError::InvalidInput("structured output"));
        }
        Ok(())
    }
}

/// Backend-neutral bounded text-generation controls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiGenerationOptions {
    /// Maximum number of new tokens requested from the backend.
    pub max_output_tokens: usize,
    /// Temperature in thousandths, from zero through 2,000.
    pub temperature_milli: u16,
    /// Nucleus probability in thousandths, from one through 1,000.
    pub top_p_milli: u16,
    /// Optional deterministic seed when the backend supports it.
    pub seed: Option<u64>,
    /// Bounded stop strings.
    pub stop_sequences: Vec<String>,
    /// Bounded tool declarations.
    pub tools: Vec<AiToolDefinition>,
    /// Requested tool-selection policy.
    pub tool_choice: AiToolChoice,
    /// Optional JSON Schema output contract.
    pub structured_output: Option<AiStructuredOutput>,
}

impl AiGenerationOptions {
    /// Validates generation controls before routing reaches any backend.
    pub fn validate(&self) -> AiResult<()> {
        if self.max_output_tokens == 0
            || self.max_output_tokens > 65_536
            || self.temperature_milli > 2_000
            || self.top_p_milli == 0
            || self.top_p_milli > 1_000
            || self.stop_sequences.len() > 8
            || self
                .stop_sequences
                .iter()
                .any(|value| value.is_empty() || value.len() > 256)
            || self.tools.len() > 32
        {
            return Err(AiError::InvalidInput("generation options"));
        }
        for tool in &self.tools {
            tool.validate()?;
        }
        if let Some(output) = &self.structured_output {
            output.validate()?;
        }
        if let AiToolChoice::Named(name) = &self.tool_choice {
            if !valid_name(name) || !self.tools.iter().any(|tool| &tool.name == name) {
                return Err(AiError::InvalidInput("tool choice"));
            }
        }
        if self.tools.is_empty()
            && matches!(
                self.tool_choice,
                AiToolChoice::Required | AiToolChoice::Named(_)
            )
        {
            return Err(AiError::InvalidInput("tool choice without tools"));
        }
        if !self.tools.is_empty() && self.structured_output.is_some() {
            return Err(AiError::InvalidInput("tools with structured output"));
        }
        Ok(())
    }
}

impl Default for AiGenerationOptions {
    fn default() -> Self {
        Self {
            max_output_tokens: 512,
            temperature_milli: 700,
            top_p_milli: 1_000,
            seed: None,
            stop_sequences: Vec::new(),
            tools: Vec::new(),
            tool_choice: AiToolChoice::Auto,
            structured_output: None,
        }
    }
}

/// One bounded tool call returned by a generative backend.
#[derive(Clone, PartialEq)]
pub struct AiToolCall {
    /// Backend-assigned bounded correlation ID.
    pub id: String,
    /// Exact declared tool name.
    pub name: String,
    /// UTF-8 JSON arguments; applications must validate against their schema.
    pub arguments_json: String,
}

impl AiToolCall {
    pub(crate) fn validate(&self) -> AiResult<()> {
        if self.id.is_empty()
            || self.id.len() > 128
            || !valid_name(&self.name)
            || self.arguments_json.is_empty()
        {
            return Err(AiError::InvalidInput("tool call output"));
        }
        Ok(())
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.id
            .len()
            .saturating_add(self.name.len())
            .saturating_add(self.arguments_json.len())
    }
}

impl std::fmt::Debug for AiToolCall {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AiToolCall")
            .field("id_bytes", &self.id.len())
            .field("name", &self.name)
            .field("redacted_argument_bytes", &self.arguments_json.len())
            .finish()
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}
