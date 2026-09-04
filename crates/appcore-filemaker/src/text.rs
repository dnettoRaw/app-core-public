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

use harfrust::{Direction, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use serde::{Deserialize, Serialize};
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

use crate::{ErrorCode, FileMakerError, FontManager, Result, Size, Unit};

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
    /// Non-fatal diagnostics.
    pub diagnostics: Vec<TextDiagnostic>,
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
        let raw_lines = break_lines(text, options.bounds.width, |candidate| {
            self.measure_line(candidate, &options.font, size)
        })?;
        let line_height = size.checked_scale(i64::from(options.line_height))?;
        let mut lines = Vec::with_capacity(raw_lines.len());
        let mut max_width = Unit::ZERO;
        for line in raw_lines {
            let runs = self.shape_bidi_line(&line, &options.font, size)?;
            let width = sum_run_widths(&runs)?;
            max_width = max_width.max(width);
            lines.push(TextLine {
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
        let raw_columns = break_lines(text, options.bounds.height, |candidate| {
            self.measure_vertical_line(candidate, &options.font, size)
        })?;
        let column_width = size.checked_scale(i64::from(options.line_height))?;
        let mut lines = Vec::with_capacity(raw_columns.len());
        let mut max_height = Unit::ZERO;
        for column in raw_columns {
            let runs = self.shape_vertical_line(&column, &options.font, size)?;
            let height = sum_run_widths(&runs)?;
            max_height = max_height.max(height);
            lines.push(TextLine {
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
        let box_overflow =
            natural_width > options.bounds.width || natural_height > options.bounds.height;
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
            Size::new(natural_width, natural_height)?
        } else {
            options.bounds
        };
        Ok(TextLayout {
            writing_mode: options.writing_mode,
            lines,
            measured,
            font_size: size,
            diagnostics,
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
        sum_run_widths(&self.shape_bidi_line(text, font, size)?)
    }

    fn measure_vertical_line(&self, text: &str, font: &str, size: Unit) -> Result<Unit> {
        sum_run_widths(&self.shape_vertical_line(text, font, size)?)
    }

    fn shape_bidi_line(&self, text: &str, primary: &str, size: Unit) -> Result<Vec<GlyphRun>> {
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let bidi = BidiInfo::new(text, None);
        let paragraph = bidi
            .paragraphs
            .first()
            .ok_or_else(|| layout_error("BiDi paragraph is missing"))?;
        let (_, visual_runs) = bidi.visual_runs(paragraph, 0..text.len());
        let mut result = Vec::new();
        for range in visual_runs {
            let rtl = bidi.levels[range.start].is_rtl();
            let direction = if rtl {
                Direction::RightToLeft
            } else {
                Direction::LeftToRight
            };
            result.extend(self.shape_font_fallback(&text[range], primary, size, direction)?);
        }
        Ok(result)
    }

    fn shape_vertical_line(&self, text: &str, primary: &str, size: Unit) -> Result<Vec<GlyphRun>> {
        if text.is_empty() {
            return Ok(Vec::new());
        }
        self.shape_font_fallback(text, primary, size, Direction::TopToBottom)
    }

    fn shape_font_fallback(
        &self,
        text: &str,
        primary: &str,
        size: Unit,
        direction: Direction,
    ) -> Result<Vec<GlyphRun>> {
        let mut chunks: Vec<(&str, std::ops::Range<usize>)> = Vec::new();
        for (offset, grapheme) in text.grapheme_indices(true) {
            let font = self
                .fonts
                .select_for_grapheme(primary, grapheme)?
                .name
                .as_str();
            let end = offset + grapheme.len();
            if let Some((last_font, range)) = chunks.last_mut() {
                if *last_font == font {
                    range.end = end;
                    continue;
                }
            }
            chunks.push((font, offset..end));
        }
        if direction == Direction::RightToLeft {
            chunks.reverse();
        }
        chunks
            .into_iter()
            .map(|(font, range)| self.shape_run(&text[range], font, size, direction))
            .collect()
    }

    fn shape_run(
        &self,
        text: &str,
        font_name: &str,
        size: Unit,
        direction: Direction,
    ) -> Result<GlyphRun> {
        let font = self.fonts.get(font_name)?;
        let face = FontRef::from_index(&font.bytes, font.face_index)
            .map_err(|_| font_error("registered font cannot be shaped"))?;
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(direction);
        let shaper_data = ShaperData::new(&face);
        let shaper = shaper_data.shaper(&face).build();
        let shaped = shaper.shape(buffer, ShapeOptions::default());
        let upem = i64::from(font.units_per_em()?);
        let mut width = Unit::ZERO;
        let mut glyphs = Vec::with_capacity(shaped.len());
        for (info, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            let advance_x = scale_font_unit(position.x_advance, size, upem)?;
            let advance_y = scale_font_unit(position.y_advance, size, upem)?;
            let inline_advance =
                if matches!(direction, Direction::TopToBottom | Direction::BottomToTop) {
                    absolute_unit(advance_y)?
                } else {
                    absolute_unit(advance_x)?
                };
            width = width.checked_add(inline_advance)?;
            glyphs.push(Glyph {
                id: u16::try_from(info.glyph_id).map_err(|_| font_error("glyph ID exceeds u16"))?,
                cluster: info.cluster,
                advance_x,
                advance_y,
                offset_x: scale_font_unit(position.x_offset, size, upem)?,
                offset_y: scale_font_unit(position.y_offset, size, upem)?,
            });
        }
        Ok(GlyphRun {
            font: font_name.to_owned(),
            rtl: direction == Direction::RightToLeft,
            text: text.to_owned(),
            glyphs,
            width,
        })
    }
}

pub(crate) fn break_lines(
    text: &str,
    max_width: Unit,
    mut measure: impl FnMut(&str) -> Result<Unit>,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    let mut probes = 0_usize;
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for part in paragraph.split_word_bounds() {
            let candidate = format!("{line}{part}");
            let candidate_width = measure_bounded(&candidate, &mut probes, &mut measure)?;
            if !line.is_empty() && candidate_width > max_width {
                lines.push(line.trim_end().to_owned());
                line.clear();
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

fn append_overlong(
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

fn measure_bounded(
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

fn sum_run_widths(runs: &[GlyphRun]) -> Result<Unit> {
    runs.iter()
        .try_fold(Unit::ZERO, |total, run| total.checked_add(run.width))
}

fn scale_font_unit(value: i32, size: Unit, units_per_em: i64) -> Result<Unit> {
    Unit::from_ratio(
        i128::from(value) * i128::from(size.raw()),
        i128::from(units_per_em) * i128::from(Unit::PER_POINT),
    )
}

fn absolute_unit(value: Unit) -> Result<Unit> {
    value
        .raw()
        .checked_abs()
        .map(Unit::from_raw)
        .ok_or_else(|| layout_error("glyph advance overflow"))
}

fn layout_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}

fn font_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::FontMissing, message)
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}

fn contains_emoji(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(
            character as u32,
            0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0xFE0F | 0x200D
        )
    })
}
