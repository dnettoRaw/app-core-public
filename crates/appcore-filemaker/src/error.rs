// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

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
}

impl FileMakerError {
    /// Creates an error while bounding user-controlled diagnostic text.
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let mut message = message.into();
        message.truncate(1_024);
        Self {
            code: CodeDisplay(code),
            message,
            source_path: None,
        }
    }

    /// Attaches a bounded logical source path.
    #[must_use]
    pub fn at(mut self, source_path: impl Into<String>) -> Self {
        let mut source_path = source_path.into();
        source_path.truncate(512);
        self.source_path = Some(source_path);
        self
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
