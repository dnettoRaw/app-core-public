// =============================================================================
//        #######
//     ###       ###     F: native.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/05 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2
// =============================================================================

//! Native argv ingestion and typed path/address results without lossy conversion.

use crate::{ArgLimits, CliError, CliErrorKind, OptionSpec, ValueMode, ValueType};
use std::ffi::{OsStr, OsString};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Additional native value validation, preserving the stable `ValueType` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ValueKind {
    /// UTF-8 text validated by `ValueType`.
    #[default]
    Text,
    /// Nonempty native path; no filesystem access or canonicalization.
    Path,
    /// Numeric IP address and port, including bracketed IPv6; no DNS lookup.
    SocketAddr,
}

/// Bounded native arguments, excluding the executable name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawOsArgs {
    words: Vec<OsString>,
    limits: ArgLimits,
}
impl RawOsArgs {
    /// Reads native argv incrementally using default budgets.
    pub fn from_env() -> Result<Self, CliError> {
        Self::parse(std::env::args_os().skip(1))
    }
    /// Preserves native bytes with default limits.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, CliError> {
        Self::parse_with_limits(args, ArgLimits::default())
    }
    /// Checks each native word before retaining it, stopping on first failure.
    pub fn parse_with_limits(
        args: impl IntoIterator<Item = OsString>,
        limits: ArgLimits,
    ) -> Result<Self, CliError> {
        let mut words = Vec::new();
        let mut total = 0;
        for word in args {
            validate_word(&word, words.len(), total, limits)?;
            total += word.as_encoded_bytes().len();
            words.push(word);
        }
        Ok(Self { words, limits })
    }
    /// Native words in original order.
    pub fn words(&self) -> &[OsString] {
        &self.words
    }
    pub(crate) fn limits(&self) -> ArgLimits {
        self.limits
    }
}

/// Native parsed option; `Debug` hides values because environment may hold secrets.
#[derive(Clone, PartialEq, Eq)]
pub struct ParsedOsOption {
    pub(crate) name: String,
    pub(crate) value: Option<OsString>,
}
impl std::fmt::Debug for ParsedOsOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedOsOption")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .finish()
    }
}
impl ParsedOsOption {
    /// Canonical option name without dashes.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Original native value, if present.
    pub fn value(&self) -> Option<&OsStr> {
        self.value.as_deref()
    }
}
/// Parsed native values; option values are redacted from `Debug`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedOsCli {
    pub(crate) command_path: Vec<String>,
    pub(crate) options: Vec<ParsedOsOption>,
    pub(crate) positionals: Vec<OsString>,
    pub(crate) passthrough: Vec<OsString>,
}
impl ParsedOsCli {
    /// Canonical command path.
    pub fn command_path(&self) -> &[String] {
        &self.command_path
    }
    /// Options including resolved environment/default values.
    pub fn options(&self) -> &[ParsedOsOption] {
        &self.options
    }
    /// Native positional arguments.
    pub fn positionals(&self) -> &[OsString] {
        &self.positionals
    }
    /// Uninterpreted native values following --.
    pub fn passthrough(&self) -> &[OsString] {
        &self.passthrough
    }
    /// Whether an option is present after source resolution.
    pub fn has_flag(&self, name: &str) -> bool {
        self.options.iter().any(|o| o.name == name)
    }
    /// Last resolved native value.
    pub fn option_value(&self, name: &str) -> Option<&OsStr> {
        self.options
            .iter()
            .rev()
            .find(|o| o.name == name)
            .and_then(ParsedOsOption::value)
    }
    /// All native values for a repeatable option.
    pub fn option_values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a OsStr> + 'a {
        self.options
            .iter()
            .filter(move |o| o.name == name)
            .filter_map(ParsedOsOption::value)
    }
    /// Number of occurrences including a resolved fallback, if used.
    pub fn option_occurrences(&self, name: &str) -> usize {
        self.options.iter().filter(|o| o.name == name).count()
    }
    /// Returns a native path without lossy conversion.
    pub fn option_path(&self, name: &str) -> Option<PathBuf> {
        self.option_value(name).map(PathBuf::from)
    }
    /// Parses a numeric socket address without DNS.
    pub fn option_socket_addr(&self, name: &str) -> Result<Option<SocketAddr>, CliError> {
        self.option_value(name)
            .map(|v| {
                v.to_str().and_then(|s| s.parse().ok()).ok_or_else(|| {
                    CliError::new(
                        CliErrorKind::InvalidValue,
                        format!("invalid socket address for --{name}"),
                    )
                })
            })
            .transpose()
    }
    pub(crate) fn into_text(self) -> Result<crate::ParsedCli, CliError> {
        let convert = |v: OsString| {
            v.into_string().map_err(|_| {
                CliError::new(CliErrorKind::InvalidInput, "native value requires parse_os")
            })
        };
        Ok(crate::ParsedCli {
            command_path: self.command_path,
            options: self
                .options
                .into_iter()
                .map(|o| {
                    Ok(crate::ParsedOption {
                        name: o.name,
                        value: o.value.map(&convert).transpose()?,
                    })
                })
                .collect::<Result<_, CliError>>()?,
            positionals: self
                .positionals
                .into_iter()
                .map(&convert)
                .collect::<Result<_, _>>()?,
            passthrough: self
                .passthrough
                .into_iter()
                .map(convert)
                .collect::<Result<_, _>>()?,
        })
    }
}

