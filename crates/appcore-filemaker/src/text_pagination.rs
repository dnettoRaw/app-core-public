// =============================================================================
//        #######
//     ###       ###     F: text_pagination.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Splits already-shaped horizontal text at complete measured-line boundaries.
//!
//! Pagination consumes measured lines rather than reshaping or slicing source
//! strings, preserving glyph runs and their order across page fragments. Every
//! emitted chunk has positive bounded height; a single line taller than the
//! continuation region fails explicitly instead of being clipped or retried.

use std::ops::Range;

use crate::{ErrorCode, FileMakerError, Rect, Result, TextLayout, Unit};

#[derive(Clone, Debug)]
pub(crate) struct TextChunk {
    pub(crate) lines: Range<usize>,
    pub(crate) height: Unit,
    pub(crate) starts_new_page: bool,
}

pub(crate) fn split_lines(
    layout: &TextLayout,
    first_y: Unit,
    container: Rect,
) -> Result<Vec<TextChunk>> {
    let vertical_padding = layout.padding.top.checked_add(layout.padding.bottom)?;
    let page_height = container.size.height.checked_sub(vertical_padding)?;
    if page_height <= Unit::ZERO {
        return Err(pagination_error(
            "text block padding leaves no continuation page content area",
        ));
    }
    let page_bottom = container.bottom()?;
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut height = Unit::ZERO;
    let mut starts_new_page = false;
    let mut remaining = page_bottom
        .checked_sub(first_y)?
        .checked_sub(vertical_padding)?
        .max(Unit::ZERO);
    let mut continuation = false;
    for (index, line) in layout.lines.iter().enumerate() {
        if line.height > page_height {
            return Err(pagination_error(
                "a shaped text line exceeds the page content height",
            ));
        }
        if height == Unit::ZERO && line.height > remaining {
            remaining = page_height;
            continuation = true;
            starts_new_page = true;
        }
        if height != Unit::ZERO && height.checked_add(line.height)? > remaining {
            chunks.push(TextChunk {
                lines: start..index,
                height: height.checked_add(vertical_padding)?,
                starts_new_page,
            });
            start = index;
            height = Unit::ZERO;
            starts_new_page = true;
            remaining = page_height;
            continuation = true;
        }
        height = height.checked_add(line.height)?;
        if continuation && chunks.is_empty() {
            starts_new_page = true;
        }
    }
    if height > Unit::ZERO {
        chunks.push(TextChunk {
            lines: start..layout.lines.len(),
            height: height.checked_add(vertical_padding)?,
            starts_new_page,
        });
    }
    Ok(chunks)
}

fn pagination_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Alignment, Size, TextLine, WritingMode};

    fn layout(line_heights: &[i64]) -> TextLayout {
        TextLayout {
            writing_mode: WritingMode::Horizontal,
            lines: line_heights
                .iter()
                .map(|height| TextLine {
                    source_text: String::new(),
                    runs: Vec::new(),
                    width: Unit::ZERO,
                    height: Unit::points(*height).unwrap(),
                })
                .collect(),
            measured: Size::default(),
            font_size: Unit::ZERO,
            paint_offset_y: Unit::ZERO,
            diagnostics: Vec::new(),
            align_x: Alignment::Start,
            padding_inline: Unit::ZERO,
            padding: crate::Insets::default(),
        }
    }

    #[test]
    fn splits_complete_lines_and_marks_page_continuations() {
        let chunks = split_lines(
            &layout(&[12; 12]),
            Unit::points(10).unwrap(),
            Rect::new(
                Unit::ZERO,
                Unit::ZERO,
                Unit::points(80).unwrap(),
                Unit::points(70).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.lines.clone())
                .collect::<Vec<_>>(),
            vec![0..5, 5..10, 10..12]
        );
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.starts_new_page)
                .collect::<Vec<_>>(),
            vec![false, true, true]
        );
    }

    #[test]
    fn rejects_a_single_line_taller_than_the_page_content() {
        let result = split_lines(
            &layout(&[71]),
            Unit::ZERO,
            Rect::new(
                Unit::ZERO,
                Unit::ZERO,
                Unit::points(80).unwrap(),
                Unit::points(70).unwrap(),
            )
            .unwrap(),
        );

        assert!(matches!(result, Err(error) if error.code() == ErrorCode::LayoutInvalid));
    }
}
