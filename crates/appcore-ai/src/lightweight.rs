// =============================================================================
//        #######
//     ###       ###     F: lightweight.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiLimits, AiMetadata, AiOutput, AiRequest, AiResponse, AiResult, AiScore, AiTask,
    CancellationToken, ExecutionAttempt, ExecutionDecision, ExecutionTarget, RouteReason,
};

/// Semantics of a deterministic text rule match.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleMatch {
    /// Complete case-sensitive equality.
    Exact,
    /// Case-sensitive input prefix.
    Prefix,
    /// Case-sensitive contained fragment.
    Contains,
}

/// One bounded classification, decision or extraction rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextRule {
    /// Stable reason label returned to the caller.
    pub label: String,
    /// Pattern used by the declared matching semantics.
    pub pattern: String,
    /// Bounded result text.
    pub output: String,
    /// Explicit matching semantics.
    pub matching: RuleMatch,
}

/// Certainty semantics for a lightweight result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightweightCertainty {
    /// Exact deterministic result; no probabilistic confidence is implied.
    Certain,
    /// Explicit match score from zero through ten thousand basis points.
    HeuristicScore(u16),
}

/// Structured lightweight-path reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightweightReason {
    /// Whitespace normalization fully satisfied the request.
    NormalizedText,
    /// An exact configured rule matched.
    ExactRule,
    /// A prefix configured rule matched.
    PrefixRule,
    /// A contained-fragment configured rule matched.
    ContainsRule,
    /// The task or input shape is unsupported by the lightweight engine.
    Unsupported,
    /// No configured rule matched.
    NoRuleMatched,
}

/// Result of asking a lightweight resolver to handle a request.
#[derive(Clone, Debug, PartialEq)]
pub enum LightweightOutcome {
    /// The request produced a bounded response.
    Handled {
        /// Validated response.
        response: AiResponse,
        /// Exact or explicitly defined heuristic certainty.
        certainty: LightweightCertainty,
        /// Structured reason.
        reason: LightweightReason,
        /// Measured deterministic work units.
        cost_units: u64,
        /// Whether the caller should continue to a model route.
        escalate: bool,
    },
    /// The resolver cannot satisfy the request.
    NotHandled {
        /// Structured non-match reason.
        reason: LightweightReason,
    },
}

/// Boundary queried by the main router before selecting a model.
pub trait LightweightResolver: Send + Sync {
    /// Reports whether the task and input shape are supported.
    fn can_handle(&self, request: &AiRequest) -> bool;

    /// Resolves one request without a model runtime.
    fn resolve(
        &self,
        request: &AiRequest,
        cancellation: &CancellationToken,
    ) -> AiResult<LightweightOutcome>;
}

/// Dependency-free bounded resolver for text normalization and configured rules.
#[derive(Clone, Debug)]
pub struct LightweightEngine {
    rules: Vec<TextRule>,
    limits: AiLimits,
    escalation_threshold_basis_points: u16,
}

impl LightweightEngine {
    /// Builds an engine after validating rule count and content bounds.
    pub fn new(
        rules: Vec<TextRule>,
        limits: AiLimits,
        escalation_threshold_basis_points: u16,
    ) -> AiResult<Self> {
        if rules.len() > 256 || escalation_threshold_basis_points > 10_000 {
            return Err(AiError::InvalidInput("lightweight engine bounds"));
        }
        for rule in &rules {
            if rule.label.is_empty()
                || rule.label.len() > 64
                || rule.pattern.is_empty()
                || rule.pattern.len() > 256
                || rule.output.len() > limits.max_output_bytes
            {
                return Err(AiError::InvalidInput("lightweight rule bounds"));
            }
        }
        Ok(Self {
            rules,
            limits,
            escalation_threshold_basis_points,
        })
    }

    fn normalize(&self, request: &AiRequest) -> AiResult<LightweightOutcome> {
        let text = request
            .input
            .single_text()
            .ok_or(AiError::InvalidInput("lightweight text input"))?;
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let response = response(
            request,
            AiOutput::Text(normalized),
            "text.normalized",
            RouteReason::LightweightSatisfied,
            self.limits,
        )?;
        Ok(LightweightOutcome::Handled {
            response,
            certainty: LightweightCertainty::Certain,
            reason: LightweightReason::NormalizedText,
            cost_units: u64::try_from(text.len()).unwrap_or(u64::MAX),
            escalate: false,
        })
    }

