// =============================================================================
//        #######
//     ###       ###     F: text.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded text contracts and behavior for this crate.

use crate::{Alignment, ErrorCode, FileMakerError, FontManager, Insets, Result, Size, Unit};
pub(crate) use helpers::break_lines;
use helpers::{contains_emoji, layout_error, sum_run_widths};
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

#[path = "text_helpers.rs"]
mod helpers;

/// Text overflow strategy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOverflow {
    /// Wrap at Unicode word boundaries.
    #[default]
    Wrap,
    /// Reduce font size down to the explicit minimum.
    Shrink,
    /// Replace the final fitting graphemes with an ellipsis.
    Ellipsis,
    /// Retain glyphs and expose a clipping diagnostic.
    Clip,
    /// Expand the measured box to fit.
    Expand,
    /// Reject overflow.
    Error,
}

/// Writing direction selected before shaping.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingMode {
    /// Horizontal lines with `BiDi` runs.
    #[default]
    Horizontal,
    /// Top-to-bottom columns flowing from right to left.
    Vertical,
}

/// Explicit text measurement options.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextOptions {
    /// Primary registered font.
    pub font: String,
    /// Requested font size.
    pub font_size: Unit,
    /// Minimum size used only by `Shrink`.
    pub min_font_size: Unit,
    /// Available layout box.
    pub bounds: Size,
    /// Optional maximum line count.
    pub max_lines: Option<usize>,
    /// Overflow behavior.
    pub overflow: TextOverflow,
    /// Line-height multiplier in millionths.
    pub line_height: u32,
    /// Horizontal or vertical writing.
    pub writing_mode: WritingMode,
    /// Inline alignment applied after shaping and wrapping.
    pub align_x: Alignment,
    /// Symmetric padding on the writing mode's inline axis.
    #[serde(default)]
    pub padding_inline: Unit,
}

impl TextOptions {
    /// Validates numeric text bounds.
    pub fn validate(&self) -> Result<()> {
        if self.font.is_empty()
            || self.font_size <= Unit::ZERO
            || self.min_font_size <= Unit::ZERO
            || self.min_font_size > self.font_size
            || self.bounds.width < Unit::ZERO
            || self.bounds.height < Unit::ZERO
            || self.padding_inline < Unit::ZERO
            || self.padding_inline.checked_scale(2_000_000)?
                > match self.writing_mode {
                    WritingMode::Horizontal => self.bounds.width,
                    WritingMode::Vertical => self.bounds.height,
                }
            || self.max_lines == Some(0)
            || !(500_000..=4_000_000).contains(&self.line_height)
        {
            return Err(layout_error("text options are invalid"));
        }
        Ok(())
    }
}

/// One positioned glyph in fixed-point output coordinates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Glyph {
    /// Font glyph ID.
    pub id: u16,
    /// UTF-8 byte cluster in the source run.
    pub cluster: u32,
    /// Horizontal advance.
    pub advance_x: Unit,
    /// Vertical advance.
    pub advance_y: Unit,
    /// Horizontal offset.
    pub offset_x: Unit,
    /// Vertical offset.
    pub offset_y: Unit,
}

/// Contiguous font/direction shaping result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GlyphRun {
    /// Explicit font name.
    pub font: String,
    /// Whether the run has right-to-left direction.
    pub rtl: bool,
    /// Original logical UTF-8 slice.
    pub text: String,
    /// Positioned glyphs.
    pub glyphs: Vec<Glyph>,
    /// Total horizontal advance.
    pub width: Unit,
}

/// One visual line.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextLine {
    /// Logical line text before bidi shaping, retained for deterministic pagination.
    #[serde(default)]
    pub source_text: String,
    /// Visual-order glyph runs.
    pub runs: Vec<GlyphRun>,
    /// Inline advance: width for horizontal lines, height for vertical columns.
    pub width: Unit,
    /// Block advance: height for horizontal lines, width for vertical columns.
    pub height: Unit,
}

/// Non-fatal text diagnostic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextDiagnostic {
    /// Content is clipped by the requested box.
    Clipped,
    /// Content was truncated and ellipsized.
    Ellipsized,
    /// Font size was reduced.
    Shrunk,
    /// An imported or manually constructed scene reports unavailable vertical writing.
    VerticalWritingUnavailable,
    /// Color emoji requires an exporter-specific capability.
    ColorEmojiRequiresExporter,
}

