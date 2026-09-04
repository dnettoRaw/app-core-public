// =============================================================================
//        #######
//     ###       ###     F: font.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded font contracts and behavior for this crate.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use skrifa::{instance::Size, FontRef, MetadataProvider};

use crate::{ErrorCode, FileMakerError, Result};

/// Explicit immutable font bytes.
#[derive(Clone, Debug)]
pub struct FontAsset {
    /// Stable logical family/name used by templates.
    pub name: String,
    /// TrueType/OpenType bytes.
    pub bytes: Arc<[u8]>,
    /// Face index for collections.
    pub face_index: u32,
    /// SHA-256 digest used by fingerprints.
    pub digest: [u8; 32],
}

impl FontAsset {
    /// Validates font bytes and creates an asset.
    pub fn new(name: impl Into<String>, bytes: Vec<u8>, face_index: u32) -> Result<Self> {
        let name = name.into();
        if name.is_empty() || name.len() > 128 {
            return Err(font_error("font name is empty or too long"));
        }
        FontRef::from_index(&bytes, face_index)
            .map_err(|_| font_error("font bytes or face index are invalid"))?;
        let digest = Sha256::digest(&bytes).into();
        Ok(Self {
            name,
            bytes: bytes.into(),
            face_index,
            digest,
        })
    }

    /// Returns whether this font maps every scalar in a grapheme cluster.
    #[must_use]
    pub fn covers(&self, text: &str) -> bool {
        let Ok(face) = FontRef::from_index(&self.bytes, self.face_index) else {
            return false;
        };
        let charmap = face.charmap();
        text.chars()
            .all(|character| character.is_control() || charmap.map(character).is_some())
    }

    /// Returns units per em.
    pub fn units_per_em(&self) -> Result<u16> {
        let face = FontRef::from_index(&self.bytes, self.face_index)
            .map_err(|_| font_error("registered font became invalid"))?;
        let units_per_em = face
            .metrics(Size::unscaled(), skrifa::instance::LocationRef::default())
            .units_per_em;
        if units_per_em == 0 {
            return Err(font_error("registered font has no units-per-em"));
        }
        Ok(units_per_em)
    }
}

/// Explicit font resolver; implementations must not discover OS fonts.
pub trait FontResolver: Send + Sync {
    /// Resolves one exact logical font name under a byte cap.
    fn resolve_font(&self, name: &str, max_bytes: usize) -> Result<FontAsset>;
}

/// Deterministic font registry and fallback order.
#[derive(Clone, Debug, Default)]
pub struct FontManager {
    fonts: BTreeMap<String, FontAsset>,
    fallback: Vec<String>,
}

impl FontManager {
    /// Resolves and registers one exact font under a caller-supplied byte cap.
    pub fn register_from(
        &mut self,
        resolver: &dyn FontResolver,
        name: &str,
        max_bytes: usize,
    ) -> Result<()> {
        self.register(resolver.resolve_font(name, max_bytes)?)
    }

    /// Registers a font without replacing an existing name.
    pub fn register(&mut self, font: FontAsset) -> Result<()> {
        if self.fonts.contains_key(&font.name) {
            return Err(font_error(format!(
                "font `{}` is already registered",
                font.name
            )));
        }
        self.fonts.insert(font.name.clone(), font);
        Ok(())
    }

    /// Replaces the global fallback list after validating every exact name.
    pub fn set_fallback(&mut self, fallback: Vec<String>) -> Result<()> {
        if fallback.len() > 64 || fallback.iter().any(|name| !self.fonts.contains_key(name)) {
            return Err(font_error(
                "fallback contains too many fonts or an unknown name",
            ));
        }
        self.fallback = fallback;
        Ok(())
    }

    /// Resolves exact name.
    pub fn get(&self, name: &str) -> Result<&FontAsset> {
        self.fonts
            .get(name)
            .ok_or_else(|| font_error(format!("font `{name}` is not registered")))
    }

    /// Selects the first explicit primary/fallback font covering a grapheme.
    pub fn select_for_grapheme<'a>(
        &'a self,
        primary: &str,
        grapheme: &str,
    ) -> Result<&'a FontAsset> {
        let primary = self.get(primary)?;
        if primary.covers(grapheme) {
            return Ok(primary);
        }
        for name in &self.fallback {
            let font = self.get(name)?;
            if font.covers(grapheme) {
                return Ok(font);
            }
        }
        let scalar = grapheme
            .chars()
            .find(|value| !value.is_control())
            .map_or(0, u32::from);
        Err(font_error(format!(
            "no explicit font contains glyph U+{scalar:04X}"
        )))
    }

    /// Returns stable font digests in lexical name order.
    pub fn digests(&self) -> impl Iterator<Item = (&str, &[u8; 32])> {
        self.fonts
            .iter()
            .map(|(name, font)| (name.as_str(), &font.digest))
    }

    /// Returns the explicit fallback order used during shaping.
    pub fn fallback_names(&self) -> impl Iterator<Item = &str> {
        self.fallback.iter().map(String::as_str)
    }
}

/// Glyph usage prepared for an embedding/subsetting exporter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontSubset {
    /// Registered font name.
    pub font: String,
    /// Sorted unique glyph IDs.
    pub glyph_ids: BTreeSet<u16>,
}

impl FontSubset {
    /// Creates an empty usage set.
    #[must_use]
    pub fn new(font: impl Into<String>) -> Self {
        Self {
            font: font.into(),
            glyph_ids: BTreeSet::new(),
        }
    }

    /// Records a glyph ID.
    pub fn record(&mut self, glyph_id: u16) {
        self.glyph_ids.insert(glyph_id);
    }
}

fn font_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::FontMissing, message)
}
