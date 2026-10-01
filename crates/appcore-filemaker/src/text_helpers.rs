// =============================================================================
//        #######
//     ###       ###     F: text_helpers.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Bounded grapheme wrapping and shared text diagnostics.

use unicode_segmentation::UnicodeSegmentation;

use crate::{ErrorCode, FileMakerError, GlyphRun, Result, Unit};

pub(super) fn append_overlong(
    source: &str,
    max_width: Unit,
    line: &mut String,
    lines: &mut Vec<String>,
    probes: &mut usize,
    measure: &mut impl FnMut(&str) -> Result<Unit>,
) -> Result<()> {
    for grapheme in source.graphemes(true) {
        let candidate = format!("{line}{grapheme}");
        if !line.is_empty() && measure_bounded(&candidate, probes, measure)? > max_width {
            lines.push(std::mem::take(line));
            grapheme.clone_into(line);
        } else {
            *line = candidate;
        }
    }
    Ok(())
}

pub(crate) fn break_lines(
    text: &str,
    max_width: Unit,
    mut measure: impl FnMut(&str) -> Result<Unit>,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    let mut probes = 0_usize;
    for paragraph in text.split('\n') {
        let indentation: String = paragraph
            .chars()
            .take_while(|character| character.is_whitespace())
            .collect();
        let mut line = indentation.clone();
        for part in paragraph[indentation.len()..].split_word_bounds() {
            let candidate = format!("{line}{part}");
            let candidate_width = measure_bounded(&candidate, &mut probes, &mut measure)?;
            let has_content = line
                .get(indentation.len()..)
                .is_some_and(|content| content.chars().any(|character| !character.is_whitespace()));
            if has_content && candidate_width > max_width {
                lines.push(line.trim_end().to_owned());
                line.clone_from(&indentation);
                append_overlong(
                    part.trim_start(),
                    max_width,
                    &mut line,
                    &mut lines,
                    &mut probes,
                    &mut measure,
                )?;
            } else if candidate_width > max_width {
                line = candidate;
                let part = std::mem::take(&mut line);
                append_overlong(
                    &part,
                    max_width,
                    &mut line,
                    &mut lines,
                    &mut probes,
                    &mut measure,
                )?;
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    Ok(lines)
}

pub(super) fn measure_bounded(
    source: &str,
    probes: &mut usize,
    measure: &mut impl FnMut(&str) -> Result<Unit>,
) -> Result<Unit> {
    const MAX_LINE_BREAK_PROBES: usize = 100_000;
    *probes = probes
        .checked_add(1)
        .ok_or_else(|| limit_error("line-break probe count overflow"))?;
    if *probes > MAX_LINE_BREAK_PROBES {
        return Err(limit_error("line breaking exceeds its operation budget"));
    }
    measure(source)
}

pub(super) fn sum_run_widths(runs: &[GlyphRun]) -> Result<Unit> {
    runs.iter()
        .try_fold(Unit::ZERO, |total, run| total.checked_add(run.width))
}

pub(super) fn layout_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}

pub(super) fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}

pub(super) fn contains_emoji(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(
            character as u32,
            0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0xFE0F | 0x200D
        )
    })
}
