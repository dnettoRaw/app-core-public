// =============================================================================
//        #######
//     ###       ###     F: debug_export.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded debug export contracts and behavior for this crate.

use std::fmt;
use std::io::{sink, Write};

use pdf_writer::writers::{Catalog, DocumentInfo};
use pdf_writer::{Finish, Rect as PdfRect, Ref, TextStr};
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, Pixmap, Rect as TinyRect, Transform};

use crate::export::bounded_string::{
    output_limit_error, CountingOutput, FormattedOutput, StreamingOutput,
};
use crate::{CollisionMask, ErrorCode, FileMakerError, ResourceLimits, Result, Unit};

/// Collision-mask output representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskFormat {
    /// Stable geometry JSON.
    Json,
    /// Vector SVG.
    Svg,
    /// Raster PNG.
    Png,
    /// Vector PDF.
    Pdf,
}

/// Exports a derived mask without inspecting rendered pixels.
pub fn export_collision_mask(
    mask: &CollisionMask,
    format: MaskFormat,
    dpi: u32,
    limits: &ResourceLimits,
    writer: &mut dyn Write,
) -> Result<usize> {
    limits.validate()?;
    mask.validate_limits(limits)?;
    match format {
        MaskFormat::Json => json(mask, limits, writer),
        MaskFormat::Svg => svg(mask, limits, writer),
        MaskFormat::Png => png(mask, dpi, limits, writer),
        MaskFormat::Pdf => pdf(mask, limits, writer),
    }
}

fn json(mask: &CollisionMask, limits: &ResourceLimits, writer: &mut dyn Write) -> Result<usize> {
    let size = crate::memory::serialized_size_pretty(mask)?;
    if size > limits.max_output_bytes {
        return Err(FileMakerError::new(
            ErrorCode::LimitExceeded,
            "debug mask JSON exceeds the output budget",
        ));
    }
    serde_json::to_writer_pretty(writer, mask)
        .map_err(|error| FileMakerError::new(ErrorCode::ExportWrite, error.to_string()))?;
    Ok(size)
}

fn svg(mask: &CollisionMask, limits: &ResourceLimits, writer: &mut dyn Write) -> Result<usize> {
    let mut counter = CountingOutput::new(limits.max_output_bytes);
    write_svg(mask, &mut counter)?;
    let size = counter.finish()?;
    let mut output = StreamingOutput::new(writer, limits.max_output_bytes);
    write_svg(mask, &mut output)?;
    let written = output.finish()?;
    debug_assert_eq!(written, size);
    Ok(written)
}

fn write_svg(mask: &CollisionMask, output: &mut impl FormattedOutput) -> Result<()> {
    write!(
        output,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\">",
        points(mask.size.width),
        points(mask.size.height)
    )
    .map_err(format_error)?;
    for (id, bounds) in &mask.occupied {
        output.push_str("<rect data-element=\"")?;
        write_escaped(output, id.as_str())?;
        write!(
            output,
            "\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ff000033\" stroke=\"#cc0000\"/>",
            points(bounds.origin.x),
            points(bounds.origin.y),
            points(bounds.size.width),
            points(bounds.size.height)
        )
        .map_err(format_error)?;
    }
    for (_, _, bounds) in &mask.collisions {
        write!(
            output,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ff00ff99\"/>",
            points(bounds.origin.x),
            points(bounds.origin.y),
            points(bounds.size.width),
            points(bounds.size.height)
        )
        .map_err(format_error)?;
    }
    output.push_str("</svg>")
}

