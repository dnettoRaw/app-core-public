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

const RASTER_TILE_TARGET_BYTES: usize = 4 * 1024 * 1024;
const RASTER_MAX_SCANLINE_BYTES: usize = 4 * 1024 * 1024;
const RASTER_MAX_TILE_ROWS: u32 = 256;

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
            tile_rows: bounded_tile_rows(width)?,
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
        if height == 0 || bottom > self.height {
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
    let scanline = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or_else(|| limit_error("raster scanline byte count overflow"))?;
    if scanline > RASTER_MAX_SCANLINE_BYTES {
        return Err(limit_error("raster scanline exceeds the memory budget"));
    }
    let rows = (RASTER_TILE_TARGET_BYTES / scanline).max(1);
    Ok(u32::try_from(rows)
        .unwrap_or(u32::MAX)
        .min(RASTER_MAX_TILE_ROWS))
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
    use super::{bounded_tile_rows, RASTER_MAX_SCANLINE_BYTES, RASTER_MAX_TILE_ROWS};

    #[test]
    fn raster_tiles_have_a_fixed_memory_ceiling() {
        for width in [1, 1_920, 2_480, 100_000, 1_048_576] {
            let rows = bounded_tile_rows(width).unwrap();
            assert!((1..=RASTER_MAX_TILE_ROWS).contains(&rows));
            let bytes = width as usize * rows as usize * 4;
            assert!(bytes <= super::RASTER_TILE_TARGET_BYTES.max(width as usize * 4));
        }
        assert!(bounded_tile_rows((RASTER_MAX_SCANLINE_BYTES / 4 + 1) as u32).is_err());
    }
}
