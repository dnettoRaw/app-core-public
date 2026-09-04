// =============================================================================
//        #######
//     ###       ###     F: style.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded style contracts and behavior for this crate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ErrorCode, FileMakerError, Result, Unit};

/// Format-neutral color retained until export.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "space", rename_all = "snake_case")]
pub enum Color {
    /// Eight-bit RGB.
    Rgb {
        /// Red channel.
        r: u8,
        /// Green channel.
        g: u8,
        /// Blue channel.
        b: u8,
    },
    /// Eight-bit RGB with alpha.
    Rgba {
        /// Red channel.
        r: u8,
        /// Green channel.
        g: u8,
        /// Blue channel.
        b: u8,
        /// Alpha channel.
        a: u8,
    },
    /// Eight-bit grayscale.
    Gray {
        /// Gray channel.
        value: u8,
    },
    /// CMYK channels in millionths.
    Cmyk {
        /// Cyan.
        c: u32,
        /// Magenta.
        m: u32,
        /// Yellow.
        y: u32,
        /// Black.
        k: u32,
    },
}

impl Color {
    /// Parses hex, stable named colors, and integer `rgb`, `rgba`, `gray`, or
    /// millionth-channel `cmyk` functions.
    pub fn parse(source: &str) -> Result<Self> {
        match source {
            "black" => return Ok(Self::Rgb { r: 0, g: 0, b: 0 }),
            "white" => {
                return Ok(Self::Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                })
            }
            "red" => return Ok(Self::Rgb { r: 255, g: 0, b: 0 }),
            "green" => return Ok(Self::Rgb { r: 0, g: 128, b: 0 }),
            "blue" => return Ok(Self::Rgb { r: 0, g: 0, b: 255 }),
            "transparent" => {
                return Ok(Self::Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                })
            }
            _ => {
                if source.ends_with(')') {
                    return parse_function_color(source);
                }
            }
        }
        let hex = source
            .strip_prefix('#')
            .ok_or_else(|| style_error("invalid color syntax"))?;
        match hex.len() {
            3 => Ok(Self::Rgb {
                r: duplicate_nibble(hex, 0)?,
                g: duplicate_nibble(hex, 1)?,
                b: duplicate_nibble(hex, 2)?,
            }),
            6 => Ok(Self::Rgb {
                r: hex_byte(hex, 0)?,
                g: hex_byte(hex, 2)?,
                b: hex_byte(hex, 4)?,
            }),
            8 => Ok(Self::Rgba {
                r: hex_byte(hex, 0)?,
                g: hex_byte(hex, 2)?,
                b: hex_byte(hex, 4)?,
                a: hex_byte(hex, 6)?,
            }),
            _ => Err(style_error("hex color requires 3, 6, or 8 digits")),
        }
    }

    /// Validates channel ranges not enforced by their storage type.
    pub fn validate(self) -> Result<()> {
        if let Self::Cmyk { c, m, y, k } = self {
            if [c, m, y, k].iter().any(|channel| *channel > 1_000_000) {
                return Err(style_error("CMYK channels must be at most 1000000"));
            }
        }
        Ok(())
    }

    /// Converts to RGBA for exporters that do not preserve CMYK.
    #[must_use]
    pub fn to_rgba(self) -> [u8; 4] {
        match self {
            Self::Rgb { r, g, b } => [r, g, b, 255],
            Self::Rgba { r, g, b, a } => [r, g, b, a],
            Self::Gray { value } => [value, value, value, 255],
            Self::Cmyk { c, m, y, k } => {
                let convert = |channel: u32| {
                    let combined = channel.saturating_add(k).min(1_000_000);
                    ((1_000_000 - combined) * 255 / 1_000_000) as u8
                };
                [convert(c), convert(m), convert(y), 255]
            }
        }
    }
}

/// Partial style layer.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// Fill.
    pub fill: Option<Color>,
    /// Stroke.
    pub stroke: Option<Color>,
    /// Stroke width.
    pub stroke_width: Option<Unit>,
    /// Opacity in millionths.
    pub opacity: Option<u32>,
    /// Explicit font asset name.
    pub font: Option<String>,
    /// Font size.
    pub font_size: Option<Unit>,
    /// Text foreground.
    pub color: Option<Color>,
}

/// Conditional partial style retained until typed data binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ElementStyleRule {
    /// Deterministic boolean expression.
    pub when: String,
    /// Partial style applied when the expression is truthy.
    pub style: Style,
}

impl Style {
    /// Validates field ranges.
    pub fn validate(&self) -> Result<()> {
        for color in [self.fill, self.stroke, self.color].into_iter().flatten() {
            color.validate()?;
        }
        if self.opacity.is_some_and(|value| value > 1_000_000)
            || self.stroke_width.is_some_and(|value| value < Unit::ZERO)
            || self.font_size.is_some_and(|value| value <= Unit::ZERO)
        {
            return Err(style_error("style range is invalid"));
        }
        Ok(())
    }

    pub(crate) fn overlay(&mut self, next: &Self) {
        if next.fill.is_some() {
            self.fill = next.fill;
        }
        if next.stroke.is_some() {
            self.stroke = next.stroke;
        }
        if next.stroke_width.is_some() {
            self.stroke_width = next.stroke_width;
        }
        if next.opacity.is_some() {
            self.opacity = next.opacity;
        }
        if next.font.is_some() {
            self.font.clone_from(&next.font);
        }
        if next.font_size.is_some() {
            self.font_size = next.font_size;
        }
        if next.color.is_some() {
            self.color = next.color;
        }
    }
}

