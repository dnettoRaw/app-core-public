// =============================================================================
//        #######
//     ###       ###     F: response.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::request::check_limit;
use crate::{AiError, AiLimits, AiResult, AiToolCall, BackendId, DeviceId, LimitKind};

/// Label and finite score returned by a classifier.
#[derive(Clone, PartialEq)]
pub struct AiScore {
    /// Bounded label text.
    pub label: String,
    /// Backend-defined finite score; it is not automatically a probability.
    pub score: f32,
}

/// Backend-neutral response content.
#[derive(Clone, PartialEq)]
pub enum AiOutput {
    /// Generated or transformed UTF-8 text.
    Text(String),
    /// Bounded classifier labels and scores.
    Scores(Vec<AiScore>),
    /// Numeric embedding values.
    Embedding(Vec<f32>),
    /// Structured tool calls returned by a generative model.
    ToolCalls(Vec<AiToolCall>),
    /// Opaque bounded multimodal output.
    Binary {
        /// ASCII output media type.
        media_type: String,
        /// Opaque output bytes.
        bytes: Vec<u8>,
    },
}

impl AiOutput {
    fn byte_len(&self) -> usize {
        match self {
            Self::Text(value) => value.len(),
            Self::Scores(scores) => scores.iter().fold(0usize, |total, score| {
                total.saturating_add(score.label.len()).saturating_add(4)
            }),
            Self::Embedding(values) => values.len().saturating_mul(4),
            Self::ToolCalls(calls) => calls
                .iter()
                .fold(0usize, |total, call| total.saturating_add(call.byte_len())),
            Self::Binary { media_type, bytes } => media_type.len().saturating_add(bytes.len()),
        }
    }

    fn validate(&self) -> AiResult<()> {
        match self {
            Self::Text(_) => Ok(()),
            Self::Scores(scores) => {
                if scores.is_empty()
                    || scores.iter().any(|score| {
                        score.label.is_empty() || score.label.len() > 96 || !score.score.is_finite()
                    })
                {
                    return Err(AiError::InvalidInput("classifier output"));
                }
                Ok(())
            }
            Self::Embedding(values) => {
                if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
                    return Err(AiError::InvalidInput("embedding output"));
                }
                Ok(())
            }
            Self::ToolCalls(calls) => {
                if calls.is_empty() || calls.len() > 32 {
                    return Err(AiError::InvalidInput("tool call output"));
                }
                for call in calls {
                    call.validate()?;
                }
                Ok(())
            }
            Self::Binary { media_type, bytes } => {
                if media_type.is_empty()
                    || media_type.len() > 96
                    || bytes.is_empty()
                    || !media_type.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'+' | b'-')
                    })
                {
                    return Err(AiError::InvalidInput("binary output"));
                }
                Ok(())
            }
        }
    }
}

/// Bounded non-sensitive response metadata.
#[derive(Clone, Eq, PartialEq)]
pub struct AiMetadata {
    /// Validated metadata key.
    pub key: String,
    /// Redacted metadata value.
    pub value: String,
}

/// Structured reason for selecting or rejecting a route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteReason {
    /// A deterministic lightweight resolver satisfied the request.
    LightweightSatisfied,
    /// A required override selected the route.
    ForcedOverride,
    /// The route had the lowest admitted cost.
    LowestAdmittedCost,
    /// A prior bounded attempt requested escalation.
    Escalated,
    /// Local privacy policy excluded remote candidates.
    PrivacyRestricted,
    /// Resource admission excluded another candidate.
    ResourceRestricted,
    /// No compatible route was available.
    NoCompatibleRoute,
}

/// Safe description of a selected execution target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionTarget {
    /// Deterministic in-process resolver.
    Lightweight,
    /// Local backend and device.
    Local {
        /// Selected backend.
        backend: BackendId,
        /// Selected device.
        device: DeviceId,
    },
    /// Authenticated remote compute without exposing network coordinates.
    Remote {
        /// Bounded opaque peer class rather than a high-cardinality peer ID.
        peer_class: String,
        /// Selected backend.
        backend: BackendId,
        /// Selected device.
        device: DeviceId,
    },
}

/// One safe routing attempt in an execution decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionAttempt {
    /// Attempt number starting at one.
    pub sequence: usize,
    /// Candidate target.
    pub target: ExecutionTarget,
    /// Structured selection or escalation reason.
    pub reason: RouteReason,
    /// Backend-neutral estimated cost units.
    pub estimated_cost_units: u64,
}

/// Optional redacted explanation of a completed route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionDecision {
    /// Selected execution target.
    pub selected: ExecutionTarget,
    /// Primary structured reason.
    pub reason: RouteReason,
    /// Bounded attempts considered or executed.
    pub attempts: Vec<ExecutionAttempt>,
}

/// Validated AI response.
#[derive(Clone, PartialEq)]
pub struct AiResponse {
    /// Backend-neutral response content.
    pub output: AiOutput,
    /// Bounded non-sensitive metadata.
    pub metadata: Vec<AiMetadata>,
    /// Optional safe route diagnostics.
    pub decision: Option<ExecutionDecision>,
}

impl AiResponse {
    /// Validates response content and metadata against explicit limits.
    pub fn new(
        output: AiOutput,
        metadata: Vec<AiMetadata>,
        decision: Option<ExecutionDecision>,
        limits: AiLimits,
    ) -> AiResult<Self> {
        output.validate()?;
        check_limit(
            output.byte_len(),
            limits.max_output_bytes,
            LimitKind::OutputBytes,
        )?;
        check_limit(
            metadata.len(),
            limits.max_metadata_entries,
            LimitKind::MetadataEntries,
        )?;
        for entry in &metadata {
            check_limit(
                entry.key.len(),
                limits.max_metadata_key_bytes,
                LimitKind::MetadataKeyBytes,
            )?;
            check_limit(
                entry.value.len(),
                limits.max_metadata_value_bytes,
                LimitKind::MetadataValueBytes,
            )?;
            if entry.key.is_empty()
                || !entry
                    .key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            {
                return Err(AiError::InvalidInput("response metadata key"));
            }
        }
        if let Some(decision) = &decision {
            check_limit(
                decision.attempts.len(),
                limits.max_attempts,
                LimitKind::Attempts,
            )?;
        }
        Ok(Self {
            output,
            metadata,
            decision,
        })
    }
}
