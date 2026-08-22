// =============================================================================
//        #######
//     ###       ###     F: request.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiExecutionMode, AiMessage, AiModality, AiOptions, AiPrivacyMode, AiResult,
    CapabilityId, LimitKind,
};

/// Maximum accepted inputs and outputs for one resolution operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiLimits {
    /// Maximum sum of request content bytes.
    pub max_input_bytes: usize,
    /// Maximum number of request content parts.
    pub max_input_parts: usize,
    /// Maximum response content bytes.
    pub max_output_bytes: usize,
    /// Maximum response metadata entries.
    pub max_metadata_entries: usize,
    /// Maximum metadata key bytes.
    pub max_metadata_key_bytes: usize,
    /// Maximum metadata value bytes.
    pub max_metadata_value_bytes: usize,
    /// Maximum routing and escalation attempts.
    pub max_attempts: usize,
}

impl Default for AiLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 1024 * 1024,
            max_input_parts: 16,
            max_output_bytes: 1024 * 1024,
            max_metadata_entries: 32,
            max_metadata_key_bytes: 64,
            max_metadata_value_bytes: 256,
            max_attempts: 3,
        }
    }
}

/// A standard or explicitly named AI capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiTask {
    /// Generate bounded text from the supplied input.
    GenerateText,
    /// Continue a bounded role-aware conversation.
    Chat,
    /// Transform text without changing its essential meaning.
    TransformText,
    /// Assign one or more bounded labels.
    ClassifyText,
    /// Extract a bounded fragment or field.
    Extract,
    /// Produce a deterministic or model-assisted decision.
    Decide,
    /// Produce a numeric embedding.
    Embed,
    /// Analyze one or more image parts and return a bounded result.
    AnalyzeImage,
    /// Analyze a bounded document such as PDF without assuming its extraction engine.
    AnalyzeDocument,
    /// A validated capability owned by a consumer.
    Capability(CapabilityId),
}

/// One bounded request content part.
#[derive(Clone, PartialEq)]
pub enum AiContent {
    /// UTF-8 text content.
    Text(String),
    /// One role-aware text message for a generative conversation.
    Message(AiMessage),
    /// Opaque future multimodal content with a validated media type.
    Binary {
        /// ASCII media type such as `image/png`.
        media_type: String,
        /// Opaque bounded bytes.
        bytes: Vec<u8>,
    },
}

impl AiContent {
    fn byte_len(&self) -> usize {
        match self {
            Self::Text(value) => value.len(),
            Self::Message(message) => message.content.len(),
            Self::Binary { media_type, bytes } => media_type.len().saturating_add(bytes.len()),
        }
    }

    /// Returns the coarse modality without decoding opaque bytes.
    #[must_use]
    pub fn modality(&self) -> AiModality {
        match self {
            Self::Text(_) => AiModality::Text,
            Self::Message(_) => AiModality::Text,
            Self::Binary { media_type, .. } => AiModality::from_media_type(media_type),
        }
    }

    fn validate(&self) -> AiResult<()> {
        match self {
            Self::Text(value) if value.is_empty() => Err(AiError::InvalidInput("empty text")),
            Self::Message(message) => message.validate(),
            Self::Binary { media_type, bytes } => {
                if media_type.is_empty()
                    || media_type.len() > 96
                    || !media_type.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'+' | b'-')
                    })
                {
                    return Err(AiError::InvalidInput("binary media type"));
                }
                if bytes.is_empty() {
                    return Err(AiError::InvalidInput("empty binary content"));
                }
                Ok(())
            }
            Self::Text(_) => Ok(()),
        }
    }
}

/// Validated collection of input content parts.
#[derive(Clone, PartialEq)]
pub struct AiInput {
    parts: Vec<AiContent>,
}

impl AiInput {
    /// Validates content parts against request limits.
    pub fn new(parts: Vec<AiContent>, limits: AiLimits) -> AiResult<Self> {
        validate_parts(&parts, limits)?;
        Ok(Self { parts })
    }

    fn validate(&self, limits: AiLimits) -> AiResult<()> {
        validate_parts(&self.parts, limits)
    }

    /// Builds one text input with the supplied limits.
    pub fn text(text: impl Into<String>, limits: AiLimits) -> AiResult<Self> {
        Self::new(vec![AiContent::Text(text.into())], limits)
    }

    /// Returns the validated content parts.
    #[must_use]
    pub fn parts(&self) -> &[AiContent] {
        &self.parts
    }