fn png(
    mask: &CollisionMask,
    dpi: u32,
    limits: &ResourceLimits,
    writer: &mut dyn Write,
) -> Result<usize> {
    if dpi == 0 || dpi > 9_600 {
        return Err(mask_error("mask DPI must be between 1 and 9600"));
    }
    let scale = f64::from(dpi) / 72.0;
    let width = pixels(mask.size.width, scale)?;
    let height = pixels(mask.size.height, scale)?;
    if u64::from(width) * u64::from(height) > limits.max_pixels {
        return Err(FileMakerError::new(
            ErrorCode::LimitExceeded,
            "mask pixel count exceeds configured limit",
        ));
    }
    let tile_rows = crate::export::bounded_tile_rows(width)?;
    let occupied = paint(0.8, 0.0, 0.0, 0.22)?;
    let collision = paint(1.0, 0.0, 1.0, 0.6)?;
    crate::export::encode_png_tiled(
        width,
        height,
        tile_rows,
        limits.max_output_bytes,
        writer,
        &mut |top, rows| {
            let mut pixmap = Pixmap::new(width, rows).ok_or_else(|| {
                FileMakerError::new(ErrorCode::LimitExceeded, "cannot allocate mask strip")
            })?;
            pixmap.fill(tiny_skia::Color::WHITE);
            for (_, bounds) in &mask.occupied {
                fill_rect(&mut pixmap, *bounds, scale, top, &occupied)?;
            }
            for (_, _, bounds) in &mask.collisions {
                fill_rect(&mut pixmap, *bounds, scale, top, &collision)?;
            }
            Ok(pixmap)
        },
    )
}

fn pdf(mask: &CollisionMask, limits: &ResourceLimits, writer: &mut dyn Write) -> Result<usize> {
    let mut counter = CountingOutput::new(limits.max_output_bytes);
    write_pdf_content(mask, &mut counter)?;
    let content_size = counter.finish()?;
    let expected = write_pdf(mask, content_size, limits, &mut sink())?;
    let written = write_pdf(mask, content_size, limits, writer)?;
    if written != expected {
        return Err(FileMakerError::new(
            ErrorCode::Validation,
            "debug PDF changed between sizing and streaming",
        ));
    }
    Ok(written)
}

fn write_pdf(
    mask: &CollisionMask,
    content_size: usize,
    limits: &ResourceLimits,
    writer: &mut dyn Write,
) -> Result<usize> {
    let catalog = Ref::new(1);
    let info = Ref::new(2);
    let tree = Ref::new(3);
    let page = Ref::new(4);
    let stream = Ref::new(5);
    let mut pdf = crate::export::pdf_stream::PdfDocument::new(
        writer,
        limits.max_output_bytes,
        catalog,
        info,
    )?;
    let width = points(mask.size.width);
    let height = points(mask.size.height);
    pdf.object(catalog, |chunk| {
        let mut catalog_writer: Catalog<'_> = chunk.indirect(catalog).start();
        catalog_writer.pages(tree);
        Ok(())
    })?;
    pdf.object(info, |chunk| {
        let mut info_writer: DocumentInfo<'_> = chunk.indirect(info).start();
        info_writer
            .title(TextStr("AppCore FileMaker collision mask"))
            .producer(TextStr(concat!(
                "appcore-filemaker ",
                env!("CARGO_PKG_VERSION")
            )));
        Ok(())
    })?;
    pdf.object(tree, |chunk| {
        chunk.pages(tree).kids([page]).count(1);
        Ok(())
    })?;
    pdf.object(page, |chunk| {
        let mut page_writer = chunk.page(page);
        page_writer
            .media_box(PdfRect::new(0.0, 0.0, width, height))
            .parent(tree)
            .contents(stream);
        page_writer.finish();
        Ok(())
    })?;
    pdf.stream_object(stream, content_size, |writer| {
        let mut output = StreamingOutput::new(writer, content_size);
        write_pdf_content(mask, &mut output)?;
        let written = output.finish()?;
        if written != content_size {
            return Err(FileMakerError::new(
                ErrorCode::Validation,
                "debug PDF content changed after sizing",
            ));
        }
        Ok(())
    })?;
    pdf.finish()
}

fn write_pdf_content(mask: &CollisionMask, output: &mut impl FormattedOutput) -> Result<()> {
    output.push_str("0.8 0 0 rg\n")?;
    for (_, bounds) in &mask.occupied {
        write_pdf_rect(output, mask.size.height, *bounds)?;
    }
    output.push_str("1 0 1 rg\n")?;
    for (_, _, bounds) in &mask.collisions {
        write_pdf_rect(output, mask.size.height, *bounds)?;
    }
    Ok(())
}

