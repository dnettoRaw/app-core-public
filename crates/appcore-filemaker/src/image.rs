// =============================================================================
//        #######
//     ###       ###     F: image.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded image contracts and behavior for this crate.

use std::io::Cursor;

use image::ImageDecoder as _;
use rust_decimal::prelude::ToPrimitive as _;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{Asset, ErrorCode, FileMakerError, Rect, Result, Size, Unit};

/// How image pixels are mapped into an element box.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    /// Preserve aspect ratio and fit entirely inside the box.
    #[default]
    Contain,
    /// Preserve aspect ratio and crop to cover the box.
    Cover,
    /// Stretch the selected pixels to the complete box.
    Fill,
    /// Paint at the intrinsic 96-DPI CSS size without scaling.
    None,
    /// Use `none` when pixels fit, otherwise `contain`.
    ScaleDown,
}

/// Fractional crop insets expressed in parts per million.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImageCrop {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

/// Source-independent image paint options.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImageOptions {
    pub fit: ImageFit,
    /// Horizontal focal point in parts per million.
    pub focal_x: u32,
    /// Vertical focal point in parts per million.
    pub focal_y: u32,
    pub crop: ImageCrop,
    /// Apply raster EXIF orientation before crop and fit.
    pub respect_exif: bool,
}

impl Default for ImageOptions {
    fn default() -> Self {
        Self {
            fit: ImageFit::Contain,
            focal_x: 500_000,
            focal_y: 500_000,
            crop: ImageCrop::default(),
            respect_exif: true,
        }
    }
}

impl ImageOptions {
    pub fn validate(self) -> Result<()> {
        if self.focal_x > 1_000_000
            || self.focal_y > 1_000_000
            || self.crop.left > 1_000_000
            || self.crop.top > 1_000_000
            || self.crop.right > 1_000_000
            || self.crop.bottom > 1_000_000
            || self.crop.left.saturating_add(self.crop.right) >= 1_000_000
            || self.crop.top.saturating_add(self.crop.bottom) >= 1_000_000
        {
            return Err(image_error(
                "image crop/focal values must be valid ppm fractions",
            ));
        }
        Ok(())
    }
}

/// Orientation normalized from raster EXIF metadata.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageOrientation {
    #[default]
    Identity,
    Rotate90,
    Rotate180,
    Rotate270,
    FlipHorizontal,
    FlipVertical,
    Rotate90FlipHorizontal,
    Rotate270FlipHorizontal,
}

impl ImageOrientation {
    #[must_use]
    pub fn swaps_dimensions(self) -> bool {
        matches!(
            self,
            Self::Rotate90
                | Self::Rotate270
                | Self::Rotate90FlipHorizontal
                | Self::Rotate270FlipHorizontal
        )
    }

    pub(crate) fn apply(self, image: &mut image::DynamicImage) {
        image.apply_orientation(match self {
            Self::Identity => image::metadata::Orientation::NoTransforms,
            Self::Rotate90 => image::metadata::Orientation::Rotate90,
            Self::Rotate180 => image::metadata::Orientation::Rotate180,
            Self::Rotate270 => image::metadata::Orientation::Rotate270,
            Self::FlipHorizontal => image::metadata::Orientation::FlipHorizontal,
            Self::FlipVertical => image::metadata::Orientation::FlipVertical,
            Self::Rotate90FlipHorizontal => image::metadata::Orientation::Rotate90FlipH,
            Self::Rotate270FlipHorizontal => image::metadata::Orientation::Rotate270FlipH,
        });
    }
}

/// Integer source pixel rectangle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Fully resolved image paint geometry consumed by exporters.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImagePlacement {
    pub source: PixelRect,
    pub intrinsic_width: u32,
    pub intrinsic_height: u32,
    pub destination: Rect,
    pub clip: Rect,
    pub orientation: ImageOrientation,
    pub vector: bool,
}