pub(crate) fn validate_word(
    value: &OsStr,
    count: usize,
    total: usize,
    limits: ArgLimits,
) -> Result<(), CliError> {
    let bytes = value.as_encoded_bytes();
    if count >= limits.max_words
        || bytes.len() > limits.max_word_bytes
        || total.saturating_add(bytes.len()) > limits.max_total_bytes
        || bytes.contains(&0)
    {
        return Err(CliError::new(
            CliErrorKind::InvalidInput,
            "native argument exceeds count/byte budget or contains NUL",
        ));
    }
    Ok(())
}

pub(crate) fn validate_native(
    value: &OsStr,
    kind: ValueKind,
    ty: ValueType,
    possible: &[String],
    owner: &str,
) -> Result<(), CliError> {
    if kind == ValueKind::Path {
        if value.is_empty() {
            return Err(CliError::new(
                CliErrorKind::InvalidValue,
                format!("empty path for {owner}"),
            ));
        }
        if possible.is_empty() {
            return Ok(());
        }
    }
    let text = value.to_str().ok_or_else(|| {
        CliError::new(
            CliErrorKind::InvalidValue,
            format!("{owner} requires UTF-8 text"),
        )
    })?;
    if kind == ValueKind::SocketAddr && text.parse::<SocketAddr>().is_err() {
        return Err(CliError::new(
            CliErrorKind::InvalidValue,
            format!("{owner} requires a numeric socket address"),
        ));
    }
    // Do not echo environment values or secrets in errors.
    crate::value_parser::validate_value(text, ty, possible, owner)
}

pub(crate) fn split_attached(word: &OsStr, separator: u8) -> (OsString, Option<OsString>) {
    let bytes = word.as_encoded_bytes();
    match bytes.iter().position(|b| *b == separator) {
        Some(i) => (
            os_slice(word, 0, i),
            Some(os_slice(word, i + 1, bytes.len())),
        ),
        None => (word.to_os_string(), None),
    }
}

pub(crate) fn os_slice(word: &OsStr, start: usize, end: usize) -> OsString {
    // Boundaries are immediately adjacent to ASCII option syntax, so valid UTF-8
    // words can be sliced directly. Platform-native conversion preserves others.
    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        OsString::from_vec(word.as_bytes()[start..end].to_vec())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let units = word.encode_wide().collect::<Vec<_>>();
        let bytes = word.as_encoded_bytes();
        if !bytes[..start].is_ascii() || (end != bytes.len() && !bytes[..end].is_ascii()) {
            return word.to_os_string();
        }
        let length = if end == bytes.len() { units.len() } else { end };
        OsString::from_wide(&units[start..length])
    }
    #[cfg(not(any(unix, windows)))]
    {
        OsString::from(&word.to_str().unwrap_or("")[start..end])
    }
}

pub(crate) fn option_value(
    words: &[OsString],
    index: usize,
    inline: Option<OsString>,
    option: &OptionSpec,
    short: bool,
) -> Result<(Option<OsString>, usize), CliError> {
    match option.value_mode() {
        ValueMode::Forbidden if inline.is_some() && !short => Err(CliError::new(
            CliErrorKind::InvalidValue,
            "flag does not accept a value",
        )),
        ValueMode::Forbidden => Ok((None, index + 1)),
        ValueMode::Required => match inline {
            Some(value) => Ok((Some(value), index + 1)),
            None => words
                .get(index + 1)
                .cloned()
                .map(|v| (Some(v), index + 2))
                .ok_or_else(|| {
                    CliError::new(
                        CliErrorKind::MissingValue,
                        format!("--{} requires a value", option.long()),
                    )
                }),
        },
        ValueMode::Optional => {
            if inline.is_some() || !option.accepts_detached_optional_value() {
                return Ok((inline, index + 1));
            }
            match words.get(index + 1).filter(|v| {
                validate_native(
                    v,
                    option.kind,
                    option.value_type_kind(),
                    option.possible_values(),
                    option.long(),
                )
                .is_ok()
            }) {
                Some(v) => Ok((Some(v.clone()), index + 2)),
                None => Ok((None, index + 1)),
            }
        }
    }
}
