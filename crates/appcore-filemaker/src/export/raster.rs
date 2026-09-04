// =============================================================================
//        #######
//     ###       ###     F: raster.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded raster contracts and behavior for this crate.

use std::collections::BTreeSet;
use std::io::Write;

use tiny_skia::{
    FillRule, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform as SkTransform,
};

use super::core::{record_text_capability_losses, selected_pages};
use super::progress::ExportProgress;
use super::raster_plan::RasterPlan;
use crate::{
    Color, ElementKind, ErrorCode, ExportCapabilities, ExportContext, ExportFormat, ExportLossKind,
    ExportLossReport, ExportOutcome, ExportRequest, FileMakerError, ResolvedElement, ResolvedScene,
    Result, Shape,
};

pub(super) fn export(
    scene: &ResolvedScene,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    progress: &mut ExportProgress<'_>,
    writer: &mut dyn Write,
) -> Result<ExportOutcome> {
    let pages = selected_pages(scene, request)?;
    let plan = RasterPlan::new(&pages, request.dpi, context)?;
    progress.checkpoint()?;
    let mut losses = ExportLossReport::default();
    for page in &plan.pages {
        for element in &page.page.elements {
            analyze_element(element, context, request.format, &mut losses)?;
            progress.element()?;
        }
    }
    losses.enforce(request.fidelity)?;
    let bytes_written = super::raster_encode::encode_tiled(
        plan.width,
        plan.height,
        plan.tile_rows,
        request,
        context,
        writer,
        &mut |top, height| {
            progress.checkpoint()?;
            plan.render_strip(top, height, context, request.format)
        },
    )?;
    let mut capabilities = BTreeSet::from([
        ExportCapabilities::MultiPage,
        ExportCapabilities::Raster,
        ExportCapabilities::Images,
    ]);
    if request.format == ExportFormat::Png {
        capabilities.insert(ExportCapabilities::Transparency);
    }
    Ok(ExportOutcome {
        bytes_written,
        loss_report: losses,
        capabilities,
    })
}

fn analyze_element(
    element: &ResolvedElement,
    context: &ExportContext<'_>,
    format: ExportFormat,
    losses: &mut ExportLossReport,
) -> Result<()> {
    if has_cmyk(element) {
        losses.push(
            ExportLossKind::CmykConvertedToRgb,
            Some(element.id.as_str()),
            "raster pixels use RGB channels",
        );
    }
    record_text_capability_losses(element, losses);
    if format == ExportFormat::Jpeg && has_transparency(element) {
        losses.push(
            ExportLossKind::TransparencyFlattened,
            Some(element.id.as_str()),
            "JPEG composited transparency on white",
        );
    }
    match element.kind {
        ElementKind::Image => analyze_image(element, context, format, losses),
        ElementKind::Chart | ElementKind::Qr | ElementKind::Barcode => {
            losses.push(
                ExportLossKind::UnsupportedElement,
                Some(element.id.as_str()),
                "prepared element kind has no raster renderer",
            );
            Ok(())
        }
        _ => Ok(()),
    }
}

