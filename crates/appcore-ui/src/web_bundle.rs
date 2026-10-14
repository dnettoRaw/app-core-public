// =============================================================================
//        #######
//     ###       ###     F: web_bundle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Immutable bounded web resources. No file, network or process access is implicit.
use crate::{UiError, UiResult};
use std::collections::HashMap;
/// Supported opaque resource kinds; media types are never inferred from an extension.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiWebMedia {
    /// UTF-8 HTML document.
    Html,
    /// UTF-8 JavaScript compiled by the application build.
    JavaScript,
    /// UTF-8 CSS.
    Css,
    /// PNG image bytes.
    Png,
    /// JPEG image bytes.
    Jpeg,
    /// WOFF2 font bytes.
    Woff2,
}
impl UiWebMedia {
    /// Exact MIME type emitted by the owning host.
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Html => "text/html; charset=utf-8",
            Self::JavaScript => "text/javascript; charset=utf-8",
            Self::Css => "text/css; charset=utf-8",
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Woff2 => "font/woff2",
        }
    }
}
/// Application-owned source asset. Display/script content is not included in Debug.
pub struct UiWebAsset {
    /// Canonical relative path, not a filesystem path or URL.
    pub path: String,
    /// Explicit resource kind.
    pub media: UiWebMedia,
    /// Complete immutable bytes supplied by the application build.
    pub bytes: Vec<u8>,
}
/// Borrowed response from a validated bundle, without allocating another body.
pub struct UiWebResource<'a> {
    /// Explicit media type.
    pub media: UiWebMedia,
    /// Borrowed complete asset bytes.
    pub bytes: &'a [u8],
}
/// Application-built web document and its bounded immutable local resource set.
///
/// The initial policy requires external local script/style assets. Inline scripts,
/// remote requests, frames, forms and object plugins are not granted implicitly.
/// The platform host must emit the CSP and reject all other navigation/resource origins.
pub struct UiWebBundle {
    entry: String,
    assets: HashMap<String, UiWebAsset>,
    bytes: usize,
}
impl UiWebBundle {
    /// Validates at most 512 assets, 2 MiB per asset and 16 MiB aggregate bytes.
    /// UTF-8 text assets and an explicitly HTML entry are required.
    pub fn new(entry: String, assets: impl IntoIterator<Item = UiWebAsset>) -> UiResult<Self> {
        validate_path(&entry)?;
        let mut resources = HashMap::new();
        let mut bytes = 0_usize;
        for asset in assets {
            validate_path(&asset.path)?;
            if resources.len() >= 512 {
                return Err(UiError::Busy);
            }
            if asset.bytes.len() > 2 * 1024 * 1024 {
                return Err(UiError::PayloadTooLarge);
            }
            bytes = bytes
                .checked_add(asset.bytes.len())
                .ok_or(UiError::PayloadTooLarge)?;
            if bytes > 16 * 1024 * 1024 {
                return Err(UiError::PayloadTooLarge);
            }
            if matches!(
                asset.media,
                UiWebMedia::Html | UiWebMedia::JavaScript | UiWebMedia::Css
            ) {
                std::str::from_utf8(&asset.bytes).map_err(|_| UiError::InvalidRequest)?;
            }
            if resources.contains_key(&asset.path) {
                return Err(UiError::InvalidRequest);
            }
            resources.insert(asset.path.clone(), asset);
        }
        if resources
            .get(&entry)
            .is_none_or(|asset| asset.media != UiWebMedia::Html)
        {
            return Err(UiError::InvalidRequest);
        }
        Ok(Self {
            entry,
            assets: resources,
            bytes,
        })
    }
    /// Canonical entry resource selected by the application, never by page input.
    pub fn entry(&self) -> &str {
        &self.entry
    }
    /// Retained resource count.
    pub fn asset_count(&self) -> usize {
        self.assets.len()
    }
    /// Accounted aggregate asset bytes, not platform engine memory.
    pub fn asset_bytes(&self) -> usize {
        self.bytes
    }
    /// Exact canonical lookup; missing resources have no fallback route.
    pub fn resource(&self, path: &str) -> UiResult<UiWebResource<'_>> {
        validate_path(path)?;
        let asset = self.assets.get(path).ok_or(UiError::InvalidRequest)?;
        Ok(UiWebResource {
            media: asset.media,
            bytes: &asset.bytes,
        })
    }
    /// Restrictive initial document policy to be emitted by the platform host.
    pub fn content_security_policy(&self) -> &'static str {
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"
    }
}
fn validate_path(path: &str) -> UiResult<()> {
    if path.is_empty()
        || path.len() > 256
        || !path.is_ascii()
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(UiError::InvalidRequest);
    }
    Ok(())
}
