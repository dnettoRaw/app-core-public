// =============================================================================
//        #######
//     ###       ###     F: barcode.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/05 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Deterministic EAN-8/EAN-13 symbol geometry shared by all visual exporters.

use crate::{ErrorCode, FileMakerError, PathCommand, Point, Rect, Result, Shape, Unit};

const LEFT: [&str; 10] = [
    "0001101", "0011001", "0010011", "0111101", "0100011", "0110001", "0101111", "0111011",
    "0110111", "0001011",
];
const PARITY: [&str; 10] = [
    "LLLLLL", "LLGLGG", "LLGGLG", "LLGGGL", "LGLLGG", "LGGLLG", "LGGGLL", "LGLGLG", "LGLGGL",
    "LGGLGL",
];

/// EAN-13 validated digits and a fixed 95-module bar pattern.
/// Quiet zones belong to the geometry footprint, not this pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ean13 {
    digits: [u8; 13],
    modules: [bool; 95],
}
impl Ean13 {
    /// Accepts exactly 13 ASCII digits, preserving leading zeroes and verifying
    /// the supplied check digit. Does not infer, pad or repair invalid inputs.
    pub fn parse(value: &str) -> Result<Self> {
        if value.len() != 13 || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid("EAN-13 requires exactly 13 ASCII digits"));
        }
        let mut digits = [0; 13];
        for (digit, byte) in digits.iter_mut().zip(value.bytes()) {
            *digit = byte - b'0';
        }
        let sum = digits[..12]
            .iter()
            .enumerate()
            .map(|(i, d)| u32::from(*d) * if i % 2 == 0 { 1 } else { 3 })
            .sum::<u32>();
        if u32::from(digits[12]) != (10 - sum % 10) % 10 {
            return Err(invalid("EAN-13 check digit mismatch"));
        }
        let mut modules = [false; 95];
        modules[..3].copy_from_slice(&[true, false, true]);
        modules[45..50].copy_from_slice(&[false, true, false, true, false]);
        modules[92..].copy_from_slice(&[true, false, true]);
        for i in 0..6 {
            let left = LEFT[usize::from(digits[i + 1])].as_bytes();
            let use_g = PARITY[usize::from(digits[0])].as_bytes()[i] == b'G';
            let right = LEFT[usize::from(digits[i + 7])].as_bytes();
            for j in 0..7 {
                modules[3 + i * 7 + j] = if use_g {
                    left[6 - j] == b'0'
                } else {
                    left[j] == b'1'
                };
                modules[50 + i * 7 + j] = right[j] == b'0';
            }
        }
        Ok(Self { digits, modules })
    }
    /// Fixed symbol modules, excluding quiet zones (true means a painted bar).
    pub fn modules(&self) -> &[bool; 95] {
        &self.modules
    }
    /// Original numeric digits, including check digit and leading zeroes.
    pub fn digits(&self) -> &[u8; 13] {
        &self.digits
    }
    /// Generates bars inside a 113-module footprint: 11 blank modules left and
    /// 7 right. Human-readable text is a separate YAML text element. The caller
    /// owns print size, contrast, clear background and scanner certification.
    pub fn shape(&self, bounds: Rect) -> Result<Shape> {
        linear_shape(&self.modules, bounds, 11, 7)
    }
}
/// EAN-8 validated digits and a fixed 67-module pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ean8 {
    digits: [u8; 8],
    modules: [bool; 67],
}
impl Ean8 {
    /// Accepts exactly eight ASCII digits, preserving zeroes and checking checksum.
    pub fn parse(value: &str) -> Result<Self> {
        if value.len() != 8 || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid("EAN-8 requires exactly 8 ASCII digits"));
        }
        let mut digits = [0; 8];
        for (digit, byte) in digits.iter_mut().zip(value.bytes()) {
            *digit = byte - b'0';
        }
        let sum = digits[..7]
            .iter()
            .enumerate()
            .map(|(i, d)| u32::from(*d) * if i % 2 == 0 { 3 } else { 1 })
            .sum::<u32>();
        if u32::from(digits[7]) != (10 - sum % 10) % 10 {
            return Err(invalid("EAN-8 check digit mismatch"));
        }
        let mut modules = [false; 67];
        modules[..3].copy_from_slice(&[true, false, true]);
        modules[31..36].copy_from_slice(&[false, true, false, true, false]);
        modules[64..].copy_from_slice(&[true, false, true]);
        for i in 0..4 {
            let left = LEFT[usize::from(digits[i])].as_bytes();
            let right = LEFT[usize::from(digits[i + 4])].as_bytes();
            for j in 0..7 {
                modules[3 + i * 7 + j] = left[j] == b'1';
                modules[36 + i * 7 + j] = right[j] == b'0';
            }
        }
        Ok(Self { digits, modules })
    }
    /// Fixed modules excluding quiet zones.
    pub fn modules(&self) -> &[bool; 67] {
        &self.modules
    }
    /// Original numeric digits including checksum.
    pub fn digits(&self) -> &[u8; 8] {
        &self.digits
    }
    /// Vector bars in an 81-module footprint including seven quiet modules each side.
    /// Human-readable digits remain a separate YAML text node.
    pub fn shape(&self, bounds: Rect) -> Result<Shape> {
        linear_shape(&self.modules, bounds, 7, 7)
    }
}

