// =============================================================================
//        #######
//     ###       ###     F: pdf.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded pdf contracts and behavior for this crate.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use pdf_writer::writers::{Catalog, DocumentInfo};
use pdf_writer::{Finish, Name, Rect as PdfRect, Ref, TextStr};

use super::core::record_text_capability_losses;
use super::core::selected_pages;
use super::pdf_font::{collect_fonts, write_fonts, PdfFont};
use super::pdf_image::{collect_images, write_images, PdfImage};
use super::pdf_paint::collect_opacities;
use super::pdf_render::render_page;
use super::pdf_stream::PdfDocument;
use super::progress::ExportProgress;
use crate::{
    ElementKind, ErrorCode, ExportCapabilities, ExportContext, ExportLossKind, ExportLossReport,
    ExportOutcome, ExportRequest, FileMakerError, PdfMode, ResolvedScene, Result,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct FontRefs {
    pub(super) type0: Ref,
    pub(super) cid: Ref,
    pub(super) descriptor: Ref,
    pub(super) stream: Ref,
    pub(super) cmap: Ref,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ImageRefs {
    pub(super) image: Ref,
    pub(super) mask: Option<Ref>,
}

#[derive(Default)]
struct RefAllocator(i32);

impl RefAllocator {
    fn next(&mut self) -> Result<Ref> {
        self.0 = self.0.checked_add(1).ok_or_else(|| {
            FileMakerError::new(ErrorCode::LimitExceeded, "PDF object reference overflow")
        })?;
        Ok(Ref::new(self.0))
    }
}

struct PreparedPdf {
    catalog_ref: Ref,
    info_ref: Ref,
    page_tree_ref: Ref,
    page_refs: Vec<(Ref, Ref)>,
    fonts: BTreeMap<String, PdfFont>,
    images: BTreeMap<String, PdfImage>,
    opacity_refs: BTreeMap<u32, Ref>,
}

pub(super) fn export(
    scene: &ResolvedScene,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    progress: &mut ExportProgress<'_>,
    writer: &mut dyn Write,
) -> Result<ExportOutcome> {
    let pages = selected_pages(scene, request)?;
    let mut losses = ExportLossReport::default();
    let mut prepared = prepare_pdf(&pages, request, context, &mut losses)?;
    progress.checkpoint()?;
    losses.enforce(request.fidelity)?;
    let build = PdfBuild {
        pages: &pages,
        title: &scene.template_id,
        request,
        context,
    };
    let mut counter = std::io::sink();
    let expected = build_pdf(
        &build,
        &mut prepared,
        Some(&mut *progress),
        false,
        &mut counter,
    )?;
    progress.checkpoint()?;
    let bytes_written = build_pdf(&build, &mut prepared, None, true, writer)?;
    if bytes_written != expected {
        return Err(FileMakerError::new(
            ErrorCode::Validation,
            "PDF output changed between sizing and streaming",
        ));
    }
    let mut capabilities = BTreeSet::from([
        ExportCapabilities::MultiPage,
        ExportCapabilities::Vector,
        ExportCapabilities::Transparency,
        ExportCapabilities::Cmyk,
        ExportCapabilities::Images,
        ExportCapabilities::Metadata,
    ]);
    if request.pdf_mode != PdfMode::Flattened {
        capabilities.insert(ExportCapabilities::EditableText);
        capabilities.insert(ExportCapabilities::EmbeddedFonts);
    }
    Ok(ExportOutcome {
        bytes_written,
        loss_report: losses,
        capabilities,
    })
}

fn prepare_pdf(
    pages: &[&crate::ResolvedPage],
    request: &ExportRequest,
    context: &ExportContext<'_>,
    losses: &mut ExportLossReport,
) -> Result<PreparedPdf> {
    let mut fonts = collect_fonts(pages, request.pdf_mode, context)?;
    let mut images = collect_images(pages, context, losses)?;
    for element in pages.iter().flat_map(|page| &page.elements) {
        record_text_capability_losses(element, losses);
        if matches!(
            element.kind,
            ElementKind::Chart | ElementKind::Qr | ElementKind::Barcode
        ) {
            losses.push(
                ExportLossKind::UnsupportedElement,
                Some(element.id.as_str()),
                "prepared element kind has no PDF renderer",
            );
        }
    }
    let mut refs = RefAllocator::default();
    let catalog_ref = refs.next()?;
    let info_ref = refs.next()?;
    let page_tree_ref = refs.next()?;
    let page_refs = pages
        .iter()
        .map(|_| Ok((refs.next()?, refs.next()?)))
        .collect::<Result<Vec<_>>>()?;
    for font in fonts.values_mut() {
        font.refs = Some(FontRefs {
            type0: refs.next()?,
            cid: refs.next()?,
            descriptor: refs.next()?,
            stream: refs.next()?,
            cmap: refs.next()?,
        });
    }
    for image in images.values_mut() {
        image.refs = Some(ImageRefs {
            image: refs.next()?,
            mask: image.has_alpha.then(|| refs.next()).transpose()?,
        });
    }
    let opacities = collect_opacities(pages);
    let opacity_refs = opacities
        .iter()
        .map(|opacity| Ok((*opacity, refs.next()?)))
        .collect::<Result<BTreeMap<_, _>>>()?;

    Ok(PreparedPdf {
        catalog_ref,
        info_ref,
        page_tree_ref,
        page_refs,
        fonts,
        images,
        opacity_refs,
    })
}

struct PdfBuild<'a> {
    pages: &'a [&'a crate::ResolvedPage],
    title: &'a str,
    request: &'a ExportRequest,
    context: &'a ExportContext<'a>,
}

