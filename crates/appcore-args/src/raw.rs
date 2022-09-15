// =============================================================================
//        #######
//     ###       ###     F: raw.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{CliError, CliErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgLimits {
    pub max_words: usize,
    pub max_word_bytes: usize,
    pub max_total_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawArgs {
    words: Vec<String>,
}

impl Default for ArgLimits {
    fn default() -> Self {
        Self {
            max_words: 1024,
            max_word_bytes: 64 * 1024,
            max_total_bytes: 1024 * 1024,
        }
    }
}

impl RawArgs {
    pub fn from_env() -> Result<Self, CliError> {
        let mut words = Vec::new();
        for (index, word) in std::env::args_os().skip(1).enumerate() {
            let word = word.into_string().map_err(|_| {
                CliError::new(
                    CliErrorKind::InvalidInput,
                    format!("argument {} is not valid UTF-8", index + 1),
                )
            })?;
            words.push(word);
        }
        Self::parse_with_limits(words, ArgLimits::default())
    }

    pub fn parse<I>(args: I) -> Result<Self, CliError>
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        Self::parse_with_limits(args, ArgLimits::default())
    }

    pub fn parse_with_limits<I>(args: I, limits: ArgLimits) -> Result<Self, CliError>
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        let mut words = Vec::new();
        let mut total_bytes = 0usize;
        for arg in args {
            let value = arg.into();
            validate_raw_word(&value, words.len(), total_bytes, limits)?;
            total_bytes = total_bytes.saturating_add(value.len());
            words.push(value);
        }
        Ok(Self { words })
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }
}

fn validate_raw_word(
    value: &str,
    count: usize,
    total: usize,
    limits: ArgLimits,
) -> Result<(), CliError> {
    if count >= limits.max_words {
        return Err(CliError::new(
            CliErrorKind::InvalidInput,
            format!("argument count exceeds limit of {}", limits.max_words),
        ));
    }
    if value.len() > limits.max_word_bytes {
        return Err(CliError::new(
            CliErrorKind::InvalidInput,
            format!(
                "argument {} exceeds byte limit of {}",
                count + 1,
                limits.max_word_bytes
            ),
        ));
    }
    if total.saturating_add(value.len()) > limits.max_total_bytes {
        return Err(CliError::new(
            CliErrorKind::InvalidInput,
            format!(
                "total argument bytes exceed limit of {}",
                limits.max_total_bytes
            ),
        ));
    }
    if value.contains('\0') {
        return Err(CliError::new(
            CliErrorKind::InvalidInput,
            format!("argument {} contains a NUL byte", count + 1),
        ));
    }
    Ok(())
}