/// Reads bounded metadata and resolves crop/fit geometry before export.
pub fn resolve_image_placement(
    asset: &Asset,
    bounds: Rect,
    options: ImageOptions,
    max_pixels: u64,
) -> Result<ImagePlacement> {
    options.validate()?;
    if bounds.size.width <= Unit::ZERO || bounds.size.height <= Unit::ZERO {
        return Err(image_error("image destination dimensions must be positive"));
    }
    bounds.right()?;
    bounds.bottom()?;
    let (mut width, mut height, orientation, vector) = image_metadata(asset, options.respect_exif)?;
    if u64::from(width) * u64::from(height) > max_pixels {
        return Err(FileMakerError::new(
            ErrorCode::LimitExceeded,
            "image pixel count exceeds configured limit",
        ));
    }
    if orientation.swaps_dimensions() {
        std::mem::swap(&mut width, &mut height);
    }
    let mut source = crop_rect(width, height, options.crop)?;
    let destination = match options.fit {
        ImageFit::Fill => bounds,
        ImageFit::Contain => contain(bounds, source.width, source.height)?,
        ImageFit::Cover => {
            source = cover_crop(source, bounds.size, options.focal_x, options.focal_y)?;
            bounds
        }
        ImageFit::None => intrinsic_destination(bounds, source.width, source.height)?,
        ImageFit::ScaleDown => {
            let intrinsic = intrinsic_destination(bounds, source.width, source.height)?;
            if intrinsic.size.width <= bounds.size.width
                && intrinsic.size.height <= bounds.size.height
            {
                intrinsic
            } else {
                contain(bounds, source.width, source.height)?
            }
        }
    };
    Ok(ImagePlacement {
        source,
        intrinsic_width: width,
        intrinsic_height: height,
        destination,
        clip: bounds,
        orientation,
        vector,
    })
}

fn image_metadata(asset: &Asset, respect_exif: bool) -> Result<(u32, u32, ImageOrientation, bool)> {
    if asset.media_type == "image/svg+xml" {
        let (width, height) = svg_dimensions(&asset.bytes)?;
        return Ok((width, height, ImageOrientation::Identity, true));
    }
    let reader = image::ImageReader::new(Cursor::new(&asset.bytes))
        .with_guessed_format()
        .map_err(|error| image_error(format!("cannot identify image: {error}")))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| image_error(format!("cannot read image metadata: {error}")))?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return Err(image_error("image dimensions must be non-zero"));
    }
    let orientation = if respect_exif {
        decoder
            .orientation()
            .map(ImageOrientation::from)
            .map_err(|error| image_error(format!("cannot read image orientation: {error}")))?
    } else {
        ImageOrientation::Identity
    };
    Ok((width, height, orientation, false))
}

impl From<image::metadata::Orientation> for ImageOrientation {
    fn from(value: image::metadata::Orientation) -> Self {
        match value {
            image::metadata::Orientation::NoTransforms => Self::Identity,
            image::metadata::Orientation::Rotate90 => Self::Rotate90,
            image::metadata::Orientation::Rotate180 => Self::Rotate180,
            image::metadata::Orientation::Rotate270 => Self::Rotate270,
            image::metadata::Orientation::FlipHorizontal => Self::FlipHorizontal,
            image::metadata::Orientation::FlipVertical => Self::FlipVertical,
            image::metadata::Orientation::Rotate90FlipH => Self::Rotate90FlipHorizontal,
            image::metadata::Orientation::Rotate270FlipH => Self::Rotate270FlipHorizontal,
        }
    }
}

fn crop_rect(width: u32, height: u32, crop: ImageCrop) -> Result<PixelRect> {
    let scale = |value: u32, fraction: u32| -> u32 {
        (u64::from(value) * u64::from(fraction) / 1_000_000).min(u64::from(u32::MAX)) as u32
    };
    let x = scale(width, crop.left);
    let y = scale(height, crop.top);
    let right = scale(width, crop.right);
    let bottom = scale(height, crop.bottom);
    let width = width.saturating_sub(x).saturating_sub(right);
    let height = height.saturating_sub(y).saturating_sub(bottom);
    if width == 0 || height == 0 {
        return Err(image_error("image crop produced an empty source"));
    }
    Ok(PixelRect {
        x,
        y,
        width,
        height,
    })
}

fn contain(bounds: Rect, width: u32, height: u32) -> Result<Rect> {
    let by_width = scale_unit_ratio(bounds.size.width, height, width)?;
    let size = if by_width <= bounds.size.height {
        Size::new(bounds.size.width, by_width)?
    } else {
        Size::new(
            scale_unit_ratio(bounds.size.height, width, height)?,
            bounds.size.height,
        )?
    };
    centered(bounds, size)
}

fn scale_unit_ratio(value: Unit, numerator: u32, denominator: u32) -> Result<Unit> {
    if denominator == 0 {
        return Err(image_error("image aspect-ratio denominator is zero"));
    }
    let denominator = i128::from(denominator);
    let raw = i128::from(value.raw())
        .checked_mul(i128::from(numerator))
        .and_then(|scaled| scaled.checked_add(denominator / 2))
        .ok_or_else(|| image_error("image aspect-ratio calculation overflow"))?
        / denominator;
    Ok(Unit::from_raw(i64::try_from(raw).map_err(|_| {
        image_error("image aspect-ratio result exceeds supported range")
    })?))
}

