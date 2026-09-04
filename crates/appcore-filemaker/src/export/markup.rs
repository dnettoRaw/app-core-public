// =============================================================================
//        #######
//     ###       ###     F: markup.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Shared allocation-bounded SVG and HTML formatting primitives.

use base64::Engine as _;

use super::bounded_string::FormattedOutput;
use crate::{Color, ErrorCode, FileMakerError, Result};

pub(super) fn write_base64(output: &mut dyn FormattedOutput, bytes: &[u8]) -> Result<()> {
    const INPUT_CHUNK_BYTES: usize = 3 * 1024;
    let mut encoded = [0_u8; 4 * 1024];
    for chunk in bytes.chunks(INPUT_CHUNK_BYTES) {
        let length = base64::encoded_len(chunk.len(), true)
            .ok_or_else(|| FileMakerError::new(ErrorCode::LimitExceeded, "base64 size overflow"))?;
        let written = base64::engine::general_purpose::STANDARD
            .encode_slice(chunk, &mut encoded[..length])
            .map_err(|error| {
                FileMakerError::new(
                    ErrorCode::ExportWrite,
                    format!("cannot encode embedded asset: {error}"),
                )
            })?;
        output.write_bytes(&encoded[..written])?;
    }
    Ok(())
}

pub(super) fn normalized_image_bytes<'a>(
    asset: &'a crate::Asset,
    orientation: crate::ImageOrientation,
) -> Result<(std::borrow::Cow<'a, str>, std::borrow::Cow<'a, [u8]>)> {
    if asset.media_type == "image/svg+xml" || orientation == crate::ImageOrientation::Identity {
        return Ok((
            std::borrow::Cow::Borrowed(&asset.media_type),
            std::borrow::Cow::Borrowed(&asset.bytes),
        ));
    }
    let mut decoded = image::load_from_memory(&asset.bytes).map_err(|error| {
        FileMakerError::new(
            ErrorCode::AssetInvalid,
            format!("cannot normalize image orientation: {error}"),
        )
    })?;
    orientation.apply(&mut decoded);
    let mut bytes = std::io::Cursor::new(Vec::new());
    decoded
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| {
            FileMakerError::new(
                ErrorCode::ExportWrite,
                format!("cannot encode oriented image: {error}"),
            )
        })?;
    Ok((
        std::borrow::Cow::Borrowed("image/png"),
        std::borrow::Cow::Owned(bytes.into_inner()),
    ))
}

pub(super) fn color(value: Color) -> String {
    let [r, g, b, a] = value.to_rgba();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r},{g},{b},{})", f32::from(a) / 255.0)
    }
}

pub(super) fn points(value: crate::Unit) -> String {
    format!("{:.6}", value.as_points_f64())
}

pub(super) fn opacity(value: u32) -> String {
    format!("{:.6}", f64::from(value) / 1_000_000.0)
}

pub(super) struct Escaped<'a>(&'a str);

impl std::fmt::Display for Escaped<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut start = 0;
        for (index, character) in self.0.char_indices() {
            let replacement = match character {
                '&' => "&amp;",
                '<' => "&lt;",
                '>' => "&gt;",
                '"' => "&quot;",
                '\'' => "&#39;",
                _ => continue,
            };
            formatter.write_str(&self.0[start..index])?;
            formatter.write_str(replacement)?;
            start = index + character.len_utf8();
        }
        formatter.write_str(&self.0[start..])
    }
}

pub(super) const fn escape(value: &str) -> Escaped<'_> {
    Escaped(value)
}
