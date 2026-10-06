// =============================================================================
//        #######
//     ###       ###     F: qrcode.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Bounded UTF-8 QR Model 2 encoding and shared square-module vector geometry.

use crate::{ErrorCode, FileMakerError, PathCommand, Point, Rect, Result, Shape, Unit};
use ::qrcode::{bits::Bits, Color, EcLevel, Version};

/// Maximum UTF-8 payload bytes per symbol, checked before encoder allocation.
pub const MAX_QR_PAYLOAD_BYTES: usize = 1024;

/// QR Model 2 with UTF-8 ECI (26), byte mode and medium error correction.
/// Debug hides the reversible symbol. No URL is fetched or payload interpreted.
#[derive(Clone)]
pub struct QrCode {
    symbol: ::qrcode::QrCode,
}

impl std::fmt::Debug for QrCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QrCode")
            .field("width", &self.width())
            .field("modules", &"[REDACTED]")
            .finish()
    }
}

impl QrCode {
    /// Selects the smallest normal version that fits the exact UTF-8 bytes.
    /// Empty or oversized input fails without logging the payload.
    pub fn encode(value: &str) -> Result<Self> {
        if value.is_empty() {
            return Err(invalid("QR payload must not be empty"));
        }
        if value.len() > MAX_QR_PAYLOAD_BYTES {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "QR payload exceeds 1024 UTF-8 bytes",
            ));
        }
        for version in 1..=40 {
            let mut bits = Bits::new(Version::Normal(version));
            let encoded = bits
                .push_eci_designator(26)
                .and_then(|()| bits.push_byte_data(value.as_bytes()))
                .and_then(|()| bits.push_terminator(EcLevel::M));
            if encoded.is_ok() {
                let symbol = ::qrcode::QrCode::with_bits(bits, EcLevel::M)
                    .map_err(|_| invalid("QR encoding failed"))?;
                return Ok(Self { symbol });
            }
        }
        Err(invalid("QR payload cannot fit a normal symbol"))
    }

    /// Matrix width excluding the four-module quiet zone on each side.
    pub fn width(&self) -> usize {
        self.symbol.width()
    }

    /// Dark module at bounded coordinates; out-of-range coordinates return None.
    pub fn module(&self, x: usize, y: usize) -> Option<bool> {
        (x < self.width() && y < self.width()).then(|| self.symbol[(x, y)] == Color::Dark)
    }

    /// Centers a square symbol in the supplied box, with four clear modules each
    /// side. Requires at least one fixed-point quantum per module. Preserves
    /// square modules in rectangular boxes; no stretching or background paint.
    /// Rejects the command budget before allocating vector commands.
    pub fn shape(&self, bounds: Rect, max_commands: usize) -> Result<Shape> {
        let footprint = self.width() + 8;
        let step = bounds.size.width.raw().min(bounds.size.height.raw()) / footprint as i64;
        if step <= 0 {
            return Err(invalid("QR bounds cannot resolve positive square modules"));
        }
        let count = self.runs().count().checked_mul(5).ok_or_else(|| {
            FileMakerError::new(ErrorCode::LimitExceeded, "QR path command count overflow")
        })?;
        crate::ResourceLimits::check("QR path commands", count, max_commands)?;
        let side = step * footprint as i64;
        let origin = Point {
            x: bounds
                .origin
                .x
                .checked_add(Unit::from_raw((bounds.size.width.raw() - side) / 2))?,
            y: bounds
                .origin
                .y
                .checked_add(Unit::from_raw((bounds.size.height.raw() - side) / 2))?,
        };
        let mut commands = Vec::with_capacity(count);
        for (y, start, end) in self.runs() {
            let left = origin
                .x
                .checked_add(Unit::from_raw((start + 4) as i64 * step))?;
            let right = origin
                .x
                .checked_add(Unit::from_raw((end + 4) as i64 * step))?;
            let top = origin
                .y
                .checked_add(Unit::from_raw((y + 4) as i64 * step))?;
            let bottom = top.checked_add(Unit::from_raw(step))?;
            commands.extend([
                PathCommand::Move {
                    to: Point { x: left, y: top },
                },
                PathCommand::Line {
                    to: Point { x: right, y: top },
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

    fn runs(&self) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
        (0..self.width()).flat_map(move |y| {
            let mut x = 0;
            std::iter::from_fn(move || {
                while x < self.width() && self.symbol[(x, y)] != Color::Dark {
                    x += 1;
                }
                if x == self.width() {
                    return None;
                }
                let start = x;
                while x < self.width() && self.symbol[(x, y)] == Color::Dark {
                    x += 1;
                }
                Some((y, start, x))
            })
        })
    }
}

fn invalid(message: &str) -> FileMakerError {
    FileMakerError::new(ErrorCode::Validation, message)
}
