// =============================================================================
//        #######
//     ###       ###     F: html.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded html contracts and behavior for this crate.

use std::collections::BTreeSet;
use std::io::Write;

use super::bounded_string::{output_limit_error, CountingOutput, FormattedOutput, StreamingOutput};
use super::core::{record_text_capability_losses, selected_pages, text_fonts};
use super::markup::{color, escape, normalized_image_bytes, opacity, points, write_base64};
use super::progress::ExportProgress;
use super::svg::write_path_data;
use crate::{
    Color, ElementKind, ExportCapabilities, ExportContext, ExportLossKind, ExportLossReport,
    ExportOutcome, ExportRequest, HtmlMode, ResolvedElement, ResolvedScene, Result,
};

pub(super) fn export(
    scene: &ResolvedScene,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    progress: &mut ExportProgress<'_>,
    writer: &mut dyn Write,
) -> Result<ExportOutcome> {
    let pages = selected_pages(scene, request)?;
    let mut losses = ExportLossReport::default();
    for element in pages.iter().flat_map(|page| &page.elements) {
        analyze_element(element, context, &mut losses);
    }
    losses.enforce(request.fidelity)?;
    let mut counter = CountingOutput::new(context.limits.max_output_bytes);
    write_html(
        &mut counter,
        &pages,
        request.html_mode,
        context,
        Some(progress),
        true,
    )?;
    let expected = counter.finish()?;
    progress.checkpoint()?;
    let mut html = StreamingOutput::new(writer, context.limits.max_output_bytes);
    write_html(&mut html, &pages, request.html_mode, context, None, false)?;
    let bytes_written = html.finish()?;
    if bytes_written != expected {
        return Err(crate::FileMakerError::new(
            crate::ErrorCode::Validation,
            "HTML resolver output changed between sizing and streaming",
        ));
    }
    let mut capabilities = BTreeSet::from([
        ExportCapabilities::MultiPage,
        ExportCapabilities::EditableText,
        ExportCapabilities::EmbeddedFonts,
        ExportCapabilities::Images,
        ExportCapabilities::Transparency,
    ]);
    if request.html_mode == HtmlMode::Semantic {
        capabilities.insert(ExportCapabilities::Semantic);
    }
    Ok(ExportOutcome {
        bytes_written,
        loss_report: losses,
        capabilities,
    })
}

fn write_html(
    html: &mut dyn FormattedOutput,
    pages: &[&crate::ResolvedPage],
    mode: HtmlMode,
    context: &ExportContext<'_>,
    mut progress: Option<&mut ExportProgress<'_>>,
    advance_progress: bool,
) -> Result<()> {
    html.push_str("<!doctype html><html><head><meta charset=\"utf-8\"><style>")?;
    html.push_str("*{box-sizing:border-box}.fm-page{position:relative;overflow:hidden}.fm-fixed{position:absolute}")?;
    embed_fonts(html, pages, context)?;
    html.push_str("</style></head><body>")?;
    for page in pages {
        write!(
            html,
            "<article class=\"fm-page\" data-page=\"{}\" style=\"width:{}pt;height:{}pt\">",
            page.index,
            points(page.size.width),
            points(page.size.height)
        )
        .map_err(html_error)?;
        for element in &page.elements {
            render_element(html, element, mode, context)?;
            if let Some(progress) = progress.as_deref_mut() {
                if advance_progress {
                    progress.element()?;
                } else {
                    progress.checkpoint()?;
                }
            }
        }
        html.push_str("</article>")?;
    }
    html.push_str("</body></html>")?;
    Ok(())
}

fn embed_fonts(
    html: &mut dyn FormattedOutput,
    pages: &[&crate::ResolvedPage],
    context: &ExportContext<'_>,
) -> Result<()> {
    let fonts: BTreeSet<&str> = pages
        .iter()
        .flat_map(|page| &page.elements)
        .flat_map(|element| text_fonts(element))
        .collect();
    for name in fonts {
        let font = context.fonts.get(name)?;
        write!(
            html,
            "@font-face{{font-family:'{}';src:url(data:font/ttf;base64,",
            escape(name)
        )
        .map_err(html_error)?;
        write_base64(html, &font.bytes)?;
        html.push_str(")}")?;
    }
    Ok(())
}