fn parse_function_color(source: &str) -> Result<Color> {
    let (name, values) = source
        .split_once('(')
        .ok_or_else(|| style_error("invalid functional color syntax"))?;
    let values = values
        .strip_suffix(')')
        .ok_or_else(|| style_error("invalid functional color syntax"))?
        .split(',')
        .map(|value| {
            value
                .trim()
                .parse::<u32>()
                .map_err(|_| style_error("functional color channels must be unsigned integers"))
        })
        .collect::<Result<Vec<_>>>()?;
    match (name, values.as_slice()) {
        ("rgb", [r, g, b]) => Ok(Color::Rgb {
            r: channel_u8(*r)?,
            g: channel_u8(*g)?,
            b: channel_u8(*b)?,
        }),
        ("rgba", [r, g, b, a]) => Ok(Color::Rgba {
            r: channel_u8(*r)?,
            g: channel_u8(*g)?,
            b: channel_u8(*b)?,
            a: channel_u8(*a)?,
        }),
        ("gray", [value]) => Ok(Color::Gray {
            value: channel_u8(*value)?,
        }),
        ("cmyk", [c, m, y, k]) if [c, m, y, k].iter().all(|value| **value <= 1_000_000) => {
            Ok(Color::Cmyk {
                c: *c,
                m: *m,
                y: *y,
                k: *k,
            })
        }
        ("cmyk", _) => Err(style_error("CMYK requires four channels up to 1000000")),
        _ => Err(style_error("unsupported functional color syntax")),
    }
}

fn channel_u8(value: u32) -> Result<u8> {
    u8::try_from(value).map_err(|_| style_error("RGB/Gray channels must be at most 255"))
}

/// Fully resolved style.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComputedStyle {
    /// Fill.
    pub fill: Option<Color>,
    /// Stroke.
    pub stroke: Option<Color>,
    /// Stroke width.
    pub stroke_width: Unit,
    /// Opacity in millionths.
    pub opacity: u32,
    /// Explicit font asset name.
    pub font: Option<String>,
    /// Font size.
    pub font_size: Unit,
    /// Text foreground.
    pub color: Color,
}

impl ComputedStyle {
    /// Validates the resolved style before it crosses an exporter boundary.
    pub fn validate(&self) -> Result<()> {
        for color in [self.fill, self.stroke, Some(self.color)]
            .into_iter()
            .flatten()
        {
            color.validate()?;
        }
        if self.opacity > 1_000_000
            || self.stroke_width < Unit::ZERO
            || self.font_size <= Unit::ZERO
            || self
                .font
                .as_ref()
                .is_some_and(|font| font.is_empty() || font.len() > 128)
        {
            return Err(style_error("computed style range is invalid"));
        }
        Ok(())
    }
}

/// Named layers for the normative style cascade.
#[derive(Clone, Debug, Default)]
pub struct StyleCascade {
    /// Engine defaults.
    pub defaults: Style,
    /// Active theme.
    pub theme: Style,
    /// Template-level style.
    pub template: Style,
    /// Component layer.
    pub component: Style,
    /// Data-rule layer.
    pub data_rule: Style,
    /// Runtime override layer.
    pub runtime: Style,
    /// Export-specific override layer.
    pub export: Style,
}

impl StyleCascade {
    /// Computes defaults → theme → template → component → data → runtime → export.
    pub fn compute(&self) -> Result<ComputedStyle> {
        let mut merged = Style::default();
        for layer in [
            &self.defaults,
            &self.theme,
            &self.template,
            &self.component,
            &self.data_rule,
            &self.runtime,
            &self.export,
        ] {
            layer.validate()?;
            merged.overlay(layer);
        }
        Ok(ComputedStyle {
            fill: merged.fill,
            stroke: merged.stroke,
            stroke_width: merged.stroke_width.unwrap_or(Unit::ZERO),
            opacity: merged.opacity.unwrap_or(1_000_000),
            font: merged.font,
            font_size: merged.font_size.unwrap_or(Unit::points(12)?),
            color: merged.color.unwrap_or(Color::Rgb { r: 0, g: 0, b: 0 }),
        })
    }
}

/// Resolves `$token` references with bounded parent traversal.
pub fn resolve_token(
    token: &str,
    themes: &BTreeMap<String, BTreeMap<String, String>>,
    active_theme: &str,
    max_depth: usize,
) -> Result<String> {
    let key = token
        .strip_prefix('$')
        .ok_or_else(|| style_error("token must begin with `$`"))?;
    let mut current = active_theme;
    for _ in 0..max_depth {
        let theme = themes
            .get(current)
            .ok_or_else(|| style_error("theme was not found"))?;
        if let Some(value) = theme.get(key) {
            return Ok(value.clone());
        }
        current = theme.get("$extends").map_or("", String::as_str);
        if current.is_empty() {
            break;
        }
    }
    Err(style_error(format!("token `{token}` was not resolved")))
}

fn duplicate_nibble(hex: &str, offset: usize) -> Result<u8> {
    let value = u8::from_str_radix(&hex[offset..=offset], 16)
        .map_err(|_| style_error("invalid hex color digit"))?;
    Ok(value * 17)
}

fn hex_byte(hex: &str, offset: usize) -> Result<u8> {
    u8::from_str_radix(&hex[offset..offset + 2], 16)
        .map_err(|_| style_error("invalid hex color digit"))
}

fn style_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::SchemaField, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_layer_overrides_template_layer() {
        let cascade = StyleCascade {
            template: Style {
                fill: Some(Color::parse("#ff0000").unwrap()),
                ..Style::default()
            },
            runtime: Style {
                fill: Some(Color::parse("blue").unwrap()),
                ..Style::default()
            },
            ..StyleCascade::default()
        };
        assert_eq!(
            cascade.compute().unwrap().fill,
            Some(Color::Rgb { r: 0, g: 0, b: 255 })
        );
    }
}
