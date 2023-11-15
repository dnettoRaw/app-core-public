// =============================================================================
//        #######
//     ###       ###     F: redaction.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 10:48:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Conservative redaction for runtime logs, audit records, and diagnostics.

const REDACTED: &str = "[REDACTED]";
const TRUNCATED: &str = "[TRUNCATED]";

/// Maximum UTF-8 bytes retained from one log, audit, error, or diagnostic text.
pub const MAX_OPERATIONAL_TEXT_BYTES: usize = 8_192;

/// Redacts common credential forms without attempting to parse business payloads.
pub fn redact_text(input: &str) -> String {
    redact_text_with_limit(input, MAX_OPERATIONAL_TEXT_BYTES)
}

/// Redacts credential markers and bounds the resulting UTF-8 text.
pub fn redact_text_with_limit(input: &str, max_bytes: usize) -> String {
    let max_bytes = max_bytes.max(TRUNCATED.len());
    let scan_limit = max_bytes.saturating_add(1_024).min(input.len());
    let scan_end = floor_char_boundary(input, scan_limit);
    let mut output = input[..scan_end].to_string();
    for marker in [
        "authorization:",
        "bearer ",
        "token=",
        "secret=",
        "password=",
        "api_key=",
        "apikey=",
    ] {
        output = redact_marker(&output, marker);
    }
    truncate_text(output, max_bytes, input.len() > scan_end)
}

fn redact_marker(input: &str, marker: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let lowercase = input.to_ascii_lowercase();
    let mut cursor = 0usize;

    while let Some(relative) = lowercase[cursor..].find(marker) {
        let marker_start = cursor + relative;
        let value_start = marker_start + marker.len();
        output.push_str(&input[cursor..value_start]);
        let value_end = input[value_start..]
            .find(is_secret_delimiter)
            .map(|offset| value_start + offset)
            .unwrap_or(input.len());
        if value_end > value_start {
            output.push_str(REDACTED);
        }
        cursor = value_end;
        if cursor == input.len() {
            break;
        }
    }

    output.push_str(&input[cursor..]);
    output
}

fn is_secret_delimiter(character: char) -> bool {
    character.is_whitespace() || matches!(character, ',' | ';' | '&' | '"' | '\'')
}

fn truncate_text(mut value: String, max_bytes: usize, input_was_truncated: bool) -> String {
    if !input_was_truncated && value.len() <= max_bytes {
        return value;
    }
    let content_limit = max_bytes.saturating_sub(TRUNCATED.len());
    let end = floor_char_boundary(&value, content_limit.min(value.len()));
    value.truncate(end);
    value.push_str(TRUNCATED);
    value
}

fn floor_char_boundary(value: &str, mut index: usize) -> usize {
    index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::{redact_text, redact_text_with_limit};

    #[test]
    fn redacts_common_credentials_and_preserves_context() {
        let redacted =
            redact_text("request token=abc123 bearer xyz789 password=hunter2 status=failed");

        assert_eq!(
            redacted,
            "request token=[REDACTED] bearer [REDACTED] password=[REDACTED] status=failed"
        );
    }

    #[test]
    fn redaction_is_case_insensitive() {
        assert_eq!(
            redact_text("Authorization:Bearer.secret"),
            "Authorization:[REDACTED]"
        );
    }

    #[test]
    fn redaction_bounds_text_without_splitting_utf8() {
        let input = format!("token=secret {}", "é".repeat(100));
        let output = redact_text_with_limit(&input, 48);

        assert!(output.len() <= 48);
        assert!(output.contains("[REDACTED]"));
        assert!(output.ends_with("[TRUNCATED]"));
    }
}
