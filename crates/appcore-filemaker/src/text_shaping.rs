// =============================================================================
//        #######
//     ###       ###     F: text_shaping.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Shapes Unicode text into deterministic runs using registered fonts.

use harfrust::{Direction, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use std::collections::BTreeMap;
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    ErrorCode, FileMakerError, FontManager, Glyph, GlyphRun, PdfStandardFont, Result, Unit,
};

pub(crate) fn shape_bidi_line(
    text: &str,
    primary: &str,
    size: Unit,
    fonts: &FontManager,
) -> Result<Vec<GlyphRun>> {
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
        let direction = if bidi.levels[range.start].is_rtl() {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        };
        result.extend(shape_with_fallback(
            &text[range],
            primary,
            size,
            direction,
            fonts,
        )?);
    }
    Ok(result)
}

pub(crate) fn shape_vertical_line(
    text: &str,
    primary: &str,
    size: Unit,
    fonts: &FontManager,
) -> Result<Vec<GlyphRun>> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    shape_with_fallback(text, primary, size, Direction::TopToBottom, fonts)
}

fn shape_with_fallback(
    text: &str,
    primary: &str,
    size: Unit,
    direction: Direction,
    fonts: &FontManager,
) -> Result<Vec<GlyphRun>> {
    let mut chunks: Vec<(&str, std::ops::Range<usize>)> = Vec::new();
    for (offset, grapheme) in text.grapheme_indices(true) {
        let font = fonts.select_name_for_grapheme(primary, grapheme)?;
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
        .map(|(font, range)| shape_run(&text[range], font, size, direction, fonts))
        .collect()
}

fn shape_run(
    text: &str,
    font_name: &str,
    size: Unit,
    direction: Direction,
    fonts: &FontManager,
) -> Result<GlyphRun> {
    if let Some(face) = fonts.standard_face(font_name) {
        if direction != Direction::LeftToRight {
            return Err(font_error(
                "PDF Standard fonts support horizontal left-to-right WinAnsi text only",
            ));
        }
        return shape_standard_run(text, font_name, face, size);
    }
    let font = fonts.get(font_name)?;
    let face = FontRef::from_index(&font.bytes, font.face_index)
        .map_err(|_| font_error("registered font cannot be shaped"))?;
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_direction(direction);
    let shaper_data = ShaperData::new(&face);
    let shaped = shaper_data
        .shaper(&face)
        .build()
        .shape(buffer, ShapeOptions::default());
    let upem = i64::from(font.units_per_em()?);
    let mut width = Unit::ZERO;
    let mut glyphs = Vec::with_capacity(shaped.len());
    for (info, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        let advance_x = scale_font_unit(position.x_advance, size, upem)?;
        let advance_y = scale_font_unit(position.y_advance, size, upem)?;
        let inline_advance = if matches!(direction, Direction::TopToBottom | Direction::BottomToTop)
        {
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

fn shape_standard_run(
    text: &str,
    font_name: &str,
    face: PdfStandardFont,
    size: Unit,
) -> Result<GlyphRun> {
    let metrics = face.metrics().metrics();
    let kerning = metrics
        .kerning_pairs
        .iter()
        .map(|pair| ((pair.left.as_ref(), pair.right.as_ref()), pair.adjust))
        .collect::<BTreeMap<_, _>>();
    let mut glyphs: Vec<Glyph> = Vec::with_capacity(text.chars().count());
    let mut width = Unit::ZERO;
    let mut previous_name: Option<&str> = None;
    let mut previous_index: Option<usize> = None;
    for (offset, character) in text.char_indices() {
        let byte = pdf_base14_metrics::winansi_byte(character).ok_or_else(|| {
            font_error(format!(
                "PDF Standard font cannot encode U+{:04X}",
                u32::from(character)
            ))
        })?;
        let glyph_name = pdf_base14_metrics::winansi_glyph_name(byte)
            .ok_or_else(|| font_error("PDF WinAnsi character has no glyph name"))?;
        let advance = face
            .metrics()
            .winansi_width(byte)
            .ok_or_else(|| font_error("PDF Standard font has no WinAnsi glyph width"))?;
        let advance = scale_afm_unit(advance, size)?;
        if let (Some(left), Some(index)) = (previous_name, previous_index) {
            let adjustment = kerning.get(&(left, glyph_name)).copied().unwrap_or(0.0);
            let kern = scale_afm_unit(adjustment, size)?;
            glyphs[index].advance_x = glyphs[index].advance_x.checked_add(kern)?;
            width = width.checked_add(kern)?;
        }
        glyphs.push(Glyph {
            id: u16::from(byte),
            cluster: u32::try_from(offset).map_err(|_| layout_error("text cluster exceeds u32"))?,
            advance_x: advance,
            advance_y: Unit::ZERO,
            offset_x: Unit::ZERO,
            offset_y: Unit::ZERO,
        });
        width = width.checked_add(advance)?;
        previous_name = Some(glyph_name);
        previous_index = Some(glyphs.len() - 1);
    }
    Ok(GlyphRun {
        font: font_name.to_owned(),
        rtl: false,
        text: text.to_owned(),
        glyphs,
        width,
    })
}

fn scale_afm_unit(value: f32, size: Unit) -> Result<Unit> {
    if !value.is_finite() || value.abs() > 1_000_000.0 {
        return Err(font_error("PDF Standard font metric is invalid"));
    }
    let milli_units = (value * 1000.0).round() as i128;
    Unit::from_ratio(
        milli_units * i128::from(size.raw()),
        1_000_000 * i128::from(Unit::PER_POINT),
    )
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

fn font_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::FontMissing, message)
}

fn layout_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}
