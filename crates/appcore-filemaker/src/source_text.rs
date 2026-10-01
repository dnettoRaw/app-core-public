// =============================================================================
//        #######
//     ###       ###     F: source_text.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use serde::{Deserialize, Serialize};

use crate::{
    Alignment, ErrorCode, FileMakerError, Length, Result, TextIr, TextOverflow, Unit, WritingMode,
};

/// Declarative text measurement and overflow options.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextSourceOptions {
    /// Overflow behavior.
    pub overflow: TextOverflow,
    /// Optional maximum number of resolved lines.
    pub max_lines: Option<usize>,
    /// Minimum size used by `shrink`; must be absolute.
    pub min_font_size: Option<Length>,
    /// Line-height multiplier in millionths.
    pub line_height: u32,
    /// Horizontal lines or top-to-bottom right-to-left vertical columns.
    pub writing_mode: WritingMode,
    /// Inline alignment applied after wrapping and shrink.
    pub align_x: Alignment,
    /// Symmetric inline-axis padding applied to every line.
    pub padding_inline: Length,
    /// Per-side padding around the measured text block.
    pub padding: crate::TextBlockPadding,
}

impl Default for TextSourceOptions {
    fn default() -> Self {
        Self {
            overflow: TextOverflow::Wrap,
            max_lines: None,
            min_font_size: Some(Length::Absolute(Unit::from_raw(6_000_000))),
            line_height: 1_200_000,
            writing_mode: WritingMode::Horizontal,
            align_x: Alignment::Start,
            padding_inline: Length::Absolute(Unit::ZERO),
            padding: crate::TextBlockPadding::default(),
        }
    }
}

pub(crate) fn validate_text_options(options: &TextSourceOptions) -> Result<()> {
    let minimum = options
        .min_font_size
        .map(|value| value.resolve(Unit::ZERO, Unit::ZERO))
        .transpose()?
        .flatten();
    let invalid_padding = match options.padding_inline {
        Length::Absolute(value) => value < Unit::ZERO,
        Length::Logical(value) => value < 0,
        Length::Percent(value) => !(0..500_000).contains(&value),
        Length::Auto => true,
    };
    let block_padding = [
        options.padding.top,
        options.padding.right,
        options.padding.bottom,
        options.padding.left,
    ];
    let invalid_block_padding = block_padding.iter().any(|value| match value {
        Length::Absolute(unit) => *unit < Unit::ZERO,
        Length::Logical(units) => *units < 0,
        Length::Percent(units) => !(0..500_000).contains(units),
        Length::Auto => true,
    });
    if options.max_lines == Some(0)
        || !(500_000..=4_000_000).contains(&options.line_height)
        || minimum.is_some_and(|value| value <= Unit::ZERO)
        || options
            .min_font_size
            .is_some_and(|value| !matches!(value, Length::Absolute(_)))
        || invalid_padding
        || invalid_block_padding
    {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "text options contain an invalid line, font, or inline-padding constraint",
        ));
    }
    Ok(())
}

pub(crate) fn convert_text_options(options: TextSourceOptions) -> TextIr {
    TextIr {
        overflow: options.overflow,
        max_lines: options.max_lines,
        min_font_size: options.min_font_size,
        line_height: options.line_height,
        writing_mode: options.writing_mode,
        align_x: options.align_x,
        padding_inline: options.padding_inline,
        padding: options.padding,
    }
}
