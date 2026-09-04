// =============================================================================
//        #######
//     ###       ###     F: pdf_paint.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 20:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 20:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Applies resolved colors and opacity resources without changing geometry.

use std::collections::{BTreeMap, BTreeSet};

use pdf_writer::{Content, Name};

use super::pdf::opacity_name;
use crate::{Color, ResolvedElement, ResolvedPage};

pub(super) fn collect_opacities(pages: &[&ResolvedPage]) -> BTreeSet<u32> {
    pages
        .iter()
        .flat_map(|page| &page.elements)
        .flat_map(element_opacities)
        .filter(|opacity| *opacity < 1_000_000)
        .collect()
}

fn element_opacities(element: &ResolvedElement) -> Vec<u32> {
    let mut values: Vec<_> = [
        element.style.fill,
        element.style.stroke,
        Some(element.style.color),
    ]
    .into_iter()
    .flatten()
    .map(|color| effective_opacity(element.style.opacity, color))
    .collect();
    if let Some(table) = &element.table {
        for cell in table
            .header
            .iter()
            .chain(table.rows.iter().flat_map(|row| &row.cells))
            .chain(&table.totals)
        {
            values.extend(
                [cell.style.fill, cell.style.stroke, Some(cell.style.color)]
                    .into_iter()
                    .flatten()
                    .map(|color| effective_opacity(cell.style.opacity, color)),
            );
        }
    }
    values
}

pub(super) fn effective_opacity(opacity: u32, color: Color) -> u32 {
    let alpha = match color {
        Color::Rgba { a, .. } => u32::from(a) * 1_000_000 / 255,
        _ => 1_000_000,
    };
    (u64::from(opacity) * u64::from(alpha) / 1_000_000).min(1_000_000) as u32
}

pub(super) fn apply_opacity(
    content: &mut Content,
    opacity: u32,
    available: &BTreeMap<u32, pdf_writer::Ref>,
) {
    if available.contains_key(&opacity) {
        content.set_parameters(Name(opacity_name(opacity).as_bytes()));
    }
}

pub(super) fn set_fill(content: &mut Content, color: Color) {
    match color {
        Color::Rgb { r, g, b } | Color::Rgba { r, g, b, .. } => {
            content.set_fill_rgb(channel(r), channel(g), channel(b))
        }
        Color::Gray { value } => content.set_fill_gray(channel(value)),
        Color::Cmyk { c, m, y, k } => content.set_fill_cmyk(
            c as f32 / 1_000_000.0,
            m as f32 / 1_000_000.0,
            y as f32 / 1_000_000.0,
            k as f32 / 1_000_000.0,
        ),
    };
}

pub(super) fn set_stroke(content: &mut Content, color: Color) {
    match color {
        Color::Rgb { r, g, b } | Color::Rgba { r, g, b, .. } => {
            content.set_stroke_rgb(channel(r), channel(g), channel(b))
        }
        Color::Gray { value } => content.set_stroke_gray(channel(value)),
        Color::Cmyk { c, m, y, k } => content.set_stroke_cmyk(
            c as f32 / 1_000_000.0,
            m as f32 / 1_000_000.0,
            y as f32 / 1_000_000.0,
            k as f32 / 1_000_000.0,
        ),
    };
}

fn channel(value: u8) -> f32 {
    f32::from(value) / 255.0
}