fn analyze_image(
    element: &ResolvedElement,
    context: &ExportContext<'_>,
    format: ExportFormat,
    losses: &mut ExportLossReport,
) -> Result<()> {
    let (Some(name), Some(resolver)) = (&element.asset, context.assets) else {
        losses.push(
            ExportLossKind::ImageOmitted,
            Some(element.id.as_str()),
            "image asset/resolver is missing",
        );
        return Ok(());
    };
    let Some(placement) = element.image_placement else {
        losses.push(
            ExportLossKind::ImageOmitted,
            Some(element.id.as_str()),
            "image geometry was not resolved during layout",
        );
        return Ok(());
    };
    if placement.vector {
        losses.push(
            ExportLossKind::UnsupportedElement,
            Some(element.id.as_str()),
            "raster export does not rasterize SVG assets",
        );
        return Ok(());
    }
    if format == ExportFormat::Jpeg {
        let asset = resolver.resolve_asset(name, context.limits.max_asset_bytes)?;
        if crate::validation_capability::raster_asset_has_alpha(&asset, placement)? {
            losses.push(
                ExportLossKind::TransparencyFlattened,
                Some(element.id.as_str()),
                "JPEG composited image alpha on white",
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_element(
    pixmap: &mut Pixmap,
    element: &ResolvedElement,
    context: &ExportContext<'_>,
    scale: f32,
    page_y: f32,
) -> Result<()> {
    match element.kind {
        ElementKind::Image => render_image(pixmap, element, context, scale, page_y),
        ElementKind::Text => render_text(pixmap, element, context, scale, page_y),
        ElementKind::Table => super::table_raster::render(pixmap, element, context, scale, page_y),
        ElementKind::Chart | ElementKind::Qr | ElementKind::Barcode => Ok(()),
        _ => render_shape(pixmap, element, scale, page_y),
    }
}

fn render_shape(
    pixmap: &mut Pixmap,
    element: &ResolvedElement,
    scale: f32,
    page_y: f32,
) -> Result<()> {
    let path = path_for(element, scale, page_y)?;
    let transform = raster_transform(element.transform, scale, page_y);
    if let Some(fill) = element.style.fill {
        let mut paint = paint(fill, element.style.opacity);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
    }
    if let Some(stroke_color) = element.style.stroke {
        let stroke = Stroke {
            width: to_pixel(element.style.stroke_width, scale),
            ..Stroke::default()
        };
        let mut paint = paint(stroke_color, element.style.opacity);
        paint.anti_alias = true;
        pixmap.stroke_path(&path, &paint, &stroke, transform, None);
    }
    Ok(())
}

fn path_for(element: &ResolvedElement, scale: f32, page_y: f32) -> Result<Path> {
    let bounds = element.bounds.layout;
    let x = to_pixel(bounds.origin.x, scale);
    let y = to_pixel(bounds.origin.y, scale) + page_y;
    let width = to_pixel(bounds.size.width, scale);
    let height = to_pixel(bounds.size.height, scale);
    let rect = tiny_skia::Rect::from_xywh(x, y, width.max(0.001), height.max(0.001))
        .ok_or_else(|| export_error("invalid raster bounds"))?;
    let mut builder = PathBuilder::new();
    match element.shape {
        Shape::Ellipse { .. } => builder.push_oval(rect),
        Shape::Path { ref commands, .. } => {
            for command in commands {
                match command {
                    crate::PathCommand::Move { to } => {
                        builder.move_to(to_pixel(to.x, scale), to_pixel(to.y, scale) + page_y)
                    }
                    crate::PathCommand::Line { to } => {
                        builder.line_to(to_pixel(to.x, scale), to_pixel(to.y, scale) + page_y)
                    }
                    crate::PathCommand::Curve {
                        control_1,
                        control_2,
                        to,
                    } => builder.cubic_to(
                        to_pixel(control_1.x, scale),
                        to_pixel(control_1.y, scale) + page_y,
                        to_pixel(control_2.x, scale),
                        to_pixel(control_2.y, scale) + page_y,
                        to_pixel(to.x, scale),
                        to_pixel(to.y, scale) + page_y,
                    ),
                    crate::PathCommand::Close => builder.close(),
                }
            }
        }
        Shape::Polygon { ref points } => {
            if let Some(first) = points.first() {
                builder.move_to(to_pixel(first.x, scale), to_pixel(first.y, scale) + page_y);
                for point in &points[1..] {
                    builder.line_to(to_pixel(point.x, scale), to_pixel(point.y, scale) + page_y);
                }
                builder.close();
            } else {
                builder.push_rect(rect);
            }
        }
        Shape::Rect { .. } => builder.push_rect(rect),
    }
    builder
        .finish()
        .ok_or_else(|| export_error("cannot build raster path"))
}

fn render_text(
    pixmap: &mut Pixmap,
    element: &ResolvedElement,
    context: &ExportContext<'_>,
    scale: f32,
    page_y: f32,
) -> Result<()> {
    let layout = element
        .text_layout
        .as_ref()
        .ok_or_else(|| export_error("resolved text has no glyph layout"))?;
    let element_transform = raster_transform(element.transform, scale, page_y);
    super::raster_text::render_layout(
        pixmap,
        layout,
        element.bounds.layout,
        &element.style,
        context,
        scale,
        page_y,
        element_transform,
        element.bounds.clip,
    )
}

fn render_image(
    pixmap: &mut Pixmap,
    element: &ResolvedElement,
    context: &ExportContext<'_>,
    scale: f32,
    page_y: f32,
) -> Result<()> {
    let (Some(name), Some(resolver)) = (&element.asset, context.assets) else {
        return Ok(());
    };
    let Some(placement) = element.image_placement else {
        return Ok(());
    };
    if placement.vector {
        return Ok(());
    }
    let asset = resolver.resolve_asset(name, context.limits.max_asset_bytes)?;
    let mut decoded = image::load_from_memory(&asset.bytes)
        .map_err(|error| export_error(format!("cannot decode image: {error}")))?;
    placement.orientation.apply(&mut decoded);
    let decoded = decoded
        .crop_imm(
            placement.source.x,
            placement.source.y,
            placement.source.width,
            placement.source.height,
        )
        .to_rgba8();
    let width = decoded.width();
    let height = decoded.height();
    let mut source =
        Pixmap::new(width, height).ok_or_else(|| limit_error("cannot allocate image pixmap"))?;
    for (target, rgba) in source.pixels_mut().iter_mut().zip(decoded.pixels()) {
        *target = tiny_skia::PremultipliedColorU8::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3])
            .ok_or_else(|| export_error("invalid image pixel"))?;
    }
    let destination = placement.destination;
    let target_width = to_pixel(destination.size.width, scale);
    let target_height = to_pixel(destination.size.height, scale);
    let image_transform = SkTransform::from_row(
        target_width / width as f32,
        0.0,
        0.0,
        target_height / height as f32,
        to_pixel(destination.origin.x, scale),
        to_pixel(destination.origin.y, scale) + page_y,
    );
    let clip = placement.clip;
    let element_transform = raster_transform(element.transform, scale, page_y);
    let mask = geometry_clip_mask(
        pixmap.width(),
        pixmap.height(),
        clip,
        element_transform,
        scale,
        page_y,
    )?;
    pixmap.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &PixmapPaint::default(),
        element_transform.pre_concat(image_transform),
        Some(&mask),
    );
    Ok(())
}

pub(super) fn geometry_clip_mask(
    width: u32,
    height: u32,
    clip: crate::Rect,
    transform: SkTransform,
    scale: f32,
    page_y: f32,
) -> Result<Mask> {
    let mut mask = Mask::new(width, height)
        .ok_or_else(|| limit_error("cannot allocate geometry clip mask"))?;
    let clip_rect = tiny_skia::Rect::from_xywh(
        to_pixel(clip.origin.x, scale),
        to_pixel(clip.origin.y, scale) + page_y,
        to_pixel(clip.size.width, scale),
        to_pixel(clip.size.height, scale),
    )
    .ok_or_else(|| export_error("invalid image clip bounds"))?;
    let mut clip_path = PathBuilder::new();
    clip_path.push_rect(clip_rect);
    let clip_path = clip_path
        .finish()
        .ok_or_else(|| export_error("cannot build image clip path"))?;
    mask.fill_path(&clip_path, FillRule::Winding, true, transform);
    Ok(mask)
}

pub(super) fn paint(color: Color, opacity: u32) -> Paint<'static> {
    let [r, g, b, a] = color.to_rgba();
    let effective_alpha =
        (u64::from(a) * u64::from(opacity) / 1_000_000).min(u64::from(u8::MAX)) as u8;
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, effective_alpha);
    paint
}