    fn apply_rule(&self, request: &AiRequest) -> AiResult<LightweightOutcome> {
        let Some(text) = request.input.single_text() else {
            return Ok(LightweightOutcome::NotHandled {
                reason: LightweightReason::Unsupported,
            });
        };
        let Some((rule, score)) = self
            .rules
            .iter()
            .filter_map(|rule| rule_score(rule, text).map(|score| (rule, score)))
            .max_by_key(|(_, score)| *score)
        else {
            return Ok(LightweightOutcome::NotHandled {
                reason: LightweightReason::NoRuleMatched,
            });
        };
        let certainty = if rule.matching == RuleMatch::Exact {
            LightweightCertainty::Certain
        } else {
            LightweightCertainty::HeuristicScore(score)
        };
        let output = match &request.task {
            AiTask::ClassifyText => AiOutput::Scores(vec![AiScore {
                label: rule.output.clone(),
                score: f32::from(score) / 10_000.0,
            }]),
            _ => AiOutput::Text(rule.output.clone()),
        };
        let reason = match rule.matching {
            RuleMatch::Exact => LightweightReason::ExactRule,
            RuleMatch::Prefix => LightweightReason::PrefixRule,
            RuleMatch::Contains => LightweightReason::ContainsRule,
        };
        let response = response(
            request,
            output,
            &rule.label,
            RouteReason::LightweightSatisfied,
            self.limits,
        )?;
        Ok(LightweightOutcome::Handled {
            response,
            certainty,
            reason,
            cost_units: u64::try_from(text.len().saturating_add(rule.pattern.len()))
                .unwrap_or(u64::MAX),
            escalate: score < self.escalation_threshold_basis_points,
        })
    }
}

impl LightweightResolver for LightweightEngine {
    fn can_handle(&self, request: &AiRequest) -> bool {
        request.input.single_text().is_some()
            && matches!(
                &request.task,
                AiTask::TransformText
                    | AiTask::ClassifyText
                    | AiTask::Extract
                    | AiTask::Decide
                    | AiTask::Capability(_)
            )
    }

    fn resolve(
        &self,
        request: &AiRequest,
        cancellation: &CancellationToken,
    ) -> AiResult<LightweightOutcome> {
        if cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        if !self.can_handle(request) {
            return Ok(LightweightOutcome::NotHandled {
                reason: LightweightReason::Unsupported,
            });
        }
        if request.task == AiTask::TransformText {
            self.normalize(request)
        } else {
            self.apply_rule(request)
        }
    }
}

fn rule_score(rule: &TextRule, text: &str) -> Option<u16> {
    let matches = match rule.matching {
        RuleMatch::Exact => text == rule.pattern,
        RuleMatch::Prefix => text.starts_with(&rule.pattern),
        RuleMatch::Contains => text.contains(&rule.pattern),
    };
    if !matches {
        return None;
    }
    if rule.matching == RuleMatch::Exact {
        return Some(10_000);
    }
    let numerator = rule.pattern.len().saturating_mul(10_000);
    let denominator = text.len().max(1);
    Some(u16::try_from((numerator / denominator).min(10_000)).unwrap_or(10_000))
}

fn response(
    request: &AiRequest,
    output: AiOutput,
    reason: &str,
    route_reason: RouteReason,
    limits: AiLimits,
) -> AiResult<AiResponse> {
    let metadata = vec![AiMetadata {
        key: "reason".into(),
        value: reason.into(),
    }];
    let decision = request
        .options
        .include_diagnostics
        .then(|| ExecutionDecision {
            selected: ExecutionTarget::Lightweight,
            reason: route_reason,
            attempts: vec![ExecutionAttempt {
                sequence: 1,
                target: ExecutionTarget::Lightweight,
                reason: route_reason,
                estimated_cost_units: 0,
            }],
        });
    AiResponse::new(output, metadata, decision, limits)
}
