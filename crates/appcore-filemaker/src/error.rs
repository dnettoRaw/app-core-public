// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Stable failure codes and UTF-8-safe bounded diagnostic ownership.

use std::fmt;

use thiserror::Error;

/// Stable machine-readable failure code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    /// Source is not syntactically valid.
    SchemaSyntax,
    /// Source schema version is absent or unsupported.
    SchemaVersion,
    /// Source contains an invalid field or value.
    SchemaField,
    /// A typed data value or binding is invalid.
    DataType,
    /// Computed data contains a dependency cycle.
    DataCycle,
    /// An asset reference violates the resolver sandbox.
    AssetSandbox,
    /// An asset is missing or invalid.
    AssetInvalid,
    /// A required font or glyph is unavailable.
    FontMissing,
    /// Geometry overflowed or violates an invariant.
    GeometryInvalid,
    /// Layout constraints cannot be resolved.
    LayoutInvalid,
    /// Collision/reflow did not converge within the configured bound.
    LayoutNonConvergent,
    /// A patch is malformed or targets an absent node.
    PatchInvalid,
    /// A patch attempted to mutate a locked node.
    PatchLocked,
    /// The requested exporter does not support a required feature.
    ExportUnsupported,
    /// The output writer failed.
    ExportWrite,
    /// A configured resource budget was exceeded.
    LimitExceeded,
    /// Cooperative cancellation was requested.
    Cancelled,
    /// Validation or preflight rejected the requested operation.
    Validation,
}

impl ErrorCode {
    /// Returns the stable `FM-*` representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SchemaSyntax => "FM-SCHEMA-SYNTAX",
            Self::SchemaVersion => "FM-SCHEMA-VERSION",
            Self::SchemaField => "FM-SCHEMA-FIELD",
            Self::DataType => "FM-DATA-TYPE",
            Self::DataCycle => "FM-DATA-CYCLE",
            Self::AssetSandbox => "FM-ASSET-SANDBOX",
            Self::AssetInvalid => "FM-ASSET-INVALID",
            Self::FontMissing => "FM-FONT-MISSING",
            Self::GeometryInvalid => "FM-GEOM-INVALID",
            Self::LayoutInvalid => "FM-LAYOUT-INVALID",
            Self::LayoutNonConvergent => "FM-LAYOUT-NON-CONVERGENT",
            Self::PatchInvalid => "FM-PATCH-INVALID",
            Self::PatchLocked => "FM-PATCH-LOCKED",
            Self::ExportUnsupported => "FM-EXPORT-UNSUPPORTED",
            Self::ExportWrite => "FM-EXPORT-WRITE",
            Self::LimitExceeded => "FM-LIMIT-EXCEEDED",
            Self::Cancelled => "FM-CANCELLED",
            Self::Validation => "FM-VALIDATION",
        }
    }
}

/// Typed compiler error with stable code and bounded context.
#[derive(Debug, Error)]
#[error("{code}: {message}")]
pub struct FileMakerError {
    code: CodeDisplay,
    message: String,
    source_path: Option<String>,
    layout_context: Option<TextFailureContextV1>,
}

impl FileMakerError {
    /// Creates an error with at most 1,024 UTF-8 bytes of diagnostic text.
    /// Oversized backing allocations are replaced with the complete-character prefix.
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let message = bounded_diagnostic(message.into(), 1_024);
        Self {
            code: CodeDisplay(code),
            message,
            source_path: None,
            layout_context: None,
        }
    }

    /// Attaches at most 512 UTF-8 bytes of the logical source path, never a partial character.
    #[must_use]
    pub fn at(mut self, source_path: impl Into<String>) -> Self {
        let source_path = bounded_diagnostic(source_path.into(), 512);
        self.source_path = Some(source_path);
        self
    }

    /// Attaches payload-free text geometry in fixed-point units (1,000,000 per point).
    #[must_use]
    pub fn with_text_context(mut self, context: TextFailureContextV1) -> Self {
        self.layout_context = Some(context);
        self
    }

    /// Produces safe IPC evidence without free-form messages or document content.
    /// Only bounded logical paths with safe schema-identifier characters are retained.
    #[must_use]
    pub fn diagnostic_v1(&self) -> FileMakerDiagnosticV1 {
        FileMakerDiagnosticV1 {
            schema_version: 1,
            code: self.code().as_str().to_owned(),
            source_path: self
                .source_path
                .as_ref()
                .filter(|path| {
                    !path.starts_with('/')
                        && !path.contains("..")
                        && path.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric()
                                || matches!(byte, b'.' | b'_' | b'-' | b'/' | b'[' | b']')
                        })
                })
                .cloned(),
            text: self.layout_context,
        }
    }

    /// Returns the stable code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code.0
    }

    /// Returns the bounded human-readable diagnostic.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the logical input path, when available.
    #[must_use]
    pub fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }
}

#[derive(Debug)]
struct CodeDisplay(ErrorCode);

impl fmt::Display for CodeDisplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

/// Result returned by `FileMaker` compiler operations.
pub type Result<T> = std::result::Result<T, FileMakerError>;

// A byte cap must preserve UTF-8 and must not retain an oversized caller buffer.
pub(crate) fn bounded_diagnostic(mut text: String, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if text.capacity() > limit {
        return text[..end].to_owned();
    }
    text.truncate(end);
    text
}

/// Payload-free context for text fitting failures. Units are millionths of a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct TextFailureContextV1 {
    /// Width available after padding.
    pub available_width_raw: i64,
    /// Height available after padding.
    pub available_height_raw: i64,
    /// Requested font size.
    pub font_size_raw: i64,
    /// Minimum permitted font size.
    pub minimum_font_size_raw: i64,
}
/// Stable serializable IPC diagnostic; UI wording and template changes are consumer-owned.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FileMakerDiagnosticV1 {
    /// Exact diagnostic schema version.
    pub schema_version: u16,
    /// Stable FM code; no free-form error message.
    pub code: String,
    /// Safe logical template/input path, never an absolute host filesystem path.
    pub source_path: Option<String>,
    /// Numeric layout evidence where available.
    pub text: Option<TextFailureContextV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_reports_share_utf8_bounds_and_release_oversized_buffers() {
        let mut text = String::with_capacity(1024 * 1024);
        text.push_str("short");
        let bounded = bounded_diagnostic(text, 512);
        assert_eq!(bounded, "short");
        assert!(bounded.capacity() <= 512);
        let text = "日".repeat(200);
        let mut report = crate::ValidationReport::default();
        report.push(
            crate::ValidationSeverity::Warning,
            crate::ValidationCode::Overflow,
            None,
            None,
            text.clone(),
            1,
        );
        assert_eq!(report.issues[0].message, "日".repeat(170));
        let mut losses = crate::ExportLossReport::default();
        losses.push(crate::ExportLossKind::UnsupportedElement, None, text);
        assert_eq!(losses.losses[0].message, "日".repeat(170));
    }

    #[test]
    fn unicode_diagnostics_are_bounded_without_splitting_characters() {
        for symbol in ["a", "é", "日", "🦀"] {
            for prefix in 0..4 {
                let text = format!("{}{}", "x".repeat(prefix), symbol.repeat(1100));
                let error =
                    FileMakerError::new(ErrorCode::SchemaField, text.clone()).at(text.clone());
                assert!(error.message().len() <= 1024);
                assert!(text.starts_with(error.message()));
                assert!(error.source_path().unwrap().len() <= 512);
                assert!(text.starts_with(error.source_path().unwrap()));
            }
        }
    }
}
