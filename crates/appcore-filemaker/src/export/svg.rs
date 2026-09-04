// =============================================================================
//        #######
//     ###       ###     F: svg.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded svg contracts and behavior for this crate.

use std::collections::BTreeSet;
use std::io::Write;

use super::bounded_string::{output_limit_error, CountingOutput, FormattedOutput, StreamingOutput};
use super::core::{record_text_capability_losses, selected_pages, text_fonts};
use super::markup::{color, escape, normalized_image_bytes, opacity, points, write_base64};
use super::progress::ExportProgress;
use crate::{
    Color, ElementKind, ErrorCode, ExportCapabilities, ExportContext, ExportLossKind,
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
    let width = pages
        .iter()
        .map(|page| page.size.width)
        .max()
        .ok_or_else(|| {
            FileMakerError::new(ErrorCode::ExportUnsupported, "SVG has no selected page")
        })?;
    let height = pages.iter().try_fold(crate::Unit::ZERO, |total, page| {
        total.checked_add(page.size.height)
    })?;
    let mut losses = ExportLossReport::default();
    for element in pages.iter().flat_map(|page| &page.elements) {
        analyze_element(element, context, &mut losses);
    }
    losses.enforce(request.fidelity)?;
    let mut counter = CountingOutput::new(context.limits.max_output_bytes);
    write_svg(
        &mut counter,
        &pages,
        width,
        height,
        context,
        Some(progress),
        true,
    )?;
    let expected = counter.finish()?;
    progress.checkpoint()?;
    let mut svg = StreamingOutput::new(writer, context.limits.max_output_bytes);
    write_svg(&mut svg, &pages, width, height, context, None, false)?;
    let bytes_written = svg.finish()?;
    if bytes_written != expected {
        return Err(FileMakerError::new(
            ErrorCode::Validation,
            "SVG resolver output changed between sizing and streaming",
        ));
    }
    Ok(ExportOutcome {
        bytes_written,
        loss_report: losses,
        capabilities: BTreeSet::from([
            ExportCapabilities::MultiPage,
            ExportCapabilities::EditableText,
            ExportCapabilities::EmbeddedFonts,
            ExportCapabilities::Vector,
            ExportCapabilities::Transparency,
            ExportCapabilities::Images,
        ]),
    })
}

fn write_svg(
    svg: &mut dyn FormattedOutput,
    pages: &[&crate::ResolvedPage],
    width: crate::Unit,
    height: crate::Unit,
    context: &ExportContext<'_>,
    mut progress: Option<&mut ExportProgress<'_>>,
    advance_progress: bool,
) -> Result<()> {
    write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {} {}\">",
        points(width),
        points(height),
        points(width),
        points(height)
    )
    .map_err(format_error)?;
    embed_fonts(svg, pages, context)?;
    let mut page_y = crate::Unit::ZERO;
    for page in pages {
        write!(
            svg,
            "<g data-page=\"{}\" transform=\"translate(0 {})\">",
            page.index,
            points(page_y)
        )
        .map_err(format_error)?;
        for element in &page.elements {
            render_element(svg, element, context)?;
            if let Some(progress) = progress.as_deref_mut() {
                if advance_progress {
                    progress.element()?;
                } else {
                    progress.checkpoint()?;
                }
            }
        }
        svg.push_str("</g>")?;
        page_y = page_y.checked_add(page.size.height)?;
    }
    svg.push_str("</svg>")?;
    Ok(())
}

fn embed_fonts(
    svg: &mut dyn FormattedOutput,
    pages: &[&crate::ResolvedPage],
    context: &ExportContext<'_>,
) -> Result<()> {
    let names: BTreeSet<&str> = pages
        .iter()
        .flat_map(|page| &page.elements)
        .flat_map(|element| text_fonts(element))
        .collect();
    if names.is_empty() {
        return Ok(());
    }
    svg.push_str("<defs><style>")?;
    for name in names {
        let font = context.fonts.get(name)?;
        write!(
            svg,
            "@font-face{{font-family:'{}';src:url(data:font/ttf;base64,",
            escape(name)
        )
        .map_err(format_error)?;
        write_base64(svg, &font.bytes)?;
        svg.push_str(") format('truetype')}")?;
    }
    svg.push_str("</style></defs>")?;
    Ok(())
}

fn analyze_element(
    element: &ResolvedElement,
    context: &ExportContext<'_>,
    losses: &mut ExportLossReport,
) {
    if contains_cmyk(element) {
        losses.push(
            ExportLossKind::CmykConvertedToRgb,
            Some(element.id.as_str()),
            "SVG has no native CMYK paint",
        );
    }
    record_text_capability_losses(element, losses);
    match element.kind {
        ElementKind::Table => super::table_svg::record_losses(element, losses),
        ElementKind::Image => {
            let message = if element.asset.is_none() {
                Some("image has no asset reference")
            } else if context.assets.is_none() {
                Some("image resolver was not supplied")
            } else if element.image_placement.is_none() {
                Some("image geometry was not resolved during layout")
            } else {
                None
            };
            if let Some(message) = message {
                losses.push(
                    ExportLossKind::ImageOmitted,
                    Some(element.id.as_str()),
                    message,
                );
            }
        }
        ElementKind::Chart | ElementKind::Qr | ElementKind::Barcode => losses.push(
            ExportLossKind::UnsupportedElement,
            Some(element.id.as_str()),
            "prepared element kind has no SVG renderer",
        ),
        _ => {}
    }
}

