// =============================================================================
//        #######
//     ###       ###     F: redaction_compare.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Paired current/reference redaction workloads; the reference is benchmark-only.

pub(super) const CASES: [&str; 4] = [
    "redaction_plain_8192_current",
    "redaction_plain_8192_reference",
    "redaction_mixed_current",
    "redaction_mixed_reference",
];

pub(super) fn run(selected: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    for (index, name) in CASES.iter().enumerate() {
        if selected.is_some_and(|selected| selected != *name) {
            continue;
        }
        let input = if index < 2 {
            "x".repeat(8192)
        } else {
            "日本語 ToKeN=synthetic; secret=sample bearer example status=ok\n".repeat(96)
        };
        assert!(input.len() <= appcore_core::MAX_OPERATIONAL_TEXT_BYTES);
        assert_eq!(appcore_core::redact_text(&input), reference(&input));
        let operation = if index % 2 == 0 {
            appcore_core::redact_text
        } else {
            reference
        };
        super::measure(name, 1000, || {
            std::hint::black_box(operation(std::hint::black_box(&input)));
            Ok(())
        })?;
    }
    Ok(())
}

// Preserved from efe9a205. These fixtures fit the scan limit; output may grow.
fn reference(input: &str) -> String {
    let mut output = input.to_owned();
    for marker in [
        "authorization:",
        "bearer ",
        "token=",
        "secret=",
        "password=",
        "api_key=",
        "apikey=",
    ] {
        output = reference_marker(&output, marker);
    }
    if output.len() > appcore_core::MAX_OPERATIONAL_TEXT_BYTES {
        let mut end = appcore_core::MAX_OPERATIONAL_TEXT_BYTES - "[TRUNCATED]".len();
        while !output.is_char_boundary(end) {
            end -= 1;
        }
        output.truncate(end);
        output.push_str("[TRUNCATED]");
    }
    output
}

fn reference_marker(input: &str, marker: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let lowercase = input.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(relative) = lowercase[cursor..].find(marker) {
        let value_start = cursor + relative + marker.len();
        output.push_str(&input[cursor..value_start]);
        let value_end = input[value_start..]
            .find(|character: char| {
                character.is_whitespace() || matches!(character, ',' | ';' | '&' | '"' | '\'')
            })
            .map_or(input.len(), |offset| value_start + offset);
        if value_end > value_start {
            output.push_str("[REDACTED]");
        }
        cursor = value_end;
        if cursor == input.len() {
            break;
        }
    }
    output.push_str(&input[cursor..]);
    output
}
