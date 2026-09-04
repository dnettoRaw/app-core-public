// =============================================================================
//        #######
//     ###       ###     F: failure.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded failure contracts and behavior for this crate.

use appcore_filemaker::ErrorCode;
use serde::ser::SerializeStruct;

pub(crate) const EXIT_VALIDATION: i32 = 2;
pub(crate) const EXIT_USAGE: i32 = 64;
pub(crate) const EXIT_DATA: i32 = 65;
pub(crate) const EXIT_NOINPUT: i32 = 66;
pub(crate) const EXIT_UNAVAILABLE: i32 = 69;
pub(crate) const EXIT_SOFTWARE: i32 = 70;
pub(crate) const EXIT_CANTCREATE: i32 = 73;
pub(crate) const EXIT_IO: i32 = 74;
pub(crate) const EXIT_TEMPFAIL: i32 = 75;
pub(crate) const EXIT_CANCELLED: i32 = 130;

pub(crate) struct CliFailure {
    exit: i32,
    code: String,
    message: String,
    json: bool,
}

impl CliFailure {
    pub(crate) fn new(
        exit: i32,
        code: impl Into<String>,
        message: impl Into<String>,
        json: bool,
    ) -> Self {
        Self {
            exit,
            code: code.into(),
            message: message.into(),
            json,
        }
    }

    pub(crate) fn usage(message: impl Into<String>, json: bool) -> Self {
        Self::new(EXIT_USAGE, "FM-CLI-USAGE", message, json)
    }

    pub(crate) fn io(
        exit: i32,
        code: impl Into<String>,
        message: impl Into<String>,
        json: bool,
    ) -> Self {
        Self::new(exit, code, message, json)
    }

    pub(crate) fn from_core(error: appcore_filemaker::FileMakerError, json: bool) -> Self {
        let exit = match error.code() {
            ErrorCode::SchemaSyntax
            | ErrorCode::SchemaVersion
            | ErrorCode::SchemaField
            | ErrorCode::DataType
            | ErrorCode::DataCycle
            | ErrorCode::PatchInvalid
            | ErrorCode::PatchLocked => EXIT_DATA,
            ErrorCode::AssetSandbox | ErrorCode::AssetInvalid | ErrorCode::FontMissing => {
                EXIT_NOINPUT
            }
            ErrorCode::GeometryInvalid
            | ErrorCode::LayoutInvalid
            | ErrorCode::LayoutNonConvergent
            | ErrorCode::ExportUnsupported
            | ErrorCode::Validation => EXIT_VALIDATION,
            ErrorCode::ExportWrite => EXIT_IO,
            ErrorCode::LimitExceeded => EXIT_TEMPFAIL,
            ErrorCode::Cancelled => EXIT_CANCELLED,
        };
        Self::new(exit, error.code().as_str(), error.message(), json)
    }

    #[must_use]
    pub(crate) const fn exit_code(&self) -> i32 {
        self.exit
    }

    pub(crate) fn write_to(&self, writer: &mut dyn std::io::Write) -> std::io::Result<()> {
        if self.json {
            serde_json::to_writer(&mut *writer, &FailureOutput(self))
                .map_err(std::io::Error::other)?;
        } else {
            write!(writer, "{}: {}", self.code, self.message)?;
        }
        writer.write_all(b"\n")
    }

    #[cfg(test)]
    pub(crate) fn render(&self) -> String {
        if self.json {
            serde_json::to_string(&FailureOutput(self)).unwrap_or_default()
        } else {
            format!("{}: {}", self.code, self.message)
        }
    }
}

struct FailureOutput<'a>(&'a CliFailure);

impl serde::Serialize for FailureOutput<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("CliFailure", 4)?;
        state.serialize_field("code", &self.0.code)?;
        state.serialize_field("exit_code", &self.0.exit)?;
        state.serialize_field("message", &self.0.message)?;
        state.serialize_field("ok", &false)?;
        state.end()
    }
}

pub(crate) type CliResult<T> = Result<T, CliFailure>;

pub(crate) fn software(message: impl Into<String>, json: bool) -> CliFailure {
    CliFailure::new(EXIT_SOFTWARE, "FM-CLI-SOFTWARE", message, json)
}

pub(crate) fn unavailable(message: impl Into<String>, json: bool) -> CliFailure {
    CliFailure::new(EXIT_UNAVAILABLE, "FM-CLI-UNAVAILABLE", message, json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore_filemaker::FileMakerError;

    #[test]
    fn every_core_error_has_a_stable_exit_class() {
        let cases = [
            (ErrorCode::SchemaSyntax, EXIT_DATA),
            (ErrorCode::SchemaVersion, EXIT_DATA),
            (ErrorCode::SchemaField, EXIT_DATA),
            (ErrorCode::DataType, EXIT_DATA),
            (ErrorCode::DataCycle, EXIT_DATA),
            (ErrorCode::PatchInvalid, EXIT_DATA),
            (ErrorCode::PatchLocked, EXIT_DATA),
            (ErrorCode::AssetSandbox, EXIT_NOINPUT),
            (ErrorCode::AssetInvalid, EXIT_NOINPUT),
            (ErrorCode::FontMissing, EXIT_NOINPUT),
            (ErrorCode::GeometryInvalid, EXIT_VALIDATION),
            (ErrorCode::LayoutInvalid, EXIT_VALIDATION),
            (ErrorCode::LayoutNonConvergent, EXIT_VALIDATION),
            (ErrorCode::ExportUnsupported, EXIT_VALIDATION),
            (ErrorCode::Validation, EXIT_VALIDATION),
            (ErrorCode::ExportWrite, EXIT_IO),
            (ErrorCode::LimitExceeded, EXIT_TEMPFAIL),
            (ErrorCode::Cancelled, EXIT_CANCELLED),
        ];
        for (code, expected) in cases {
            let failure = CliFailure::from_core(FileMakerError::new(code, "controlled"), true);
            assert_eq!(failure.exit_code(), expected);
            let rendered: serde_json::Value = serde_json::from_str(&failure.render()).unwrap();
            assert_eq!(rendered["exit_code"], expected);
            assert_eq!(rendered["code"], code.as_str());
        }
    }

    #[test]
    fn json_failure_keeps_the_stable_compact_field_order() {
        let failure = CliFailure::usage("controlled", true);
        assert_eq!(
            failure.render(),
            r#"{"code":"FM-CLI-USAGE","exit_code":64,"message":"controlled","ok":false}"#
        );
    }
}
