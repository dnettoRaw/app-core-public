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
use crate::{
    ErrorCode, ExportContext, FileMakerError, PdfMode, PdfStandardFont, ResolvedPage, Result, Unit,
};

pub(super) struct PdfFont {
    pub(super) resource: String,
    pub(super) base_name: String,
    pub(super) remapper: GlyphRemapper,
    pub(super) subset: Vec<u8>,
    pub(super) unicode: BTreeMap<u16, char>,
    pub(super) standard_font: Option<PdfStandardFont>,
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
            let glyphs = glyphs.into_iter().collect::<Vec<_>>();
            let remapper = GlyphRemapper::new_from_glyphs_sorted(&glyphs);
            if let Some(standard_font) = context.fonts.standard_face(&name) {
                let base_name = standard_font.metrics().pdf_base_name().to_owned();
                return Ok((
                    name.clone(),
                    PdfFont {
                        resource: format!("F{}", index + 1),
                        base_name,
                        remapper,
                        subset: Vec::new(),
                        unicode,
                        standard_font: Some(standard_font),
                        refs: None,
                    },
                ));
            }
            let asset = context.fonts.get(&name)?;
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
                    standard_font: None,
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
    if let Some(face) = font.standard_font {
        return write_standard_font(pdf, font, refs.type0, refs.cmap, face);
    }
    let cid_ref = refs
        .cid
        .ok_or_else(|| font_error("PDF CID font reference is missing"))?;
    let descriptor_ref = refs
        .descriptor
        .ok_or_else(|| font_error("PDF font descriptor reference is missing"))?;
    let stream_ref = refs
        .stream
        .ok_or_else(|| font_error("PDF font stream reference is missing"))?;
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
            .descendant_font(cid_ref)
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
    pdf.object(cid_ref, |chunk| {
        let mut cid = chunk.cid_font(cid_ref);
        cid.subtype(CidFontType::Type2)
            .base_font(base)
            .system_info(system)
            .font_descriptor(descriptor_ref)
            .cid_to_gid_map_predefined(Name(b"Identity"));
        cid.widths().consecutive(0, widths);
        cid.finish();
        Ok(())
    })?;

    write_subset_descriptor_and_stream(
        pdf,
        descriptor_ref,
        stream_ref,
        base,
        &metrics,
        &font.subset,
    )?;
    write_unicode_cmap(pdf, refs.cmap, font)
}

fn write_subset_descriptor_and_stream(
    pdf: &mut PdfDocument<'_>,
    descriptor_ref: pdf_writer::Ref,
    stream_ref: pdf_writer::Ref,
    base: Name<'_>,
    metrics: &skrifa::metrics::Metrics,
    subset: &[u8],
) -> Result<()> {
    let upem = f32::from(metrics.units_per_em);
    if upem == 0.0 {
        return Err(font_error("subsetted PDF font has no units-per-em"));
    }
    let bbox = metrics
        .bounds
        .ok_or_else(|| font_error("subsetted PDF font has no global bounds"))?;
    let cap_height = descriptor_cap_height(metrics);
    let scale = 1000.0 / upem;
    let flags = if metrics.italic_angle != 0.0 {
        FontFlags::NON_SYMBOLIC | FontFlags::ITALIC
    } else {
        FontFlags::NON_SYMBOLIC
    };
    pdf.object(descriptor_ref, |chunk| {
        chunk
            .font_descriptor(descriptor_ref)
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
            .font_file2(stream_ref);
        Ok(())
    })?;
    let subset_length = i32::try_from(subset.len())
        .map_err(|_| font_error("PDF font subset exceeds i32 length"))?;
    pdf.object(stream_ref, |chunk| {
        chunk
            .stream(stream_ref, subset)
            .pair(Name(b"Length1"), subset_length);
        Ok(())
    })
}

fn write_unicode_cmap(
    pdf: &mut PdfDocument<'_>,
    cmap_ref: pdf_writer::Ref,
    font: &PdfFont,
) -> Result<()> {
    let system = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    };
    let mut cmap = UnicodeCmap::<u16>::new(Name(b"FMUnicode"), system);
    for (old_gid, character) in &font.unicode {
        if let Some(new_gid) = font.remapper.get(*old_gid) {
            cmap.pair(new_gid, *character);
        }
    }
    let cmap = cmap.finish();
    pdf.object(cmap_ref, |chunk| {
        chunk.cmap(cmap_ref, cmap.as_slice());
        Ok(())
    })
}

fn write_standard_font(
    pdf: &mut PdfDocument<'_>,
    font: &PdfFont,
    font_ref: pdf_writer::Ref,
    cmap_ref: pdf_writer::Ref,
    face: PdfStandardFont,
) -> Result<()> {
    let metrics = face.metrics();
    let first = 32_u8;
    let last = u8::MAX;
    let widths = (first..=last).map(|code| metrics.winansi_width(code).unwrap_or(0.0));
    pdf.object(font_ref, |chunk| {
        chunk
            .type1_font(font_ref)
            .base_font(Name(font.base_name.as_bytes()))
            .first_char(first)
            .last_char(last)
            .widths(widths)
            .encoding_predefined(Name(b"WinAnsiEncoding"))
            .to_unicode(cmap_ref);
        Ok(())
    })?;
    let mut cmap = UnicodeCmap::<u8>::new(
        Name(b"FMStandardUnicode"),
        SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"UCS"),
            supplement: 0,
        },
    );
    for (code, character) in &font.unicode {
        cmap.pair(
            u8::try_from(*code).map_err(|_| font_error("WinAnsi code exceeds one byte"))?,
            *character,
        );
    }
    let cmap = cmap.finish();
    pdf.object(cmap_ref, |chunk| {
        chunk.cmap(cmap_ref, cmap.as_slice());
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
