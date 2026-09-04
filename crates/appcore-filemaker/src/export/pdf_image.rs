// =============================================================================
//        #######
//     ###       ###     F: pdf_image.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded pdf image contracts and behavior for this crate.

use std::collections::BTreeMap;

use super::pdf::ImageRefs;
use super::pdf_stream::PdfDocument;
use crate::{
    ElementKind, ErrorCode, ExportContext, ExportLossKind, ExportLossReport, FileMakerError,
    ResolvedPage, Result,
};

pub(super) struct PdfImage {
    pub(super) resource: String,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) has_alpha: bool,
    pub(super) refs: Option<ImageRefs>,
    asset: crate::Asset,
    placement: crate::ImagePlacement,
}

pub(super) fn collect_images(
    pages: &[&ResolvedPage],
    context: &ExportContext<'_>,
    losses: &mut ExportLossReport,
) -> Result<BTreeMap<String, PdfImage>> {
    let mut images = BTreeMap::new();
    for element in pages
        .iter()
        .flat_map(|page| &page.elements)
        .filter(|element| element.kind == ElementKind::Image)
    {
        let Some(name) = &element.asset else {
            losses.push(
                ExportLossKind::ImageOmitted,
                Some(element.id.as_str()),
                "image has no asset reference",
            );
            continue;
        };
        let Some(placement) = element.image_placement else {
            losses.push(
                ExportLossKind::ImageOmitted,
                Some(element.id.as_str()),
                "image geometry was not resolved during layout",
            );
            continue;
        };
        if placement.vector {
            losses.push(
                ExportLossKind::UnsupportedElement,
                Some(element.id.as_str()),
                "PDF exporter does not rasterize SVG assets",
            );
            continue;
        }
        let Some(resolver) = context.assets else {
            losses.push(
                ExportLossKind::ImageOmitted,
                Some(element.id.as_str()),
                "PDF image export requires an explicit asset resolver",
            );
            continue;
        };
        let asset = resolver.resolve_asset(name, context.limits.max_asset_bytes)?;
        let mut decoded = image::load_from_memory(&asset.bytes)
            .map_err(|error| image_error(format!("cannot decode `{name}`: {error}")))?;
        placement.orientation.apply(&mut decoded);
        let decoded = decoded.crop_imm(
            placement.source.x,
            placement.source.y,
            placement.source.width,
            placement.source.height,
        );
        let pixels = u64::from(decoded.width()) * u64::from(decoded.height());
        if pixels > context.limits.max_pixels {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "PDF image pixel count exceeds configured limit",
            ));
        }
        let width = i32::try_from(decoded.width()).map_err(|_| {
            FileMakerError::new(ErrorCode::LimitExceeded, "PDF image width exceeds i32")
        })?;
        let height = i32::try_from(decoded.height()).map_err(|_| {
            FileMakerError::new(ErrorCode::LimitExceeded, "PDF image height exceeds i32")
        })?;
        let has_alpha = decoded.color().has_alpha();
        let index = images.len() + 1;
        images.insert(
            element.id.as_str().to_owned(),
            PdfImage {
                resource: format!("Im{index}"),
                width,
                height,
                has_alpha,
                refs: None,
                asset,
                placement,
            },
        );
    }
    Ok(images)
}

pub(super) fn write_images(
    pdf: &mut PdfDocument<'_>,
    images: &BTreeMap<String, PdfImage>,
) -> Result<()> {
    for image in images.values() {
        let refs = image.refs()?;
        let (rgb, alpha) = decode_image_pixels(image)?;
        pdf.object(refs.image, |chunk| {
            let mut object = chunk.image_xobject(refs.image, &rgb);
            object.width(image.width).height(image.height);
            object.color_space().device_rgb();
            object.bits_per_component(8);
            if let Some(mask_ref) = refs.mask {
                object.s_mask(mask_ref);
            }
            Ok(())
        })?;
        if let (Some(alpha), Some(mask_ref)) = (alpha.as_deref(), refs.mask) {
            pdf.object(mask_ref, |chunk| {
                let mut mask = chunk.image_xobject(mask_ref, alpha);
                mask.width(image.width).height(image.height);
                mask.color_space().device_gray();
                mask.bits_per_component(8);
                Ok(())
            })?;
        }
    }
    Ok(())
}

fn decode_image_pixels(image: &PdfImage) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
    let mut decoded = image::load_from_memory(&image.asset.bytes)
        .map_err(|error| image_error(format!("cannot decode `{}`: {error}", image.asset.name)))?;
    image.placement.orientation.apply(&mut decoded);
    let decoded = decoded.crop_imm(
        image.placement.source.x,
        image.placement.source.y,
        image.placement.source.width,
        image.placement.source.height,
    );
    if !image.has_alpha {
        return Ok((decoded.to_rgb8().into_raw(), None));
    }
    let rgba = decoded.to_rgba8();
    let pixels = usize::try_from(u64::from(rgba.width()) * u64::from(rgba.height()))
        .map_err(|_| FileMakerError::new(ErrorCode::LimitExceeded, "PDF pixel count overflow"))?;
    let rgb_capacity = pixels.checked_mul(3).ok_or_else(|| {
        FileMakerError::new(ErrorCode::LimitExceeded, "PDF RGB byte count overflow")
    })?;
    let mut rgb = Vec::with_capacity(rgb_capacity);
    let mut alpha = Vec::with_capacity(pixels);
    for pixel in rgba.pixels() {
        rgb.extend_from_slice(&pixel.0[..3]);
        alpha.push(pixel[3]);
    }
    Ok((rgb, Some(alpha)))
}

fn image_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::AssetInvalid, message)
}