/// Complete deterministic shaped layout.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextLayout {
    /// Horizontal lines or top-to-bottom right-to-left columns.
    #[serde(default)]
    pub writing_mode: WritingMode,
    /// Lines or columns in block-flow order.
    pub lines: Vec<TextLine>,
    /// Natural or constrained measurement.
    pub measured: Size,
    /// Effective font size after shrinking.
    pub font_size: Unit,
    /// Paint-only vertical translation retained by every visual exporter.
    /// Positive values move text toward the page bottom; measurement and
    /// pagination continue to use the unshifted geometry.
    #[serde(default)]
    pub paint_offset_y: Unit,
    /// Non-fatal diagnostics.
    pub diagnostics: Vec<TextDiagnostic>,
    /// Inline alignment applied to every resolved line.
    #[serde(default = "default_alignment")]
    pub align_x: Alignment,
    /// Symmetric padding on the writing mode's inline axis.
    #[serde(default)]
    pub padding_inline: Unit,
    /// Resolved per-side padding around the text block.
    #[serde(default)]
    pub padding: Insets,
}

const fn default_alignment() -> Alignment {
    Alignment::Start
}

impl TextLayout {
    /// Returns the measured inline offset for one line inside a box.
    pub fn inline_offset(&self, available: Unit, line: &TextLine) -> Result<Unit> {
        let content = available
            .checked_sub(self.padding_inline.checked_scale(2_000_000)?)?
            .max(Unit::ZERO);
        let remaining = content.checked_sub(line.width)?.max(Unit::ZERO);
        match self.align_x {
            Alignment::Start => Ok(self.padding_inline),
            Alignment::Center => Ok(self
                .padding_inline
                .checked_add(Unit::from_raw(remaining.raw() / 2))?),
            Alignment::End => Ok(self.padding_inline.checked_add(remaining)?),
        }
    }
}

/// Unicode text engine backed only by explicit fonts.
pub struct TextEngine<'a> {
    fonts: &'a FontManager,
}

impl<'a> TextEngine<'a> {
    /// Creates an engine over an explicit deterministic registry.
    #[must_use]
    pub const fn new(fonts: &'a FontManager) -> Self {
        Self { fonts }
    }