fn build_pdf(
    build: &PdfBuild<'_>,
    prepared: &mut PreparedPdf,
    progress: Option<&mut ExportProgress<'_>>,
    release_intermediates: bool,
    writer: &mut dyn Write,
) -> Result<usize> {
    let mut pdf = PdfDocument::new(
        writer,
        build.context.limits.max_output_bytes,
        prepared.catalog_ref,
        prepared.info_ref,
    )?;
    pdf.object(prepared.catalog_ref, |chunk| {
        let mut catalog: Catalog<'_> = chunk.indirect(prepared.catalog_ref).start();
        catalog.pages(prepared.page_tree_ref);
        Ok(())
    })?;
    pdf.object(prepared.info_ref, |chunk| {
        let mut info: DocumentInfo<'_> = chunk.indirect(prepared.info_ref).start();
        info.title(TextStr(build.title))
            .creator(TextStr("AppCore FileMaker"))
            .producer(TextStr(concat!(
                "appcore-filemaker ",
                env!("CARGO_PKG_VERSION")
            )));
        Ok(())
    })?;
    let page_count = i32::try_from(build.pages.len()).map_err(|_| {
        FileMakerError::new(
            ErrorCode::LimitExceeded,
            "PDF page count exceeds format range",
        )
    })?;
    pdf.object(prepared.page_tree_ref, |chunk| {
        chunk
            .pages(prepared.page_tree_ref)
            .kids(prepared.page_refs.iter().map(|(page, _)| *page))
            .count(page_count);
        Ok(())
    })?;
    write_fonts(&mut pdf, &mut prepared.fonts, release_intermediates)?;
    write_images(&mut pdf, &prepared.images)?;
    for (opacity, reference) in &prepared.opacity_refs {
        let alpha = *opacity as f32 / 1_000_000.0;
        pdf.object(*reference, |chunk| {
            chunk
                .ext_graphics(*reference)
                .stroking_alpha(alpha)
                .non_stroking_alpha(alpha);
            Ok(())
        })?;
    }
    let mut progress = progress;
    for (page, (page_ref, content_ref)) in build.pages.iter().zip(prepared.page_refs.iter()) {
        let content = render_page(
            page,
            build.request.pdf_mode,
            build.context,
            &prepared.fonts,
            &prepared.images,
            &prepared.opacity_refs,
            progress.as_deref_mut(),
        )?;
        pdf.object(*page_ref, |chunk| {
            let mut page_writer = chunk.page(*page_ref);
            page_writer
                .media_box(PdfRect::new(
                    0.0,
                    0.0,
                    page.size.width.as_points_f64() as f32,
                    page.size.height.as_points_f64() as f32,
                ))
                .parent(prepared.page_tree_ref)
                .contents(*content_ref);
            let mut resources = page_writer.resources();
            if build.request.pdf_mode != PdfMode::Flattened && !prepared.fonts.is_empty() {
                let mut resource_fonts = resources.fonts();
                for font in prepared.fonts.values() {
                    resource_fonts.pair(Name(font.resource.as_bytes()), font.refs()?.type0);
                }
                resource_fonts.finish();
            }
            if !prepared.images.is_empty() {
                let mut x_objects = resources.x_objects();
                for image in prepared.images.values() {
                    x_objects.pair(Name(image.resource.as_bytes()), image.refs()?.image);
                }
                x_objects.finish();
            }
            if !prepared.opacity_refs.is_empty() {
                let mut states = resources.ext_g_states();
                for (opacity, reference) in &prepared.opacity_refs {
                    states.pair(Name(opacity_name(*opacity).as_bytes()), *reference);
                }
                states.finish();
            }
            resources.finish();
            page_writer.finish();
            Ok(())
        })?;
        pdf.object(*content_ref, |chunk| {
            chunk.stream(*content_ref, &content);
            Ok(())
        })?;
    }
    pdf.finish()
}

pub(super) fn opacity_name(opacity: u32) -> String {
    format!("GS{opacity}")
}

impl PdfFont {
    pub(super) fn refs(&self) -> Result<FontRefs> {
        self.refs.ok_or_else(|| {
            FileMakerError::new(ErrorCode::ExportWrite, "PDF font references are missing")
        })
    }
}

impl PdfImage {
    pub(super) fn refs(&self) -> Result<ImageRefs> {
        self.refs.ok_or_else(|| {
            FileMakerError::new(ErrorCode::ExportWrite, "PDF image references are missing")
        })
    }
}