fn write_pdf_rect(
    output: &mut impl FormattedOutput,
    page_height: Unit,
    bounds: crate::Rect,
) -> Result<()> {
    let y = page_height
        .checked_sub(bounds.origin.y)?
        .checked_sub(bounds.size.height)?;
    write!(
        output,
        "{} {} {} {} re\nf\n",
        PdfUnit(bounds.origin.x),
        PdfUnit(y),
        PdfUnit(bounds.size.width),
        PdfUnit(bounds.size.height),
    )
    .map_err(format_error)
}

struct PdfUnit(Unit);

impl fmt::Display for PdfUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let raw = i128::from(self.0.raw());
        if raw < 0 {
            formatter.write_str("-")?;
        }
        let absolute = raw.abs();
        let whole = absolute / i128::from(Unit::PER_POINT);
        let mut fraction = absolute % i128::from(Unit::PER_POINT);
        write!(formatter, "{whole}")?;
        if fraction == 0 {
            return Ok(());
        }
        let mut width = 6;
        while fraction % 10 == 0 {
            fraction /= 10;
            width -= 1;
        }
        write!(formatter, ".{fraction:0width$}")
    }
}

fn fill_rect(
    pixmap: &mut Pixmap,
    bounds: crate::Rect,
    scale: f64,
    strip_top: u32,
    paint: &Paint<'_>,
) -> Result<()> {
    let rect = TinyRect::from_xywh(
        points(bounds.origin.x) * scale as f32,
        points(bounds.origin.y) * scale as f32,
        points(bounds.size.width) * scale as f32,
        points(bounds.size.height) * scale as f32,
    )
    .ok_or_else(|| mask_error("mask rectangle is invalid"))?;
    pixmap.fill_rect(
        rect,
        paint,
        Transform::from_translate(0.0, -(strip_top as f32)),
        None,
    );
    Ok(())
}

fn paint(r: f32, g: f32, b: f32, a: f32) -> Result<Paint<'static>> {
    let mut paint = Paint::default();
    let color = tiny_skia::Color::from_rgba(r, g, b, a)
        .ok_or_else(|| mask_error("mask paint is invalid"))?;
    paint.set_color(color);
    Ok(paint)
}

fn pixels(value: Unit, scale: f64) -> Result<u32> {
    let pixels = (value.as_points_f64() * scale).ceil();
    if !pixels.is_finite() || pixels <= 0.0 || pixels > f64::from(u32::MAX) {
        return Err(mask_error("mask pixel dimension is invalid"));
    }
    Ok(pixels as u32)
}

fn points(value: Unit) -> f32 {
    value.as_points_f64() as f32
}

fn write_escaped(output: &mut impl FormattedOutput, value: &str) -> Result<()> {
    let mut start = 0;
    for (index, character) in value.char_indices() {
        let replacement = match character {
            '&' => "&amp;",
            '"' => "&quot;",
            '<' => "&lt;",
            '>' => "&gt;",
            _ => continue,
        };
        output.push_str(&value[start..index])?;
        output.push_str(replacement)?;
        start = index + character.len_utf8();
    }
    output.push_str(&value[start..])
}

fn format_error(_: std::fmt::Error) -> FileMakerError {
    output_limit_error()
}

fn mask_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::ExportWrite, message)
}

#[cfg(test)]
mod tests {
    use super::PdfUnit;
    use crate::Unit;

    #[test]
    fn pdf_units_are_exact_non_exponential_fixed_point_numbers() {
        for (raw, expected) in [
            (0, "0"),
            (1, "0.000001"),
            (1_230_000, "1.23"),
            (-250_000, "-0.25"),
            (i64::MIN, "-9223372036854.775808"),
        ] {
            assert_eq!(PdfUnit(Unit::from_raw(raw)).to_string(), expected);
        }
    }
}
