// =============================================================================
//        #######
//     ###       ###     F: pdf_font.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded pdf font contracts and behavior for this crate.

use std::collections::{BTreeMap, BTreeSet};

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Finish, Name, Rect as PdfRect, Str};
use skrifa::{
    instance::{LocationRef, Size},
    FontRef, GlyphId, MetadataProvider,
};
use subsetter::GlyphRemapper;

use super::pdf::FontRefs;
use super::pdf_stream::PdfDocument;
use crate::{ErrorCode, ExportContext, FileMakerError, PdfMode, ResolvedPage, Result, Unit};

pub(super) struct PdfFont {
    pub(super) resource: String,
    pub(super) base_name: String,
    pub(super) remapper: GlyphRemapper,
    pub(super) subset: Vec<u8>,
    pub(super) unicode: BTreeMap<u16, char>,
    pub(super) refs: Option<FontRefs>,
}

pub(super) fn collect_fonts(
    pages: &[&ResolvedPage],
    mode: PdfMode,
    context: &ExportContext<'_>,
) -> Result<BTreeMap<String, PdfFont>> {
    if mode == PdfMode::Flattened {
        return Ok(BTreeMap::new());
    }
    let mut usage: BTreeMap<String, (BTreeSet<u16>, BTreeMap<u16, char>)> = BTreeMap::new();
    for element in pages.iter().flat_map(|page| &page.elements) {
        for line in super::core::text_layouts(element)
            .into_iter()
            .flat_map(|layout| &layout.lines)
        {
            for run in &line.runs {
                let entry = usage.entry(run.font.clone()).or_default();
                for glyph in &run.glyphs {
                    entry.0.insert(glyph.id);
                    if let Some(character) = character_for_cluster(&run.text, glyph.cluster) {
                        entry.1.entry(glyph.id).or_insert(character);
                    }
                }
            }
        }
    }
    usage
        .into_iter()
        .enumerate()
        .map(|(index, (name, (glyphs, unicode)))| {
            let asset = context.fonts.get(&name)?;
            let glyphs = glyphs.into_iter().collect::<Vec<_>>();
            let remapper = GlyphRemapper::new_from_glyphs_sorted(&glyphs);
            let subset = subsetter::subset(&asset.bytes, asset.face_index, &remapper)
                .map_err(|error| font_error(format!("cannot subset `{name}`: {error}")))?;
            if subset.len() > context.limits.max_asset_bytes {
                return Err(FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    "subsetted PDF font exceeds configured asset limit",
                ));
            }
            Ok((
                name.clone(),
                PdfFont {
                    resource: format!("F{}", index + 1),
                    base_name: format!("FMSubset{}", index + 1),
                    remapper,
                    subset,
                    unicode,
                    refs: None,
                },
            ))
        })
        .collect()
}

pub(super) fn write_fonts(
    pdf: &mut PdfDocument<'_>,
    fonts: &mut BTreeMap<String, PdfFont>,
    release_subsets: bool,
) -> Result<()> {
    for font in fonts.values_mut() {
        write_font(pdf, font)?;
        if release_subsets {
            font.subset.clear();
            font.subset.shrink_to_fit();
        }
    }
    Ok(())
}

fn write_font(pdf: &mut PdfDocument<'_>, font: &PdfFont) -> Result<()> {
    let refs = font.refs()?;
    let face = FontRef::from_index(&font.subset, 0)
        .map_err(|_| font_error("subsetted PDF font is invalid"))?;
    let system = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    };
    let base = Name(font.base_name.as_bytes());
    pdf.object(refs.type0, |chunk| {
        chunk
            .type0_font(refs.type0)
            .base_font(base)
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(refs.cid)
            .to_unicode(refs.cmap);
        Ok(())
    })?;
    let metrics = face.metrics(Size::unscaled(), LocationRef::default());
    let upem = f32::from(metrics.units_per_em);
    if upem == 0.0 {
        return Err(font_error("subsetted PDF font has no units-per-em"));
    }
    let glyph_metrics = face.glyph_metrics(Size::unscaled(), LocationRef::default());
    let widths = (0..font.remapper.num_gids())
        .map(|gid| {
            glyph_metrics
                .advance_width(GlyphId::new(u32::from(gid)))
                .map(|width| width * 1000.0 / upem)
                .ok_or_else(|| font_error("subsetted PDF glyph has no advance width"))
        })
        .collect::<Result<Vec<_>>>()?;
    pdf.object(refs.cid, |chunk| {
        let mut cid = chunk.cid_font(refs.cid);
        cid.subtype(CidFontType::Type2)
            .base_font(base)
            .system_info(system)
            .font_descriptor(refs.descriptor)
            .cid_to_gid_map_predefined(Name(b"Identity"));
        cid.widths().consecutive(0, widths);
        cid.finish();
        Ok(())
    })?;

    let bbox = metrics
        .bounds
        .ok_or_else(|| font_error("subsetted PDF font has no global bounds"))?;
    let cap_height = descriptor_cap_height(&metrics);
    let scale = 1000.0 / upem;
    let flags = if metrics.italic_angle != 0.0 {
        FontFlags::NON_SYMBOLIC | FontFlags::ITALIC
    } else {
        FontFlags::NON_SYMBOLIC
    };
    pdf.object(refs.descriptor, |chunk| {
        chunk
            .font_descriptor(refs.descriptor)
            .name(base)
            .flags(flags)
            .bbox(PdfRect::new(
                bbox.x_min * scale,
                bbox.y_min * scale,
                bbox.x_max * scale,
                bbox.y_max * scale,
            ))
            .italic_angle(0.0)
            .ascent(metrics.ascent * scale)
            .descent(metrics.descent * scale)
            .cap_height(cap_height * scale)
            .stem_v(80.0)
            .font_file2(refs.stream);
        Ok(())
    })?;
    let subset_length = i32::try_from(font.subset.len())
        .map_err(|_| font_error("PDF font subset exceeds i32 length"))?;
    pdf.object(refs.stream, |chunk| {
        chunk
            .stream(refs.stream, &font.subset)
            .pair(Name(b"Length1"), subset_length);
        Ok(())
    })?;

    let mut cmap = UnicodeCmap::<u16>::new(Name(b"FMUnicode"), system);
    for (old_gid, character) in &font.unicode {
        if let Some(new_gid) = font.remapper.get(*old_gid) {
            cmap.pair(new_gid, *character);
        }
    }
    let cmap = cmap.finish();
    pdf.object(refs.cmap, |chunk| {
        chunk.cmap(refs.cmap, cmap.as_slice());
        Ok(())
    })
}

// PDF requires CapHeight, while valid fonts may omit the OS/2 value. Ascent is
// the deterministic PDF descriptor policy in that explicit absence case.
fn descriptor_cap_height(metrics: &skrifa::metrics::Metrics) -> f32 {
    metrics.cap_height.unwrap_or(metrics.ascent)
}

fn character_for_cluster(text: &str, cluster: u32) -> Option<char> {
    usize::try_from(cluster)
        .ok()
        .and_then(|offset| text.get(offset..))
        .and_then(|value| value.chars().next())
}

pub(super) fn unit(value: Unit) -> f32 {
    value.as_points_f64() as f32
}

fn font_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::ExportUnsupported, message)
}
