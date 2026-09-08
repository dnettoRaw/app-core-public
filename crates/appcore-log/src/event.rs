// =============================================================================
//        #######
//     ###       ###     F: event.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Event values deliberately kept independent from sinks and process hosting.

use crate::Sensitivity;
use serde::{Deserialize, Serialize};

/// Maximum UTF-8 bytes retained in one message or identity-like field.
pub const MAX_LOG_TEXT_BYTES: usize = 4 * 1024;
/// Maximum structured fields attached to one event.
pub const MAX_LOG_FIELDS: usize = 32;
/// Maximum UTF-8 bytes retained in one structured field key.
pub const MAX_LOG_FIELD_KEY_BYTES: usize = 128;
/// Maximum UTF-8 bytes retained in one structured field value.
pub const MAX_LOG_FIELD_VALUE_BYTES: usize = 4 * 1024;

/// Event rejected before sanitization or sink delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogEventError {
    /// A text field exceeds its public byte ceiling.
    TextTooLong,
    /// The field count exceeds the public ceiling.
    TooManyFields,
    /// A structured field key exceeds its public byte ceiling.
    FieldKeyTooLong,
    /// A structured field value exceeds its public byte ceiling.
    FieldValueTooLong,
}

/// Operational impact of an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// Fine-grained diagnostic event.
    Trace,
    /// Developer-oriented diagnostic event.
    Debug,
    /// Normal operational event.
    Info,
    /// Recoverable or degraded condition.
    Warn,
    /// Failed operation.
    Error,
    /// Failure requiring urgent operator attention.
    Critical,
}

/// Detail threshold from V1 (critical) through V9 (deep diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Verbosity(u8);

/// Invalid public verbosity outside the V1–V9 contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerbosityError;

impl Verbosity {
    /// Critical failures only.
    pub const V1: Self = Self(1);
    /// Important recoverable errors.
    pub const V2: Self = Self(2);
    /// Relevant warnings.
    pub const V3: Self = Self(3);
    /// Essential application events.
    pub const V4: Self = Self(4);
    /// Normal operation.
    pub const V5: Self = Self(5);
    /// Flow details.
    pub const V6: Self = Self(6);
    /// Technical debugging.
    pub const V7: Self = Self(7);
    /// I/O, queue and timing diagnostics.
    pub const V8: Self = Self(8);
    /// Deep memory and internal diagnostics.
    pub const V9: Self = Self(9);

    /// Validates one public verbosity value.
    pub const fn new(value: u8) -> Option<Self> {
        if value >= 1 && value <= 9 {
            Some(Self(value))
        } else {
            None
        }
    }
    /// Returns the numeric level.
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// One structured field; paths must be supplied as fields instead of embedded text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogField {
    /// Stable field name.
    pub key: String,
    /// Field value.
    pub value: String,
    /// Whether the value is a local path.
    pub path: bool,
    /// Whether ordinary logs must redact the value.
    pub sensitive: bool,
}

/// Structured operational event passed through the sanitizer before normal sinks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEvent {
    /// Unix timestamp in milliseconds supplied by the caller or clock boundary.
    pub timestamp_ms: u64,
    /// Operational impact.
    pub severity: Severity,
    /// Detail threshold independent from severity.
    pub verbosity: Verbosity,
    /// Human-readable message.
    pub message: String,
    /// Stable subsystem name used for policy overrides.
    pub component: String,
    /// Optional source module when the caller has a stable module name.
    pub module: Option<String>,
    /// Optional named operation being observed.
    pub operation: Option<String>,
    /// Requested handling class; a policy can only preserve or reduce detail.
    pub sensitivity: Sensitivity,
    /// Optional application identity.
    pub application_id: Option<String>,
    /// Optional node identity.
    pub node_id: Option<String>,
    /// Optional tenant identity.
    pub tenant_id: Option<String>,
    /// Optional trace correlation identifier.
    pub trace_id: Option<String>,
    /// Optional request correlation identifier.
    pub request_id: Option<String>,
    /// Extra bounded structured values.
    pub fields: Vec<LogField>,
}

