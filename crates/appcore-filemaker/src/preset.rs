// =============================================================================
//        #######
//     ###       ###     F: preset.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded preset contracts and behavior for this crate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ErrorCode, FileMakerError, Result, Size, Unit};

/// Page/canvas orientation applied independently from its preset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Preset width then height.
    #[default]
    Portrait,
    /// Preset dimensions swapped.
    Landscape,
}

/// Versioned named size.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    /// Stable preset name.
    pub name: String,
    /// Registry version that defines its meaning.
    pub version: u16,
    /// Portrait/reference size.
    pub size: Size,
}

impl Preset {
    /// Applies orientation without mutating the registry value.
    #[must_use]
    pub fn oriented_size(&self, orientation: Orientation) -> Size {
        match orientation {
            Orientation::Portrait => self.size,
            Orientation::Landscape => Size {
                width: self.size.height,
                height: self.size.width,
            },
        }
    }
}

/// Deterministically ordered built-in and custom preset registry.
#[derive(Clone, Debug)]
pub struct PresetRegistry {
    presets: BTreeMap<String, Preset>,
}

impl Default for PresetRegistry {
    fn default() -> Self {
        // appcore-norm: allow(clippy::expect_used) reason: the private built-in table is covered exhaustively by registry tests
        Self::v1().expect("built-in preset constants are valid")
    }
}

impl PresetRegistry {
    /// Creates the version-one registry.
    pub fn v1() -> Result<Self> {
        let mut registry = Self {
            presets: BTreeMap::new(),
        };
        for (name, width, height, unit) in BUILT_INS {
            let size = match *unit {
                "mm" => Size::new(mm(*width)?, mm(*height)?)?,
                "in" => Size::new(inches(*width)?, inches(*height)?)?,
                "mil" => Size::new(milli_inches(*width)?, milli_inches(*height)?)?,
                "px" => Size::new(px(*width)?, px(*height)?)?,
                _ => return Err(invalid_preset("invalid built-in preset unit")),
            };
            registry.register(Preset {
                name: (*name).to_owned(),
                version: 1,
                size,
            })?;
        }
        Ok(registry)
    }

    /// Registers a custom preset without replacing an existing name.
    pub fn register(&mut self, preset: Preset) -> Result<()> {
        if !valid_name(&preset.name) || preset.version == 0 {
            return Err(invalid_preset("invalid preset name or version"));
        }
        if self.presets.contains_key(&preset.name) {
            return Err(invalid_preset(format!(
                "duplicate preset `{}`",
                preset.name
            )));
        }
        self.presets.insert(preset.name.clone(), preset);
        Ok(())
    }

    /// Resolves a preset by exact stable name.
    pub fn get(&self, name: &str) -> Result<&Preset> {
        self.presets
            .get(name)
            .ok_or_else(|| invalid_preset(format!("unknown preset `{name}`")))
    }

    /// Iterates in stable lexical order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Preset)> {
        self.presets
            .iter()
            .map(|(name, preset)| (name.as_str(), preset))
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn mm(value: i64) -> Result<Unit> {
    Unit::from_ratio(i128::from(value) * 360, 127)
}

fn inches(value_hundredths: i64) -> Result<Unit> {
    Unit::from_ratio(i128::from(value_hundredths) * 72, 100)
}

fn milli_inches(value_thousandths: i64) -> Result<Unit> {
    Unit::from_ratio(i128::from(value_thousandths) * 72, 1_000)
}

fn px(value: i64) -> Result<Unit> {
    Unit::from_ratio(i128::from(value) * 3, 4)
}

fn invalid_preset(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::GeometryInvalid, message)
}

const BUILT_INS: &[(&str, i64, i64, &str)] = &[
    ("A0", 841, 1189, "mm"),
    ("A1", 594, 841, "mm"),
    ("A2", 420, 594, "mm"),
    ("A3", 297, 420, "mm"),
    ("A4", 210, 297, "mm"),
    ("A5", 148, 210, "mm"),
    ("A6", 105, 148, "mm"),
    ("A7", 74, 105, "mm"),
    ("A8", 52, 74, "mm"),
    ("A9", 37, 52, "mm"),
    ("A10", 26, 37, "mm"),
    ("B0", 1000, 1414, "mm"),
    ("B1", 707, 1000, "mm"),
    ("B2", 500, 707, "mm"),
    ("B3", 353, 500, "mm"),
    ("B4", 250, 353, "mm"),
    ("B5", 176, 250, "mm"),
    ("B6", 125, 176, "mm"),
    ("B7", 88, 125, "mm"),
    ("B8", 62, 88, "mm"),
    ("B9", 44, 62, "mm"),
    ("B10", 31, 44, "mm"),
    ("C0", 917, 1297, "mm"),
    ("C1", 648, 917, "mm"),
    ("C2", 458, 648, "mm"),
    ("C3", 324, 458, "mm"),
    ("C4", 229, 324, "mm"),
    ("C5", 162, 229, "mm"),
    ("C6", 114, 162, "mm"),
    ("C7", 81, 114, "mm"),
    ("C8", 57, 81, "mm"),
    ("C9", 40, 57, "mm"),
    ("C10", 28, 40, "mm"),
    ("Letter", 850, 1100, "in"),
    ("Legal", 850, 1400, "in"),
    ("Tabloid", 1100, 1700, "in"),
    ("Ledger", 1700, 1100, "in"),
    ("Executive", 725, 1050, "in"),
    ("HD", 1280, 720, "px"),
    ("FHD", 1920, 1080, "px"),
    ("QHD", 2560, 1440, "px"),
    ("UHD", 3840, 2160, "px"),
    ("5K", 5120, 2880, "px"),
    ("8K", 7680, 4320, "px"),
    ("Ratio1x1", 1000, 1000, "px"),
    ("Ratio4x3", 1200, 900, "px"),
    ("Ratio3x2", 1200, 800, "px"),
    ("Ratio16x9", 1600, 900, "px"),
    ("Ratio21x9", 2100, 900, "px"),
    ("Photo4x6", 400, 600, "in"),
    ("Photo5x7", 500, 700, "in"),
    ("Photo8x10", 800, 1000, "in"),
    ("Photo10x15cm", 100, 150, "mm"),
    ("EnvelopeDL", 110, 220, "mm"),
    ("EnvelopeC5", 162, 229, "mm"),
    ("EnvelopeC6", 114, 162, "mm"),
    ("EnvelopeMonarch", 3875, 7500, "mil"),
    ("Label4x6", 400, 600, "in"),
    ("LabelAvery5160", 2625, 1000, "mil"),
    ("Thermal58", 58, 200, "mm"),
    ("Thermal80", 80, 200, "mm"),
    ("InstagramPost-v1", 1080, 1080, "px"),
    ("InstagramStory-v1", 1080, 1920, "px"),
    ("LinkedInPost-v1", 1200, 627, "px"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_is_separate_from_preset() {
        let registry = PresetRegistry::v1().unwrap();
        let a4 = registry.get("A4").unwrap();
        let landscape = a4.oriented_size(Orientation::Landscape);
        assert_eq!(landscape.width, a4.size.height);
        assert_eq!(landscape.height, a4.size.width);
    }

    #[test]
    fn v1_contains_all_declared_preset_families() {
        let registry = PresetRegistry::v1().unwrap();
        for name in [
            "A10",
            "B0",
            "B10",
            "C0",
            "C10",
            "Ratio16x9",
            "Photo4x6",
            "EnvelopeDL",
            "LabelAvery5160",
            "Thermal80",
            "InstagramStory-v1",
        ] {
            assert_eq!(registry.get(name).unwrap().version, 1);
        }
    }
}
