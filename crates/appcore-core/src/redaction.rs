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
    if is_text_redacted_and_bounded(input, max_bytes) {
        return input.to_owned();
    }
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
        output = redact_marker(output, marker);
    }
    truncate_text(output, max_bytes, input.len() > scan_end)
}

/// Reports whether redaction and bounding would preserve the text byte-for-byte.
pub(crate) fn is_text_redacted_and_bounded(input: &str, max_bytes: usize) -> bool {
    if input.len() > max_bytes.max(TRUNCATED.len()) {
        return false;
    }
    let mut cursor = 0usize;
    while cursor < input.len() {
        let Some(marker_len) = marker_len_at(input.as_bytes(), cursor) else {
            cursor += 1;
            continue;
        };
        let value_start = cursor + marker_len;
        let value_end = input[value_start..]
            .find(is_secret_delimiter)
            .map(|offset| value_start + offset)
            .unwrap_or(input.len());
        if value_end > value_start && &input[value_start..value_end] != REDACTED {
            return false;
        }
        cursor = value_end;
    }
    true
}

fn marker_len_at(input: &[u8], start: usize) -> Option<usize> {
    let candidates: &[&[u8]] = match input[start].to_ascii_lowercase() {
        b'a' => &[b"authorization:", b"api_key=", b"apikey="],
        b'b' => &[b"bearer "],
        b'p' => &[b"password="],
        b's' => &[b"secret="],
        b't' => &[b"token="],
        _ => return None,
    };
    candidates
        .iter()
        .find(|candidate| ascii_prefix_eq(&input[start..], candidate))
        .map(|candidate| candidate.len())
}

fn ascii_prefix_eq(input: &[u8], expected: &[u8]) -> bool {
    input.len() >= expected.len()
        && input[..expected.len()]
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.to_ascii_lowercase() == *expected)
}

fn redact_marker(input: String, marker: &str) -> String {
    let lowercase = input.to_ascii_lowercase();
    let Some(first) = lowercase.find(marker) else {
        return input;
    };
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0usize;
    let mut next = Some(first);

    while let Some(relative) = next {
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
        next = lowercase[cursor..].find(marker);
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
    use super::{is_text_redacted_and_bounded, redact_text, redact_text_with_limit};

    #[test]
    fn absent_marker_preserves_the_existing_allocation() {
        let input = "ordinary 日本語 العربية text".to_owned();
        let pointer = input.as_ptr();
        let output = super::redact_marker(input, "token=");
        assert_eq!(output.as_ptr(), pointer);
        assert_eq!(output, "ordinary 日本語 العربية text");
    }

    #[test]
    fn marker_pass_matches_previous_case_folding_algorithm() {
        let markers = [
            "authorization:",
            "bearer ",
            "token=",
            "secret=",
            "password=",
            "api_key=",
            "apikey=",
        ];
        let values = [
            "",
            "a",
            "日",
            "[REDACTED]",
            "secret=nested",
            "Bearer.other",
            "é\u{2003}tail",
        ];
        for marker in markers {
            for value in values {
                for delimiter in [" ", "\n", ",", ";", "&", "\"", "'", "\u{2003}"] {
                    let input = format!(
                        "日本語 {}{value}{delimiter}{marker}{value}",
                        marker.to_ascii_uppercase()
                    );
                    let mut previous = input.clone();
                    let original = input.clone();
                    let mut current = input;
                    for pass in markers {
                        previous = previous_marker(&previous, pass);
                        current = super::redact_marker(current, pass);
                        assert_eq!(current, previous);
                    }
                    assert_eq!(redact_text(&original), previous);
                }
            }
        }
    }

    // Preserved reference implementation: production no longer allocates lowercase copies.
    fn previous_marker(input: &str, marker: &str) -> String {
        let lowercase = input.to_ascii_lowercase();
        let mut output = String::with_capacity(input.len());
        let mut cursor = 0;
        while let Some(relative) = lowercase[cursor..].find(marker) {
            let value_start = cursor + relative + marker.len();
            output.push_str(&input[cursor..value_start]);
            let value_end = input[value_start..]
                .find(super::is_secret_delimiter)
                .map_or(input.len(), |offset| value_start + offset);
            if value_end > value_start {
                output.push_str(super::REDACTED);
            }
            cursor = value_end;
            if cursor == input.len() {
                break;
            }
        }
        output.push_str(&input[cursor..]);
        output
    }

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

    #[test]
    fn allocation_free_check_matches_redaction_output() {
        let long = "é".repeat(25);
        let cases = [
            ("ordinary Unicode 日本語 العربية", 128),
            ("token=[REDACTED] status=ok", 128),
            ("ToKeN=[REDACTED]; bearer ", 128),
            ("token=secret", 128),
            ("token=[redacted]", 128),
            ("authorization:Bearer.secret", 128),
            ("password=, api_key=[REDACTED]", 128),
            ("secret=[REDACTED]&apikey=[REDACTED]", 128),
            (long.as_str(), 48),
        ];

        for (input, limit) in cases {
            let expected = previous_text(input, limit);
            assert_eq!(
                is_text_redacted_and_bounded(input, limit),
                expected == input,
                "input={input:?} limit={limit}"
            );
            assert_eq!(redact_text_with_limit(input, limit), expected);
        }
    }

    fn previous_text(input: &str, limit: usize) -> String {
        let limit = limit.max(super::TRUNCATED.len());
        let end = super::floor_char_boundary(input, limit.saturating_add(1024).min(input.len()));
        let mut output = input[..end].to_owned();
        for marker in [
            "authorization:",
            "bearer ",
            "token=",
            "secret=",
            "password=",
            "api_key=",
            "apikey=",
        ] {
            output = previous_marker(&output, marker);
        }
        super::truncate_text(output, limit, end < input.len())
    }
}