impl LogEvent {
    /// Creates a normal operational event without allocating optional metadata.
    pub fn new(
        timestamp_ms: u64,
        severity: Severity,
        verbosity: Verbosity,
        component: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp_ms,
            severity,
            verbosity,
            message: message.into(),
            component: component.into(),
            module: None,
            operation: None,
            sensitivity: Sensitivity::Safe,
            application_id: None,
            node_id: None,
            tenant_id: None,
            trace_id: None,
            request_id: None,
            fields: Vec::new(),
        }
    }

    /// Adds one structured field.
    #[must_use]
    pub fn field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push(LogField {
            key: key.into(),
            value: value.into(),
            path: false,
            sensitive: false,
        });
        self
    }

    /// Adds one path field which is alias-sanitized by normal policies.
    #[must_use]
    pub fn path(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push(LogField {
            key: key.into(),
            value: value.into(),
            path: true,
            sensitive: false,
        });
        self
    }

    /// Adds one secret field which ordinary policies redact before dispatch.
    #[must_use]
    pub fn secret(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push(LogField {
            key: key.into(),
            value: value.into(),
            path: false,
            sensitive: true,
        });
        self
    }

    /// Adds the stable source module without inferring it from compiler state.
    #[must_use]
    pub fn module(mut self, module: impl Into<String>) -> Self {
        self.module = Some(module.into());
        self
    }

    /// Adds a bounded caller-defined operation name.
    #[must_use]
    pub fn operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }

    /// Requests a handling class; sensitive delivery still requires a policy.
    #[must_use]
    pub fn sensitivity(mut self, sensitivity: Sensitivity) -> Self {
        self.sensitivity = sensitivity;
        self
    }

    /// Validates bounded event storage before sanitization and sink I/O.
    pub fn validate(&self) -> Result<(), LogEventError> {
        for value in [
            &self.message,
            &self.component,
            self.module.as_deref().unwrap_or_default(),
            self.operation.as_deref().unwrap_or_default(),
            self.application_id.as_deref().unwrap_or_default(),
            self.node_id.as_deref().unwrap_or_default(),
            self.tenant_id.as_deref().unwrap_or_default(),
            self.trace_id.as_deref().unwrap_or_default(),
            self.request_id.as_deref().unwrap_or_default(),
        ] {
            if value.len() > MAX_LOG_TEXT_BYTES {
                return Err(LogEventError::TextTooLong);
            }
        }
        if self.fields.len() > MAX_LOG_FIELDS {
            return Err(LogEventError::TooManyFields);
        }
        for field in &self.fields {
            if field.key.len() > MAX_LOG_FIELD_KEY_BYTES {
                return Err(LogEventError::FieldKeyTooLong);
            }
            if field.value.len() > MAX_LOG_FIELD_VALUE_BYTES {
                return Err(LogEventError::FieldValueTooLong);
            }
        }
        Ok(())
    }

    /// Estimates retained heap bytes without serializing or allocating.
    pub fn retained_bytes(&self) -> usize {
        let optional = [
            &self.module,
            &self.operation,
            &self.application_id,
            &self.node_id,
            &self.tenant_id,
            &self.trace_id,
            &self.request_id,
        ]
        .into_iter()
        .flatten()
        .map(String::capacity)
        .sum::<usize>();
        let fields = self
            .fields
            .iter()
            .map(|field| field.key.capacity().saturating_add(field.value.capacity()))
            .sum::<usize>();
        std::mem::size_of::<Self>()
            .saturating_add(self.message.capacity())
            .saturating_add(self.component.capacity())
            .saturating_add(
                self.fields
                    .capacity()
                    .saturating_mul(std::mem::size_of::<LogField>()),
            )
            .saturating_add(optional)
            .saturating_add(fields)
    }
}