fn render_element(
    svg: &mut dyn FormattedOutput,
    element: &ResolvedElement,
    context: &ExportContext<'_>,
) -> Result<()> {
    let rect = element.bounds.layout;
    let style = svg_style(element);
    let mut groups = 0_u8;
    if !element.transform.is_identity() {
        write!(
            svg,
            "<g transform=\"{}\">",
            svg_transform(element.transform)
        )
        .map_err(format_error)?;
        groups += 1;
    }
    if let Some(clip) = element.bounds.clip {
        let clip_id = format!("fm-clip-{}", element.id.as_str());
        write!(
            svg,
            "<defs><clipPath id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#{})\">",
            escape(&clip_id),
            points(clip.origin.x),
            points(clip.origin.y),
            points(clip.size.width),
            points(clip.size.height),
            escape(&clip_id),
        )
        .map_err(format_error)?;
        groups += 1;
    }
    match element.kind {
        ElementKind::Rect | ElementKind::Group => write!(
            svg,
            "<rect id=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" {style}/>",
            escape(element.id.as_str()),
            points(rect.origin.x),
            points(rect.origin.y),
            points(rect.size.width),
            points(rect.size.height)
        )
        .map_err(format_error)?,
        ElementKind::Table => super::table_svg::render(svg, element)?,
        ElementKind::Circle | ElementKind::Ellipse => {
            let radius_x = crate::Unit::from_raw(rect.size.width.raw() / 2);
            let radius_y = crate::Unit::from_raw(rect.size.height.raw() / 2);
            write!(
                svg,
                "<ellipse id=\"{}\" cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" {style}/>",
                escape(element.id.as_str()),
                points(rect.origin.x.checked_add(radius_x)?),
                points(rect.origin.y.checked_add(radius_y)?),
                points(radius_x),
                points(radius_y)
            )
            .map_err(format_error)?;
        }
        ElementKind::Line | ElementKind::Path | ElementKind::Polygon => {
            render_shape(svg, element, &style)?;
        }
        ElementKind::Text => render_text(svg, element)?,
        ElementKind::Image => render_image(svg, element, context)?,
        ElementKind::Chart | ElementKind::Qr | ElementKind::Barcode => {}
    }
    for _ in 0..groups {
        svg.push_str("</g>")?;
    }
    Ok(())
}

fn svg_transform(transform: crate::Transform) -> String {
    format!(
        "matrix({:.6} {:.6} {:.6} {:.6} {} {})",
        transform.a as f64 / 1_000_000.0,
        transform.b as f64 / 1_000_000.0,
        transform.c as f64 / 1_000_000.0,
        transform.d as f64 / 1_000_000.0,
        points(transform.tx),
        points(transform.ty),
    )
}

fn render_shape(
    svg: &mut dyn FormattedOutput,
    element: &ResolvedElement,
    style: &str,
) -> Result<()> {
    match &element.shape {
        Shape::Polygon { points: vertices } => {
            write!(
                svg,
                "<polygon id=\"{}\" points=\"",
                escape(element.id.as_str())
            )
            .map_err(format_error)?;
            for (index, point) in vertices.iter().enumerate() {
                if index > 0 {
                    svg.push_str(" ")?;
                }
                write!(svg, "{},{}", points(point.x), points(point.y)).map_err(format_error)?;
            }
            write!(svg, "\" {style}/>").map_err(format_error)?;
        }
        Shape::Path { commands, .. } => {
            write!(svg, "<path id=\"{}\" d=\"", escape(element.id.as_str()),)
                .map_err(format_error)?;
            write_path_data(svg, commands, None)?;
            write!(svg, "\" {style}/>").map_err(format_error)?;
        }
        Shape::Rect { .. } | Shape::Ellipse { .. } => {}
    }
    Ok(())
}

pub(super) fn write_path_data(
    output: &mut dyn FormattedOutput,
    commands: &[crate::PathCommand],
    origin: Option<crate::Point>,
) -> Result<()> {
    let origin = origin.unwrap_or(crate::Point {
        x: crate::Unit::ZERO,
        y: crate::Unit::ZERO,
    });
    for (index, command) in commands.iter().enumerate() {
        if index > 0 {
            output.push_str(" ")?;
        }
        match command {
            crate::PathCommand::Move { to } => write_coordinate(output, "M", *to, origin)?,
            crate::PathCommand::Line { to } => write_coordinate(output, "L", *to, origin)?,
            crate::PathCommand::Curve {
                control_1,
                control_2,
                to,
            } => {
                write_coordinate(output, "C", *control_1, origin)?;
                write_coordinate(output, "", *control_2, origin)?;
                write_coordinate(output, "", *to, origin)?;
            }
            crate::PathCommand::Close => output.push_str("Z")?,
        }
    }
    Ok(())
}