/// Validates a complete EAN-8 or EAN-13 by its exact length; never pads or repairs.
/// Two/five-digit supplements, UPC and GTIN-14 are not inferred.
pub fn ean_shape(value: &str, bounds: Rect) -> Result<Shape> {
    match value.len() {
        8 => Ean8::parse(value)?.shape(bounds),
        13 => Ean13::parse(value)?.shape(bounds),
        _ => Err(invalid("barcode requires a complete EAN-8 or EAN-13")),
    }
}

fn linear_shape(
    modules: &[bool],
    bounds: Rect,
    quiet_left: usize,
    quiet_right: usize,
) -> Result<Shape> {
    let footprint = quiet_left + modules.len() + quiet_right;
    if bounds.size.width.raw() < footprint as i64 || bounds.size.height <= Unit::ZERO {
        return Err(invalid("EAN bounds cannot resolve positive bar modules"));
    }
    let bottom = bounds.origin.y.checked_add(bounds.size.height)?;
    let x_at = |module: usize| -> Result<Unit> {
        let offset = i128::from(bounds.size.width.raw()) * module as i128 / footprint as i128;
        bounds.origin.x.checked_add(Unit::from_raw(
            i64::try_from(offset).map_err(|_| invalid("EAN geometry overflow"))?,
        ))
    };
    let mut commands = Vec::with_capacity(modules.len() * 5);
    let mut index = 0;
    while index < modules.len() {
        if !modules[index] {
            index += 1;
            continue;
        }
        let start = index;
        while index < modules.len() && modules[index] {
            index += 1;
        }
        let left = x_at(quiet_left + start)?;
        let right = x_at(quiet_left + index)?;
        commands.extend([
            PathCommand::Move {
                to: Point {
                    x: left,
                    y: bounds.origin.y,
                },
            },
            PathCommand::Line {
                to: Point {
                    x: right,
                    y: bounds.origin.y,
                },
            },
            PathCommand::Line {
                to: Point {
                    x: right,
                    y: bottom,
                },
            },
            PathCommand::Line {
                to: Point { x: left, y: bottom },
            },
            PathCommand::Close,
        ]);
    }
    Ok(Shape::Path { bounds, commands })
}

fn invalid(message: &str) -> FileMakerError {
    FileMakerError::new(ErrorCode::Validation, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_gs1_pattern_and_check_digit_are_preserved() {
        let symbol = Ean13::parse("4006381333931").unwrap();
        let bits = symbol
            .modules
            .iter()
            .map(|b| if *b { '1' } else { '0' })
            .collect::<String>();
        assert_eq!(bits, "10100011010100111010111101111010001001011001101010100001010000101000010111010010000101100110101");
        assert_eq!(Ean13::parse("0000000000000").unwrap().digits()[0], 0);
        for input in [
            "4006381333932",
            "400638133393",
            "40063813339311",
            "４006381333931",
            " 006381333931",
        ] {
            assert!(Ean13::parse(input).is_err());
        }
    }
    #[test]
    fn geometry_keeps_quiet_zones_and_checked_bounds() {
        let symbol = Ean13::parse("4006381333931").unwrap();
        let rect = Rect::new(
            Unit::ZERO,
            Unit::ZERO,
            Unit::points(113).unwrap(),
            Unit::points(30).unwrap(),
        )
        .unwrap();
        let Shape::Path { commands, .. } = symbol.shape(rect).unwrap() else {
            panic!("path required")
        };
        assert_eq!(commands.len(), 150);
        assert_eq!(
            commands.first(),
            Some(&PathCommand::Move {
                to: Point {
                    x: Unit::points(11).unwrap(),
                    y: Unit::ZERO
                }
            })
        );
    }
}
