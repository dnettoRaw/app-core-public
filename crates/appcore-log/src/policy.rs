// =============================================================================
//        #######
//     ###       ###     F: policy.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 12:12:57 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Filtering and sanitization run before ordinary sinks receive an event.

use crate::event::LogEvent;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Explicit information-handling policy, never inferred from verbosity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sensitivity {
    /// Shareable default output.
    Safe,
    /// More technical but still redacted output.
    Diagnostic,
    /// Encrypted diagnostics only.
    Sensitive,
}

/// Explicit local path aliases used by typed path fields.
#[derive(Debug, Clone, Default)]
pub struct PathAliases {
    /// Application root if known.
    pub app_root: Option<String>,
    /// User home if allowed to be recognized.
    pub home: Option<String>,
    /// Temporary directory if known.
    pub temp: Option<String>,
    /// Data directory if known.
    pub data: Option<String>,
    /// Cache directory if known.
    pub cache: Option<String>,
}

/// Filters events before formatting and sanitizes values before ordinary sinks.
#[derive(Debug, Clone)]
pub struct LogPolicy {
    global: u8,
    components: BTreeMap<String, u8>,
    /// Enables full typed-path output only by explicit configuration.
    pub full_paths: bool,
    /// Safe, Diagnostic or explicit encrypted Sensitive output.
    pub sensitivity: Sensitivity,
    /// Trusted aliases for typed local path fields.
    pub paths: PathAliases,
}

impl Default for LogPolicy {
    fn default() -> Self {
        Self::new(crate::Verbosity::V4)
    }
}

impl LogPolicy {
    /// Creates a safe policy with one global verbosity threshold.
    pub fn new(verbosity: crate::Verbosity) -> Self {
        Self {
            global: verbosity.value(),
            components: BTreeMap::new(),
            full_paths: false,
            sensitivity: Sensitivity::Safe,
            paths: PathAliases::default(),
        }
    }
    /// Overrides a component threshold without changing the global threshold.
    pub fn set_component(&mut self, component: impl Into<String>, verbosity: crate::Verbosity) {
        self.components.insert(component.into(), verbosity.value());
    }
    /// Reports whether an event is selected before serialization or sink I/O.
    pub fn allows(&self, event: &LogEvent) -> bool {
        self.allows_component(&event.component, event.verbosity)
    }

    /// Reports whether a component and verbosity are selected without an event.
    pub(crate) fn allows_component(&self, component: &str, verbosity: crate::Verbosity) -> bool {
        let configured = component_threshold(&self.components, component).unwrap_or(self.global);
        verbosity.value() <= configured
    }
    /// Produces the event accepted by its configured sensitivity boundary.
    ///
    /// In Safe and Diagnostic modes all normal sinks receive redacted secrets
    /// and aliased typed paths. Sensitive mode is routed only to DNT sinks by
    /// the dispatcher; fields marked with [`LogEvent::secret`] remain redacted
    /// even there because they represent prohibited credential material.
    pub fn sanitize(&self, event: &LogEvent) -> LogEvent {
        self.sanitize_owned(event.clone())
    }

    /// Sanitizes an owned event without cloning its message or metadata.
    pub fn sanitize_owned(&self, mut value: LogEvent) -> LogEvent {
        if self.sensitivity != Sensitivity::Sensitive {
            value.message = redact_text(value.message);
        }
        for field in &mut value.fields {
            if field.sensitive {
                field.value = "<REDACTED>".to_string();
            } else if field.path && !self.full_paths && self.sensitivity != Sensitivity::Sensitive {
                field.value = alias_path(&field.value, &self.paths);
            }
        }
        value
    }

    /// Returns the warning an application must surface when it enables
    /// encrypted sensitive diagnostics.
    pub const fn sensitive_warning() -> &'static str {
        "Sensitive logging is active: encrypted diagnostics may contain confidential material."
    }
}

fn component_threshold(components: &BTreeMap<String, u8>, component: &str) -> Option<u8> {
    let mut candidate = component;
    loop {
        if let Some(verbosity) = components.get(candidate) {
            return Some(*verbosity);
        }
        let (parent, _) = candidate.rsplit_once('.')?;
        candidate = parent;
    }
}

fn alias_path(value: &str, aliases: &PathAliases) -> String {
    for (prefix, alias) in [
        (&aliases.app_root, "<APP_ROOT>"),
        (&aliases.home, "<HOME>"),
        (&aliases.temp, "<TEMP>"),
        (&aliases.cache, "<CACHE>"),
        (&aliases.data, "<DATA>"),
    ] {
        if let Some(prefix) = prefix.as_deref() {
            if Path::new(value).starts_with(prefix) {
                return value.replacen(prefix, alias, 1);
            }
        }
    }
    "<LOCAL_PATH>".to_string()
}

fn redact_text(message: String) -> String {
    if !message
        .split_whitespace()
        .any(|token| redaction_for_token(token).is_some())
    {
        return message;
    }
    let mut redacted = String::with_capacity(message.len());
    for (index, token) in message.split_whitespace().enumerate() {
        if index != 0 {
            redacted.push(' ');
        }
        redacted.push_str(redaction_for_token(token).unwrap_or(token));
    }
    redacted
}

fn redaction_for_token(token: &str) -> Option<&'static str> {
    let secret_markers = [
        "token=",
        "password=",
        "secret=",
        "api_key=",
        "apikey=",
        "authorization:",
        "cookie:",
        "set-cookie:",
        "session=",
        "private_key=",
    ];
    if secret_markers
        .iter()
        .any(|marker| contains_ascii_case_insensitive(token, marker))
    {
        return Some("<REDACTED>");
    }
    if token.contains("://") && token.contains('@') {
        return Some("<REDACTED_URL>");
    }
    None
}

fn contains_ascii_case_insensitive(value: &str, pattern: &str) -> bool {
    value.as_bytes().windows(pattern.len()).any(|window| {
        window
            .iter()
            .zip(pattern.as_bytes())
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
    })
}