pub(super) fn to_pixel(value: crate::Unit, scale: f32) -> f32 {
    value.as_points_f64() as f32 * scale
}

pub(super) fn raster_transform(
    transform: crate::Transform,
    scale: f32,
    page_y: f32,
) -> SkTransform {
    let a = transform.a as f32 / 1_000_000.0;
    let b = transform.b as f32 / 1_000_000.0;
    let c = transform.c as f32 / 1_000_000.0;
    let d = transform.d as f32 / 1_000_000.0;
    SkTransform::from_row(
        a,
        b,
        c,
        d,
        to_pixel(transform.tx, scale) - c * page_y,
        to_pixel(transform.ty, scale) + (1.0 - d) * page_y,
    )
}

fn has_cmyk(element: &ResolvedElement) -> bool {
    let base = [
        element.style.fill,
        element.style.stroke,
        Some(element.style.color),
    ]
    .into_iter()
    .flatten()
    .any(|value| matches!(value, Color::Cmyk { .. }));
    base || element.table.as_ref().is_some_and(|table| {
        table
            .header
            .iter()
            .chain(table.rows.iter().flat_map(|row| &row.cells))
            .chain(&table.totals)
            .any(|cell| {
                [cell.style.fill, cell.style.stroke, Some(cell.style.color)]
                    .into_iter()
                    .flatten()
                    .any(|value| matches!(value, Color::Cmyk { .. }))
            })
    })
}

fn has_transparency(element: &ResolvedElement) -> bool {
    let base = element.style.opacity < 1_000_000
        || [
            element.style.fill,
            element.style.stroke,
            Some(element.style.color),
        ]
        .into_iter()
        .flatten()
        .any(|value| matches!(value, Color::Rgba { a, .. } if a < 255));
    base || element.table.as_ref().is_some_and(|table| {
        table
            .header
            .iter()
            .chain(table.rows.iter().flat_map(|row| &row.cells))
            .chain(&table.totals)
            .any(|cell| {
                cell.style.opacity < 1_000_000
                    || [cell.style.fill, cell.style.stroke, Some(cell.style.color)]
                        .into_iter()
                        .flatten()
                        .any(|value| matches!(value, Color::Rgba { a, .. } if a < 255))
            })
    })
}

fn export_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::ExportUnsupported, message)
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}