fn intrinsic_destination(bounds: Rect, width: u32, height: u32) -> Result<Rect> {
    let size = Size::new(
        Unit::from_ratio(i128::from(width) * 3, 4)?,
        Unit::from_ratio(i128::from(height) * 3, 4)?,
    )?;
    centered(bounds, size)
}

fn centered(bounds: Rect, size: Size) -> Result<Rect> {
    Rect::new(
        bounds.origin.x.checked_add(Unit::from_raw(
            bounds.size.width.checked_sub(size.width)?.raw() / 2,
        ))?,
        bounds.origin.y.checked_add(Unit::from_raw(
            bounds.size.height.checked_sub(size.height)?.raw() / 2,
        ))?,
        size.width,
        size.height,
    )
}

fn cover_crop(
    mut source: PixelRect,
    target: Size,
    focal_x: u32,
    focal_y: u32,
) -> Result<PixelRect> {
    let target_height = u128::try_from(target.height.raw())
        .map_err(|_| image_error("image cover target height is invalid"))?;
    let target_width = u128::try_from(target.width.raw())
        .map_err(|_| image_error("image cover target width is invalid"))?;
    if target_height == 0 || target_width == 0 {
        return Err(image_error(
            "image cover target dimensions must be positive",
        ));
    }
    let source_ratio = u128::from(source.width) * target_height;
    let target_ratio = u128::from(source.height) * target_width;
    if source_ratio > target_ratio {
        let width = u32::try_from(
            i128::from(source.height) * i128::from(target.width.raw())
                / i128::from(target.height.raw()),
        )
        .map_err(|_| image_error("image cover width overflow"))?;
        source.x = source
            .x
            .saturating_add(focal_offset(source.width, width, focal_x));
        source.width = width.max(1);
    } else if source_ratio < target_ratio {
        let height = u32::try_from(
            i128::from(source.width) * i128::from(target.height.raw())
                / i128::from(target.width.raw()),
        )
        .map_err(|_| image_error("image cover height overflow"))?;
        source.y = source
            .y
            .saturating_add(focal_offset(source.height, height, focal_y));
        source.height = height.max(1);
    }
    Ok(source)
}

fn focal_offset(full: u32, selected: u32, focal: u32) -> u32 {
    let center = u64::from(full) * u64::from(focal) / 1_000_000;
    let desired = center.saturating_sub(u64::from(selected) / 2);
    desired.min(u64::from(full.saturating_sub(selected))) as u32
}

fn svg_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    let text = std::str::from_utf8(bytes).map_err(|_| image_error("SVG is not UTF-8"))?;
    let marker = "viewBox=";
    let start = text
        .find(marker)
        .ok_or_else(|| image_error("SVG requires a viewBox"))?
        + marker.len();
    let quote = text
        .as_bytes()
        .get(start)
        .copied()
        .ok_or_else(|| image_error("invalid SVG viewBox"))?;
    if !matches!(quote, b'\'' | b'"') {
        return Err(image_error("SVG viewBox must be quoted"));
    }
    let rest = &text[start + 1..];
    let end = rest
        .find(char::from(quote))
        .ok_or_else(|| image_error("unterminated SVG viewBox"))?;
    let values = rest[..end]
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|value| !value.is_empty())
        .map(str::parse::<Decimal>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| image_error("invalid SVG viewBox number"))?;
    if values.len() != 4 || values[2] <= Decimal::ZERO || values[3] <= Decimal::ZERO {
        return Err(image_error("SVG viewBox must contain four valid numbers"));
    }
    let width = values[2]
        .ceil()
        .to_u32()
        .ok_or_else(|| image_error("SVG viewBox width exceeds supported range"))?;
    let height = values[3]
        .ceil()
        .to_u32()
        .ok_or_else(|| image_error("SVG viewBox height exceeds supported range"))?;
    Ok((width, height))
}

fn image_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::AssetInvalid, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_uses_focal_point_and_exact_target() {
        let asset = Asset::new(
            "vector.svg",
            "image/svg+xml",
            br#"<svg viewBox="0 0 200 100"></svg>"#.to_vec(),
        );
        let bounds = Rect::new(
            Unit::ZERO,
            Unit::ZERO,
            Unit::points(50).unwrap(),
            Unit::points(50).unwrap(),
        )
        .unwrap();
        let placement = resolve_image_placement(
            &asset,
            bounds,
            ImageOptions {
                fit: ImageFit::Cover,
                focal_x: 1_000_000,
                ..ImageOptions::default()
            },
            1_000_000,
        )
        .unwrap();
        assert_eq!(
            placement.source,
            PixelRect {
                x: 100,
                y: 0,
                width: 100,
                height: 100
            }
        );
        assert_eq!(placement.destination, bounds);
    }
}
