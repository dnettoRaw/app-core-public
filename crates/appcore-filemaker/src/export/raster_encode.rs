// =============================================================================
//        #######
//     ###       ###     F: raster_encode.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Streams bounded raster strips without retaining the complete pixel surface.

use std::cell::RefCell;
use std::io::{self, Write};

use image::{GenericImageView, Rgb};
use tiny_skia::Pixmap;

use crate::{ErrorCode, ExportContext, ExportFormat, ExportRequest, FileMakerError, Result};

pub(crate) type StripRenderer<'a> = dyn FnMut(u32, u32) -> Result<Pixmap> + 'a;

#[allow(clippy::too_many_arguments)]
pub(super) fn encode_tiled(
    width: u32,
    height: u32,
    tile_rows: u32,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    writer: &mut dyn Write,
    render: &mut StripRenderer<'_>,
) -> Result<usize> {
    if request.format == ExportFormat::Png {
        return encode_png_tiled(
            width,
            height,
            tile_rows,
            context.limits.max_output_bytes,
            writer,
            render,
        );
    }
    let mut bounded = BoundedOutput::new(writer, context.limits.max_output_bytes);
    let result = match request.format {
        ExportFormat::Jpeg => encode_jpeg(
            width,
            height,
            tile_rows,
            request.jpeg_quality,
            &mut bounded,
            render,
        ),
        _ => return Err(export_error("raster encoder received a non-JPEG format")),
    };
    if bounded.exceeded {
        return Err(limit_error("raster output exceeds configured byte limit"));
    }
    result?;
    Ok(bounded.written)
}

pub(crate) fn encode_png_tiled(
    width: u32,
    height: u32,
    tile_rows: u32,
    max_output_bytes: usize,
    writer: &mut dyn Write,
    render: &mut StripRenderer<'_>,
) -> Result<usize> {
    let mut bounded = BoundedOutput::new(writer, max_output_bytes);
    let result = encode_png(width, height, tile_rows, &mut bounded, render);
    if bounded.exceeded {
        return Err(limit_error("raster output exceeds configured byte limit"));
    }
    result?;
    Ok(bounded.written)
}

fn encode_png(
    width: u32,
    height: u32,
    tile_rows: u32,
    writer: &mut impl Write,
    render: &mut StripRenderer<'_>,
) -> Result<()> {
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut header = encoder
        .write_header()
        .map_err(|error| export_error(format!("cannot write PNG header: {error}")))?;
    let mut stream = header
        .stream_writer()
        .map_err(|error| export_error(format!("cannot start PNG stream: {error}")))?;
    let mut top = 0_u32;
    while top < height {
        let rows = tile_rows.min(height - top);
        let mut pixmap = render(top, rows)?;
        validate_strip(&pixmap, width, rows)?;
        demultiply_in_place(pixmap.data_mut());
        stream
            .write_all(pixmap.data())
            .map_err(|error| export_error(format!("cannot stream PNG pixels: {error}")))?;
        top = top
            .checked_add(rows)
            .ok_or_else(|| limit_error("PNG strip offset overflow"))?;
    }
    stream
        .finish()
        .map_err(|error| export_error(format!("cannot finish PNG stream: {error}")))?;
    header
        .finish()
        .map_err(|error| export_error(format!("cannot finish PNG output: {error}")))
}

fn encode_jpeg(
    width: u32,
    height: u32,
    tile_rows: u32,
    quality: u8,
    writer: &mut impl Write,
    render: &mut StripRenderer<'_>,
) -> Result<()> {
    let source = StripImage::new(width, height, tile_rows, render);
    let encoded =
        image::codecs::jpeg::JpegEncoder::new_with_quality(writer, quality).encode_image(&source);
    if let Some(error) = source.take_failure() {
        return Err(error);
    }
    encoded.map_err(|error| export_error(format!("cannot encode raster: {error}")))
}

fn validate_strip(pixmap: &Pixmap, width: u32, height: u32) -> Result<()> {
    if pixmap.width() != width || pixmap.height() != height {
        return Err(FileMakerError::new(
            ErrorCode::Validation,
            "raster strip renderer returned inconsistent dimensions",
        ));
    }
    Ok(())
}

fn demultiply_in_place(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = pixel[3];
        for channel in &mut pixel[..3] {
            *channel = demultiply_channel(*channel, alpha);
        }
    }
}

fn demultiply_channel(channel: u8, alpha: u8) -> u8 {
    match alpha {
        0 => 0,
        255 => channel,
        _ => ((u32::from(channel) * 255 + u32::from(alpha) / 2) / u32::from(alpha)) as u8,
    }
}

struct RenderedStrip {
    top: u32,
    pixmap: Pixmap,
}

struct StripImage<'a> {
    width: u32,
    height: u32,
    tile_rows: u32,
    render: RefCell<&'a mut StripRenderer<'a>>,
    cached: RefCell<Option<RenderedStrip>>,
    failure: RefCell<Option<FileMakerError>>,
}