fn analyze_element(
    element: &ResolvedElement,
    context: &ExportContext<'_>,
    losses: &mut ExportLossReport,
) {
    if [
        element.style.fill,
        element.style.stroke,
        Some(element.style.color),
    ]
    .into_iter()
    .flatten()
    .any(|value| matches!(value, Color::Cmyk { .. }))
    {
        losses.push(
            ExportLossKind::CmykConvertedToRgb,
            Some(element.id.as_str()),
            "HTML/CSS has no portable CMYK paint",
        );
    }
    record_text_capability_losses(element, losses);
    match element.kind {
        ElementKind::Table => super::table_html::record_losses(element, losses),
        ElementKind::Image
            if element.asset.is_none()
                || context.assets.is_none()
                || element.image_placement.is_none() =>
        {
            losses.push(
                ExportLossKind::ImageOmitted,
                Some(element.id.as_str()),
                "image asset/resolver is missing",
            );
        }
        ElementKind::Chart | ElementKind::Qr | ElementKind::Barcode => losses.push(
            ExportLossKind::UnsupportedElement,
            Some(element.id.as_str()),
            "prepared element kind has no HTML renderer",
        ),
        _ => {}
    }
}

fn render_element(
    html: &mut dyn FormattedOutput,
    element: &ResolvedElement,
    mode: HtmlMode,
    context: &ExportContext<'_>,
) -> Result<()> {
    let fixed = mode == HtmlMode::Fixed;
    let (attributes, geometry_style) = html_geometry(element, fixed)?;
    match element.kind {
        ElementKind::Text => render_text(html, element, mode, &attributes, &geometry_style)?,
        ElementKind::Table => {
            super::table_html::render(html, element, mode, &attributes, &geometry_style)?
        }
        ElementKind::Image => render_image(html, element, &attributes, &geometry_style, context)?,
        ElementKind::Chart | ElementKind::Qr | ElementKind::Barcode => {}
        ElementKind::Line | ElementKind::Path => {
            render_path(html, element, &attributes, &geometry_style)?
        }
        ElementKind::Polygon => render_polygon(html, element, &attributes, &geometry_style)?,
        _ => render_rect(html, element, &attributes, &geometry_style)?,
    }
    Ok(())
}

fn html_geometry(element: &ResolvedElement, fixed: bool) -> Result<(String, String)> {
    let rect = element.bounds.layout;
    let transformed_origin = element.transform.apply(rect.origin)?;
    let local_x = transformed_origin.x.checked_sub(rect.origin.x)?;
    let local_y = transformed_origin.y.checked_sub(rect.origin.y)?;
    let transform = if element.transform.is_identity() {
        String::new()
    } else {
        format!(
            "transform-origin:0 0;transform:matrix({:.6},{:.6},{:.6},{:.6},{},{});",
            element.transform.a as f64 / 1_000_000.0,
            element.transform.b as f64 / 1_000_000.0,
            element.transform.c as f64 / 1_000_000.0,
            element.transform.d as f64 / 1_000_000.0,
            css_pixels(local_x),
            css_pixels(local_y),
        )
    };
    let clipping = if element.bounds.clip.is_some() {
        "overflow:hidden;"
    } else {
        ""
    };
    if fixed {
        Ok((
            "class=\"fm-fixed\"".to_owned(),
            format!(
                "left:{}pt;top:{}pt;width:{}pt;height:{}pt;opacity:{};{transform}{clipping}",
                points(rect.origin.x),
                points(rect.origin.y),
                points(rect.size.width),
                points(rect.size.height),
                opacity(element.style.opacity),
            ),
        ))
    } else {
        let semantic_box = if element.bounds.clip.is_some() {
            format!(
                "width:{}pt;height:{}pt;",
                points(rect.size.width),
                points(rect.size.height),
            )
        } else {
            String::new()
        };
        Ok((
            String::new(),
            format!("{transform}{clipping}{semantic_box}"),
        ))
    }
}

fn render_text(
    html: &mut dyn FormattedOutput,
    element: &ResolvedElement,
    mode: HtmlMode,
    attributes: &str,
    geometry_style: &str,
) -> Result<()> {
    let tag = if mode == HtmlMode::Semantic {
        "p"
    } else {
        "div"
    };
    let layout = element.text_layout.as_ref().ok_or_else(|| {
        crate::FileMakerError::new(
            crate::ErrorCode::FontMissing,
            "resolved HTML text has no glyph layout",
        )
    })?;
    let writing_mode = if layout.writing_mode == crate::WritingMode::Vertical {
        "writing-mode:vertical-rl;"
    } else {
        ""
    };
    write!(
        html,
        "<{tag} id=\"{}\" {attributes} style=\"{geometry_style}{writing_mode}color:{};font-size:{}pt;white-space:pre\">",
        escape(element.id.as_str()),
        color(element.style.color),
        points(layout.font_size),
    )
    .map_err(html_error)?;
    for (line_index, line) in layout.lines.iter().enumerate() {
        if line_index > 0 {
            html.push_str("<br>")?;
        }
        for run in &line.runs {
            write!(
                html,
                "<span style=\"font-family:'{}';direction:{}\">{}</span>",
                escape(&run.font),
                if run.rtl { "rtl" } else { "ltr" },
                escape(&run.text),
            )
            .map_err(html_error)?;
        }
    }
    write!(html, "</{tag}>").map_err(html_error)
}