fn write_coordinate(
    output: &mut dyn FormattedOutput,
    command: &str,
    point: crate::Point,
    origin: crate::Point,
) -> Result<()> {
    if command.is_empty() {
        output.push_str(" ")?;
    } else {
        write!(output, "{command} ").map_err(format_error)?;
    }
    write!(
        output,
        "{} {}",
        points(point.x.checked_sub(origin.x)?),
        points(point.y.checked_sub(origin.y)?)
    )
    .map_err(format_error)
}

fn render_text(svg: &mut dyn FormattedOutput, element: &ResolvedElement) -> Result<()> {
    let rect = element.bounds.layout;
    let layout = element.text_layout.as_ref().ok_or_else(|| {
        FileMakerError::new(ErrorCode::FontMissing, "resolved text has no glyph layout")
    })?;
    let vertical = layout.writing_mode == crate::WritingMode::Vertical;
    let (mut line_x, mut line_y) = if vertical {
        (rect.origin.x.checked_add(rect.size.width)?, rect.origin.y)
    } else {
        (rect.origin.x, rect.origin.y.checked_add(layout.font_size)?)
    };
    write!(
        svg,
        "<text id=\"{}\" font-size=\"{}\" fill=\"{}\" opacity=\"{}\"{}>",
        escape(element.id.as_str()),
        points(layout.font_size),
        color(element.style.color),
        opacity(element.style.opacity),
        if vertical {
            " writing-mode=\"vertical-rl\""
        } else {
            ""
        },
    )
    .map_err(format_error)?;
    for line in &layout.lines {
        write!(
            svg,
            "<tspan x=\"{}\" y=\"{}\">",
            points(line_x),
            points(line_y),
        )
        .map_err(format_error)?;
        for run in &line.runs {
            write!(
                svg,
                "<tspan font-family=\"{}\" direction=\"{}\">{}</tspan>",
                escape(&run.font),
                if run.rtl { "rtl" } else { "ltr" },
                escape(&run.text),
            )
            .map_err(format_error)?;
        }
        svg.push_str("</tspan>")?;
        if vertical {
            line_x = line_x.checked_sub(line.height)?;
        } else {
            line_y = line_y.checked_add(line.height)?;
        }
    }
    svg.push_str("</text>")?;
    Ok(())
}

fn render_image(
    svg: &mut dyn FormattedOutput,
    element: &ResolvedElement,
    context: &ExportContext<'_>,
) -> Result<()> {
    let Some(name) = &element.asset else {
        return Ok(());
    };
    let Some(resolver) = context.assets else {
        return Ok(());
    };
    let asset = resolver.resolve_asset(name, context.limits.max_asset_bytes)?;
    let Some(placement) = element.image_placement else {
        return Ok(());
    };
    let (media_type, bytes) = normalized_image_bytes(&asset, placement.orientation)?;
    let clip = placement.clip;
    let destination = placement.destination;
    let source = placement.source;
    write!(
        svg,
        "<svg id=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" overflow=\"hidden\"><svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" preserveAspectRatio=\"none\"><image width=\"{}\" height=\"{}\" href=\"data:{};base64,",
        escape(element.id.as_str()),
        points(clip.origin.x),
        points(clip.origin.y),
        points(clip.size.width),
        points(clip.size.height),
        points(clip.size.width),
        points(clip.size.height),
        points(destination.origin.x.checked_sub(clip.origin.x)?),
        points(destination.origin.y.checked_sub(clip.origin.y)?),
        points(destination.size.width),
        points(destination.size.height),
        source.x,
        source.y,
        source.width,
        source.height,
        placement.intrinsic_width,
        placement.intrinsic_height,
        escape(&media_type),
    )
    .map_err(format_error)?;
    write_base64(svg, bytes.as_ref())?;
    svg.push_str("\"/></svg></svg>")?;
    Ok(())
}

fn svg_style(element: &ResolvedElement) -> String {
    format!(
        "fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\"",
        element.style.fill.map_or_else(|| "none".to_owned(), color),
        element
            .style
            .stroke
            .map_or_else(|| "none".to_owned(), color),
        points(element.style.stroke_width),
        opacity(element.style.opacity)
    )
}

fn contains_cmyk(element: &ResolvedElement) -> bool {
    [
        element.style.fill,
        element.style.stroke,
        Some(element.style.color),
    ]
    .into_iter()
    .flatten()
    .any(|color| matches!(color, Color::Cmyk { .. }))
}

fn format_error(_: std::fmt::Error) -> FileMakerError {
    output_limit_error()
}