impl<'a> StripImage<'a> {
    fn new(width: u32, height: u32, tile_rows: u32, render: &'a mut StripRenderer<'a>) -> Self {
        Self {
            width,
            height,
            tile_rows,
            render: RefCell::new(render),
            cached: RefCell::new(None),
            failure: RefCell::new(None),
        }
    }

    fn take_failure(&self) -> Option<FileMakerError> {
        self.failure.borrow_mut().take()
    }

    fn ensure_strip(&self, y: u32) {
        if self.failure.borrow().is_some()
            || self.cached.borrow().as_ref().is_some_and(|strip| {
                y >= strip.top && y < strip.top.saturating_add(strip.pixmap.height())
            })
        {
            return;
        }
        let top = y / self.tile_rows * self.tile_rows;
        let rows = self.tile_rows.min(self.height - top);
        let rendered = (self.render.borrow_mut())(top, rows)
            .and_then(|pixmap| validate_strip(&pixmap, self.width, rows).map(|()| pixmap));
        match rendered {
            Ok(pixmap) => {
                *self.cached.borrow_mut() = Some(RenderedStrip { top, pixmap });
            }
            Err(error) => {
                *self.failure.borrow_mut() = Some(error);
                *self.cached.borrow_mut() = None;
            }
        }
    }
}

impl GenericImageView for StripImage<'_> {
    type Pixel = Rgb<u8>;

    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn get_pixel(&self, x: u32, y: u32) -> Self::Pixel {
        self.ensure_strip(y);
        let cached = self.cached.borrow();
        let Some(strip) = cached.as_ref() else {
            return Rgb([255, 255, 255]);
        };
        let local_y = y.saturating_sub(strip.top);
        let index = (local_y as usize * self.width as usize + x as usize) * 4;
        let data = strip.pixmap.data();
        Rgb([data[index], data[index + 1], data[index + 2]])
    }
}

struct BoundedOutput<'a> {
    inner: &'a mut dyn Write,
    limit: usize,
    written: usize,
    exceeded: bool,
}

impl<'a> BoundedOutput<'a> {
    const fn new(inner: &'a mut dyn Write, limit: usize) -> Self {
        Self {
            inner,
            limit,
            written: 0,
            exceeded: false,
        }
    }
}

impl Write for BoundedOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(next) = self.written.checked_add(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("raster byte accounting overflow"));
        };
        if next > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("raster output limit exceeded"));
        }
        let written = self.inner.write(bytes)?;
        self.written += written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn export_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::ExportUnsupported, message)
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests {
    use super::encode_tiled;
    use crate::{ExportContext, ExportFormat, ExportRequest, FontManager, ResourceLimits};
    use image::GenericImageView as _;

    #[test]
    fn png_encoder_requests_and_streams_each_strip_once() {
        let limits = ResourceLimits::default();
        let fonts = FontManager::default();
        let context = ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        };
        let request = ExportRequest {
            format: ExportFormat::Png,
            ..ExportRequest::default()
        };
        let mut requested = Vec::new();
        let mut output = Vec::new();
        let outcome = encode_tiled(
            4,
            6,
            2,
            &request,
            &context,
            &mut output,
            &mut |top, rows| {
                requested.push((top, rows));
                let mut pixmap = tiny_skia::Pixmap::new(4, rows).unwrap();
                pixmap.fill(tiny_skia::Color::from_rgba8(top as u8, 20, 30, 255));
                Ok(pixmap)
            },
        )
        .unwrap();
        assert_eq!(requested, [(0, 2), (2, 2), (4, 2)]);
        assert_eq!(outcome, output.len());
        let decoded = image::load_from_memory(&output).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (4, 6));
        assert_eq!(decoded.get_pixel(0, 0)[0], 0);
        assert_eq!(decoded.get_pixel(0, 2)[0], 2);
        assert_eq!(decoded.get_pixel(0, 4)[0], 4);
    }

    #[test]
    fn jpeg_encoder_reads_bounded_strips_in_scan_order() {
        let limits = ResourceLimits::default();
        let fonts = FontManager::default();
        let context = ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        };
        let request = ExportRequest {
            format: ExportFormat::Jpeg,
            ..ExportRequest::default()
        };
        let mut requested = Vec::new();
        let mut output = Vec::new();
        encode_tiled(
            8,
            6,
            2,
            &request,
            &context,
            &mut output,
            &mut |top, rows| {
                requested.push((top, rows));
                let mut pixmap = tiny_skia::Pixmap::new(8, rows).unwrap();
                pixmap.fill(tiny_skia::Color::from_rgba8(20, top as u8, 30, 255));
                Ok(pixmap)
            },
        )
        .unwrap();
        assert_eq!(requested, [(0, 2), (2, 2), (4, 2)]);
        assert_eq!(
            image::load_from_memory(&output).unwrap().dimensions(),
            (8, 6)
        );
    }
}