fn render_path(
    html: &mut dyn FormattedOutput,
    element: &ResolvedElement,
    attributes: &str,
    geometry_style: &str,
) -> Result<()> {
    let rect = element.bounds.layout;
    let crate::Shape::Path { commands, .. } = &element.shape else {
        return Err(html_error(std::fmt::Error));
    };
    write!(
        html,
        "<svg aria-hidden=\"true\" {attributes} style=\"{geometry_style}\" viewBox=\"0 0 {} {}\"><path d=\"",
        points(rect.size.width),
        points(rect.size.height),
    )
    .map_err(html_error)?;
    write_path_data(html, commands, Some(rect.origin))?;
    write!(
        html,
        "\" fill=\"{}\" stroke=\"{}\"/></svg>",
        element.style.fill.map_or_else(|| "none".to_owned(), color),
        element
            .style
            .stroke
            .map_or_else(|| "none".to_owned(), color)
    )
    .map_err(html_error)
}

fn render_polygon(
    html: &mut dyn FormattedOutput,
    element: &ResolvedElement,
    attributes: &str,
    geometry_style: &str,
) -> Result<()> {
    let rect = element.bounds.layout;
    let crate::Shape::Polygon { points: vertices } = &element.shape else {
        return Err(html_error(std::fmt::Error));
    };
    write!(
        html,
        "<svg aria-hidden=\"true\" {attributes} style=\"{geometry_style}\" viewBox=\"0 0 {} {}\"><polygon points=\"",
        points(rect.size.width),
        points(rect.size.height),
    )
    .map_err(html_error)?;
    for (index, point) in vertices.iter().enumerate() {
        if index > 0 {
            html.push_str(" ")?;
        }
        write!(
            html,
            "{},{}",
            points(point.x.checked_sub(rect.origin.x)?),
            points(point.y.checked_sub(rect.origin.y)?)
        )
        .map_err(html_error)?;
    }
    write!(
        html,
        "\" fill=\"{}\" stroke=\"{}\"/></svg>",
        element.style.fill.map_or_else(|| "none".to_owned(), color),
        element
            .style
            .stroke
            .map_or_else(|| "none".to_owned(), color)
    )
    .map_err(html_error)
}

fn render_rect(
    html: &mut dyn FormattedOutput,
    element: &ResolvedElement,
    attributes: &str,
    geometry_style: &str,
) -> Result<()> {
    let rect = element.bounds.layout;
    write!(
        html,
        "<svg aria-hidden=\"true\" {attributes} style=\"{geometry_style}\" viewBox=\"0 0 {} {}\"><rect width=\"100%\" height=\"100%\" fill=\"{}\" stroke=\"{}\"/></svg>",
        points(rect.size.width),
        points(rect.size.height),
        element.style.fill.map_or_else(|| "none".to_owned(), color),
        element.style.stroke.map_or_else(|| "none".to_owned(), color)
    )
    .map_err(html_error)
}

fn render_image(
    html: &mut dyn FormattedOutput,
    element: &ResolvedElement,
    attributes: &str,
    geometry_style: &str,
    context: &ExportContext<'_>,
) -> Result<()> {
    let (Some(name), Some(resolver)) = (&element.asset, context.assets) else {
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
        html,
        "<svg id=\"{}\" {attributes} style=\"{geometry_style}\" viewBox=\"0 0 {} {}\" overflow=\"hidden\" aria-hidden=\"true\"><svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" preserveAspectRatio=\"none\"><image width=\"{}\" height=\"{}\" href=\"data:{};base64,",
        escape(element.id.as_str()),
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
    .map_err(html_error)?;
    write_base64(html, bytes.as_ref())?;
    html.push_str("\"/></svg></svg>")?;
    Ok(())
}

fn html_error(_: std::fmt::Error) -> crate::FileMakerError {
    output_limit_error()
}

fn css_pixels(value: crate::Unit) -> String {
    format!("{:.6}", value.as_points_f64() * 4.0 / 3.0)
}
