// =============================================================================
//        #######
//     ###       ###     F: raster_plan.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Bounds raster working memory to one vertical strip.

use tiny_skia::Pixmap;

use crate::{
    ErrorCode, ExportContext, ExportFormat, FileMakerError, ResolvedElement, Result, Unit,
};

const RASTER_MAX_SCANLINE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(super) struct RasterPage<'a> {
    pub(super) page: &'a crate::ResolvedPage,
    top: u32,
    height: u32,
}

pub(super) struct RasterPlan<'a> {
    pub(super) pages: Vec<RasterPage<'a>>,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) tile_rows: u32,
    scale: f32,
}

impl<'a> RasterPlan<'a> {
    pub(super) fn new(
        pages: &[&'a crate::ResolvedPage],
        dpi: u32,
        context: &ExportContext<'_>,
        options: super::RasterOptions,
    ) -> Result<Self> {
        let scale = f64::from(dpi) / 72.0;
        let width = pages.iter().try_fold(0_u32, |largest, page| {
            Ok::<_, FileMakerError>(largest.max(pixels(page.size.width, scale)?))
        })?;
        if width == 0 {
            return Err(export_error("raster has no selected page"));
        }
        let mut top = 0_u32;
        let mut bands = Vec::with_capacity(pages.len());
        for page in pages {
            let height = pixels(page.size.height, scale)?;
            bands.push(RasterPage { page, top, height });
            top = top
                .checked_add(height)
                .ok_or_else(|| limit_error("raster height overflow"))?;
        }
        let pixel_count = u64::from(width) * u64::from(top);
        if pixel_count > context.limits.max_pixels {
            return Err(limit_error("raster pixel count exceeds configured limit"));
        }
        Ok(Self {
            pages: bands,
            width,
            height: top,
            tile_rows: configured_tile_rows(width, options)?,
            scale: scale as f32,
        })
    }

    pub(super) fn render_strip(
        &self,
        top: u32,
        height: u32,
        context: &ExportContext<'_>,
        format: ExportFormat,
    ) -> Result<Pixmap> {
        let bottom = top
            .checked_add(height)
            .ok_or_else(|| limit_error("raster strip offset overflow"))?;
        if height == 0 || height > self.tile_rows || bottom > self.height {
            return Err(limit_error("raster encoder requested an invalid strip"));
        }
        let mut pixmap = Pixmap::new(self.width, height)
            .ok_or_else(|| limit_error("cannot allocate raster strip"))?;
        if format == ExportFormat::Jpeg {
            pixmap.fill(tiny_skia::Color::WHITE);
        }
        for page in &self.pages {
            let page_bottom = page
                .top
                .checked_add(page.height)
                .ok_or_else(|| limit_error("page raster offset overflow"))?;
            if page.top >= bottom || page_bottom <= top {
                continue;
            }
            let page_y = page.top as f32 - top as f32;
            for element in &page.page.elements {
                if element_intersects_strip(element, self.scale, page.top, top, bottom)? {
                    super::raster::render_element(
                        &mut pixmap,
                        element,
                        context,
                        self.scale,
                        page_y,
                    )?;
                }
            }
        }
        Ok(pixmap)
    }
}

pub(crate) fn bounded_tile_rows(width: u32) -> Result<u32> {
    configured_tile_rows(width, super::RasterOptions::default())
}

fn configured_tile_rows(width: u32, options: super::RasterOptions) -> Result<u32> {
    let scanline = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or_else(|| limit_error("raster scanline byte count overflow"))?;
    if scanline == 0 || scanline > RASTER_MAX_SCANLINE_BYTES {
        return Err(limit_error("raster scanline exceeds the memory budget"));
    }
    let rows = options.max_strip_bytes() / scanline;
    if rows == 0 {
        return Err(limit_error(
            "raster scanline exceeds configured strip byte limit",
        ));
    }
    Ok(u32::try_from(rows)
        .unwrap_or(u32::MAX)
        .min(options.max_strip_rows()))
}

fn element_intersects_strip(
    element: &ResolvedElement,
    scale: f32,
    page_top: u32,
    strip_top: u32,
    strip_bottom: u32,
) -> Result<bool> {
    let visual = element.bounds.visual;
    let top = super::raster::to_pixel(visual.origin.y, scale) + page_top as f32;
    let bottom = super::raster::to_pixel(visual.bottom()?, scale) + page_top as f32;
    Ok(bottom.ceil() + 1.0 > strip_top as f32 && top.floor() - 1.0 < strip_bottom as f32)
}

fn pixels(value: Unit, scale: f64) -> Result<u32> {
    let pixels = value.as_points_f64() * scale;
    if !pixels.is_finite() || pixels <= 0.0 || pixels > f64::from(u32::MAX) {
        return Err(limit_error("raster dimension is outside supported range"));
    }
    Ok(pixels.ceil() as u32)
}

fn export_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::ExportUnsupported, message)
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests {
    use super::{bounded_tile_rows, RASTER_MAX_SCANLINE_BYTES};

    #[test]
    fn renderer_rejects_requests_beyond_the_planned_strip_height() {
        let plan = super::RasterPlan {
            pages: Vec::new(),
            width: 4,
            height: 512,
            tile_rows: 256,
            scale: 1.0,
        };
        let limits = crate::ResourceLimits::default();
        let fonts = crate::FontManager::default();
        let context = crate::ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        };
        for (top, rows) in [(0, 0), (0, 257), (512, 1), (u32::MAX, 1)] {
            let error = plan
                .render_strip(top, rows, &context, crate::ExportFormat::Png)
                .unwrap_err();
            assert_eq!(error.code(), crate::ErrorCode::LimitExceeded);
        }
        let strip = plan
            .render_strip(256, 256, &context, crate::ExportFormat::Png)
            .unwrap();
        assert_eq!((strip.width(), strip.height()), (4, 256));
    }

    #[test]
    fn raster_tiles_have_a_fixed_memory_ceiling() {
        assert!(bounded_tile_rows(0).is_err());
        for width in [1, 1_920, 2_480, 100_000, 1_048_576] {
            let rows = bounded_tile_rows(width).unwrap();
            assert!((1..=256).contains(&rows));
            let bytes = width as usize * rows as usize * 4;
            assert!(bytes <= 4 * 1024 * 1024);
        }
        assert!(bounded_tile_rows((RASTER_MAX_SCANLINE_BYTES / 4 + 1) as u32).is_err());
    }

    #[test]
    fn custom_strip_budgets_never_round_up_past_the_byte_limit() {
        let options = crate::RasterOptions::new(64, 4096).unwrap();
        assert_eq!(super::configured_tile_rows(16, options).unwrap(), 1);
        assert!(super::configured_tile_rows(17, options).is_err());
        let largest = crate::RasterOptions::new(64 * 1024 * 1024, 4096).unwrap();
        assert_eq!(super::configured_tile_rows(16, largest).unwrap(), 4096);
        assert!(super::configured_tile_rows(1_048_577, largest).is_err());
    }
}