    /// Performs line breaking, `BiDi` run construction, font fallback, shaping, and measurement.
    pub fn layout(&self, text: &str, options: &TextOptions) -> Result<TextLayout> {
        options.validate()?;
        if text.len() > 4 * 1024 * 1024 {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "text exceeds engine hard limit",
            ));
        }
        let mut layout = match options.overflow {
            TextOverflow::Shrink => self.shrink_to_fit(text, options),
            TextOverflow::Ellipsis => self.ellipsize(text, options),
            _ => self.layout_at_size(text, options, options.font_size),
        }?;
        if contains_emoji(text) {
            layout
                .diagnostics
                .push(TextDiagnostic::ColorEmojiRequiresExporter);
        }
        Ok(layout)
    }

    fn layout_at_size(&self, text: &str, options: &TextOptions, size: Unit) -> Result<TextLayout> {
        match options.writing_mode {
            WritingMode::Horizontal => self.layout_horizontal_at_size(text, options, size),
            WritingMode::Vertical => self.layout_vertical_at_size(text, options, size),
        }
    }

    fn layout_horizontal_at_size(
        &self,
        text: &str,
        options: &TextOptions,
        size: Unit,
    ) -> Result<TextLayout> {
        let content_width = options
            .bounds
            .width
            .checked_sub(options.padding_inline.checked_scale(2_000_000)?)?;
        let raw_lines = break_lines(text, content_width, |candidate| {
            self.measure_line(candidate, &options.font, size)
        })?;
        let line_height = size.checked_scale(i64::from(options.line_height))?;
        let mut lines = Vec::with_capacity(raw_lines.len());
        let mut max_width = Unit::ZERO;
        for line in raw_lines {
            let runs =
                crate::text_shaping::shape_bidi_line(&line, &options.font, size, self.fonts)?;
            let width = sum_run_widths(&runs)?;
            max_width = max_width.max(width);
            lines.push(TextLine {
                source_text: line,
                runs,
                width,
                height: line_height,
            });
        }
        let natural_height = line_height.checked_scale(
            i64::try_from(lines.len()).map_err(|_| layout_error("line count overflow"))?
                * 1_000_000,
        )?;
        self.finish_layout(lines, max_width, natural_height, options, size)
    }

    fn layout_vertical_at_size(
        &self,
        text: &str,
        options: &TextOptions,
        size: Unit,
    ) -> Result<TextLayout> {
        let content_height = options
            .bounds
            .height
            .checked_sub(options.padding_inline.checked_scale(2_000_000)?)?;
        let raw_columns = break_lines(text, content_height, |candidate| {
            self.measure_vertical_line(candidate, &options.font, size)
        })?;
        let column_width = size.checked_scale(i64::from(options.line_height))?;
        let mut lines = Vec::with_capacity(raw_columns.len());
        let mut max_height = Unit::ZERO;
        for column in raw_columns {
            let runs =
                crate::text_shaping::shape_vertical_line(&column, &options.font, size, self.fonts)?;
            let height = sum_run_widths(&runs)?;
            max_height = max_height.max(height);
            lines.push(TextLine {
                source_text: column,
                runs,
                width: height,
                height: column_width,
            });
        }
        let natural_width = column_width.checked_scale(
            i64::try_from(lines.len()).map_err(|_| layout_error("column count overflow"))?
                * 1_000_000,
        )?;
        self.finish_layout(lines, natural_width, max_height, options, size)
    }

    fn finish_layout(
        &self,
        mut lines: Vec<TextLine>,
        natural_width: Unit,
        natural_height: Unit,
        options: &TextOptions,
        size: Unit,
    ) -> Result<TextLayout> {
        let line_overflow = options.max_lines.is_some_and(|max| lines.len() > max);
        let (measured_width, measured_height) = match options.writing_mode {
            WritingMode::Horizontal => (
                natural_width.checked_add(options.padding_inline.checked_scale(2_000_000)?)?,
                natural_height,
            ),
            WritingMode::Vertical => (
                natural_width,
                natural_height.checked_add(options.padding_inline.checked_scale(2_000_000)?)?,
            ),
        };
        let box_overflow =
            measured_width > options.bounds.width || measured_height > options.bounds.height;
        let mut diagnostics = Vec::new();
        if line_overflow || box_overflow {
            match options.overflow {
                TextOverflow::Error => {
                    return Err(layout_error("text does not fit requested bounds"))
                }
                TextOverflow::Clip | TextOverflow::Wrap => {
                    diagnostics.push(TextDiagnostic::Clipped);
                }
                TextOverflow::Expand => {}
                TextOverflow::Shrink | TextOverflow::Ellipsis => {
                    return Err(layout_error("invalid text overflow phase"))
                }
            }
        }
        if let Some(max) = options.max_lines {
            lines.truncate(max);
        }
        let measured = if options.overflow == TextOverflow::Expand {
            Size::new(measured_width, measured_height)?
        } else {
            options.bounds
        };
        Ok(TextLayout {
            writing_mode: options.writing_mode,
            lines,
            measured,
            font_size: size,
            paint_offset_y: Unit::ZERO,
            diagnostics,
            align_x: options.align_x,
            padding_inline: options.padding_inline,
            padding: Insets::default(),
        })
    }

    fn shrink_to_fit(&self, text: &str, options: &TextOptions) -> Result<TextLayout> {
        let mut size = options.font_size;
        loop {
            let mut adjusted = options.clone();
            adjusted.overflow = TextOverflow::Error;
            match self.layout_at_size(text, &adjusted, size) {
                Ok(mut layout) => {
                    if size != options.font_size {
                        layout.diagnostics.push(TextDiagnostic::Shrunk);
                    }
                    return Ok(layout);
                }
                Err(error) if error.code() == ErrorCode::LayoutInvalid => {}
                Err(error) => return Err(error),
            }
            if size <= options.min_font_size {
                return Err(layout_error("text does not fit at minimum font size"));
            }
            let next = size.checked_scale(950_000)?.max(options.min_font_size);
            if next == size {
                return Err(layout_error("font shrinking did not converge"));
            }
            size = next;
        }
    }

    fn ellipsize(&self, text: &str, options: &TextOptions) -> Result<TextLayout> {
        let mut adjusted = options.clone();
        adjusted.overflow = TextOverflow::Error;
        match self.layout_at_size(text, &adjusted, options.font_size) {
            Ok(layout) => return Ok(layout),
            Err(error) if error.code() == ErrorCode::LayoutInvalid => {}
            Err(error) => return Err(error),
        }
        let mut graphemes: Vec<&str> = text.graphemes(true).collect();
        loop {
            if graphemes.pop().is_none() {
                return Err(layout_error("ellipsis does not fit requested bounds"));
            }
            let candidate = format!("{}…", graphemes.concat());
            match self.layout_at_size(&candidate, &adjusted, options.font_size) {
                Ok(mut layout) => {
                    layout.diagnostics.push(TextDiagnostic::Ellipsized);
                    return Ok(layout);
                }
                Err(error) if error.code() == ErrorCode::LayoutInvalid => {}
                Err(error) => return Err(error),
            }
        }
    }

    fn measure_line(&self, text: &str, font: &str, size: Unit) -> Result<Unit> {
        sum_run_widths(&crate::text_shaping::shape_bidi_line(
            text, font, size, self.fonts,
        )?)
    }

    fn measure_vertical_line(&self, text: &str, font: &str, size: Unit) -> Result<Unit> {
        sum_run_widths(&crate::text_shaping::shape_vertical_line(
            text, font, size, self.fonts,
        )?)
    }
}