    /// Returns the only text part when the input contains exactly one.
    #[must_use]
    pub fn single_text(&self) -> Option<&str> {
        match self.parts.as_slice() {
            [AiContent::Text(text)] => Some(text),
            _ => None,
        }
    }

    /// Returns the deduplicated modalities in stable order.
    #[must_use]
    pub fn modalities(&self) -> Vec<AiModality> {
        let mut modalities = self
            .parts
            .iter()
            .map(AiContent::modality)
            .collect::<Vec<_>>();
        modalities.sort_unstable();
        modalities.dedup();
        modalities
    }
}

/// One validated AI resolution request.
#[derive(Clone, PartialEq)]
pub struct AiRequest {
    /// Requested capability.
    pub task: AiTask,
    /// Validated request content.
    pub input: AiInput,
    /// Routing and resource policy.
    pub options: AiOptions,
}

impl AiRequest {
    /// Builds a text request using explicit limits.
    pub fn text(task: AiTask, text: impl Into<String>, limits: AiLimits) -> AiResult<Self> {
        Ok(Self {
            task,
            input: AiInput::text(text, limits)?,
            options: AiOptions::default(),
        })
    }

    /// Builds a role-aware generative conversation using explicit limits.
    pub fn chat(messages: impl IntoIterator<Item = AiMessage>, limits: AiLimits) -> AiResult<Self> {
        let parts = messages.into_iter().map(AiContent::Message).collect();
        Ok(Self {
            task: AiTask::Chat,
            input: AiInput::new(parts, limits)?,
            options: AiOptions::default(),
        })
    }

    /// Revalidates cross-field privacy and distribution rules.
    pub fn validate(&self, limits: AiLimits) -> AiResult<()> {
        self.input.validate(limits)?;
        self.options.generation.validate()?;
        if !matches!(self.task, AiTask::GenerateText | AiTask::Chat)
            && !self.options.generation.tools.is_empty()
        {
            return Err(AiError::InvalidInput("tools require generative task"));
        }
        let modalities = self.input.modalities();
        if self.task == AiTask::AnalyzeImage && !modalities.contains(&AiModality::Image) {
            return Err(AiError::InvalidInput(
                "image analysis requires image content",
            ));
        }
        if self.task == AiTask::AnalyzeDocument && !modalities.contains(&AiModality::Document) {
            return Err(AiError::InvalidInput(
                "document analysis requires document content",
            ));
        }
        if self.options.privacy == AiPrivacyMode::LocalOnly
            && (self.options.execution == AiExecutionMode::Swarm
                || self.options.distribution.allow_remote_compute
                || self.options.distribution.allow_remote_storage)
        {
            return Err(AiError::InvalidInput(
                "local-only request permits remote resources",
            ));
        }
        if self.options.execution == AiExecutionMode::Local
            && self.options.distribution.allow_remote_compute
        {
            return Err(AiError::InvalidInput(
                "local execution permits remote compute",
            ));
        }
        if self.options.distribution.max_peers == 0 {
            return Err(AiError::InvalidInput("max peers"));
        }
        if let Some(authorization) = &self.options.authorization {
            authorization.validate()?;
        }
        if (self.options.execution == AiExecutionMode::Swarm
            || self.options.distribution.allow_remote_compute)
            && !self
                .options
                .authorization
                .as_ref()
                .is_some_and(|authorization| authorization.allows(crate::REMOTE_COMPUTE_GRANT))
        {
            return Err(AiError::Unauthorized);
        }
        if self.options.distribution.allow_remote_storage
            && !self
                .options
                .authorization
                .as_ref()
                .is_some_and(|authorization| authorization.allows(crate::REMOTE_STORAGE_GRANT))
        {
            return Err(AiError::Unauthorized);
        }
        Ok(())
    }
}

fn validate_parts(parts: &[AiContent], limits: AiLimits) -> AiResult<()> {
    check_limit(parts.len(), limits.max_input_parts, LimitKind::InputParts)?;
    if parts.is_empty() {
        return Err(AiError::InvalidInput("request has no content"));
    }
    let mut bytes = 0usize;
    for part in parts {
        part.validate()?;
        bytes = bytes.saturating_add(part.byte_len());
    }
    check_limit(bytes, limits.max_input_bytes, LimitKind::InputBytes)
}

pub(crate) fn check_limit(actual: usize, limit: usize, kind: LimitKind) -> AiResult<()> {
    if actual <= limit {
        return Ok(());
    }
    Err(AiError::LimitExceeded {
        kind,
        actual: u64::try_from(actual).unwrap_or(u64::MAX),
        limit: u64::try_from(limit).unwrap_or(u64::MAX),
    })
}
