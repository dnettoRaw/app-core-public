// =============================================================================
//        #######
//     ###       ###     F: geometry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded geometry contracts and behavior for this crate.

use serde::{Deserialize, Serialize};

use crate::transform_math::{fixed_sin_cos, matrix_term};
use crate::{ErrorCode, FileMakerError, Result, Unit};

/// A point in resolved page coordinates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: Unit,
    /// Vertical coordinate.
    pub y: Unit,
}

/// A non-negative resolved size.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Size {
    /// Width.
    pub width: Unit,
    /// Height.
    pub height: Unit,
}

impl Size {
    /// Creates a size after rejecting negative dimensions.
    pub fn new(width: Unit, height: Unit) -> Result<Self> {
        if width < Unit::ZERO || height < Unit::ZERO {
            return Err(invalid_geometry("size dimensions cannot be negative"));
        }
        Ok(Self { width, height })
    }
}

/// Axis-aligned resolved rectangle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Top-left origin.
    pub origin: Point,
    /// Non-negative size.
    pub size: Size,
}

impl Rect {
    /// Creates a rectangle after validating dimensions.
    pub fn new(x: Unit, y: Unit, width: Unit, height: Unit) -> Result<Self> {
        Ok(Self {
            origin: Point { x, y },
            size: Size::new(width, height)?,
        })
    }

    /// Checked right edge.
    pub fn right(self) -> Result<Unit> {
        self.origin.x.checked_add(self.size.width)
    }

    /// Checked bottom edge.
    pub fn bottom(self) -> Result<Unit> {
        self.origin.y.checked_add(self.size.height)
    }

    /// Whether rectangles overlap with positive area.
    pub fn intersects(self, other: Self) -> Result<bool> {
        Ok(self.origin.x < other.right()?
            && self.right()? > other.origin.x
            && self.origin.y < other.bottom()?
            && self.bottom()? > other.origin.y)
    }

    /// Intersection rectangle, when positive-area overlap exists.
    pub fn intersection(self, other: Self) -> Result<Option<Self>> {
        if !self.intersects(other)? {
            return Ok(None);
        }
        let x = self.origin.x.max(other.origin.x);
        let y = self.origin.y.max(other.origin.y);
        let right = self.right()?.min(other.right()?);
        let bottom = self.bottom()?.min(other.bottom()?);
        Ok(Some(Self::new(
            x,
            y,
            right.checked_sub(x)?,
            bottom.checked_sub(y)?,
        )?))
    }

    /// Smallest rectangle containing both inputs.
    pub fn union(self, other: Self) -> Result<Self> {
        let x = self.origin.x.min(other.origin.x);
        let y = self.origin.y.min(other.origin.y);
        let right = self.right()?.max(other.right()?);
        let bottom = self.bottom()?.max(other.bottom()?);
        Self::new(x, y, right.checked_sub(x)?, bottom.checked_sub(y)?)
    }

    /// Returns whether the point lies in the half-open rectangle.
    pub fn contains(self, point: Point) -> Result<bool> {
        Ok(point.x >= self.origin.x
            && point.x < self.right()?
            && point.y >= self.origin.y
            && point.y < self.bottom()?)
    }
}

/// Insets around a rectangle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Insets {
    /// Top inset.
    pub top: Unit,
    /// Right inset.
    pub right: Unit,
    /// Bottom inset.
    pub bottom: Unit,
    /// Left inset.
    pub left: Unit,
}

/// Fixed-point affine transform using millionth scale coefficients.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Horizontal scale/rotation coefficient.
    pub a: i64,
    /// Vertical shear/rotation coefficient.
    pub b: i64,
    /// Horizontal shear/rotation coefficient.
    pub c: i64,
    /// Vertical scale/rotation coefficient.
    pub d: i64,
    /// Horizontal translation.
    pub tx: Unit,
    /// Vertical translation.
    pub ty: Unit,
}

/// Fully resolved vector path command in page coordinates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum PathCommand {
    /// Starts a contour.
    Move { to: Point },
    /// Adds a straight segment.
    Line { to: Point },
    /// Adds a cubic Bézier segment.
    Curve {
        control_1: Point,
        control_2: Point,
        to: Point,
    },
    /// Closes the current contour.
    Close,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// Identity transform.
    pub const IDENTITY: Self = Self {
        a: 1_000_000,
        b: 0,
        c: 0,
        d: 1_000_000,
        tx: Unit::ZERO,
        ty: Unit::ZERO,
    };

    /// Creates a checked page-space translation.
    #[must_use]
    pub const fn translation(tx: Unit, ty: Unit) -> Self {
        Self {
            tx,
            ty,
            ..Self::IDENTITY
        }
    }

    /// Creates a fixed-point scale. Negative coefficients mirror an axis.
    pub fn scale(x: i64, y: i64) -> Result<Self> {
        if x == 0 || y == 0 {
            return Err(invalid_geometry("transform scale cannot collapse an axis"));
        }
        Ok(Self {
            a: x,
            b: 0,
            c: 0,
            d: y,
            tx: Unit::ZERO,
            ty: Unit::ZERO,
        })
    }

    /// Creates an integer-degree rotation quantized to millionth coefficients.
    pub fn rotation_degrees(degrees: i32) -> Result<Self> {
        let (sin, cos) = fixed_sin_cos(degrees)?;
        Ok(Self {
            a: cos,
            b: sin,
            c: sin
                .checked_neg()
                .ok_or_else(|| invalid_geometry("rotation overflow"))?,
            d: cos,
            tx: Unit::ZERO,
            ty: Unit::ZERO,
        })
    }

    /// Applies `self`, then `next`, using checked fixed-point matrix composition.
    pub fn then(self, next: Self) -> Result<Self> {
        Ok(Self {
            a: matrix_term(next.a, self.a, next.c, self.b)?,
            b: matrix_term(next.b, self.a, next.d, self.b)?,
            c: matrix_term(next.a, self.c, next.c, self.d)?,
            d: matrix_term(next.b, self.c, next.d, self.d)?,
            tx: combine(self.tx, next.a, self.ty, next.c)?.checked_add(next.tx)?,
            ty: combine(self.tx, next.b, self.ty, next.d)?.checked_add(next.ty)?,
        })
    }

    /// Applies this linear transform around an explicit page-space origin.
    pub fn around(self, origin: Point) -> Result<Self> {
        Self::translation(
            Unit::from_raw(
                origin
                    .x
                    .raw()
                    .checked_neg()
                    .ok_or_else(|| invalid_geometry("transform origin negation overflow"))?,
            ),
            Unit::from_raw(
                origin
                    .y
                    .raw()
                    .checked_neg()
                    .ok_or_else(|| invalid_geometry("transform origin negation overflow"))?,
            ),
        )
        .then(self)?
        .then(Self::translation(origin.x, origin.y))
    }

    /// Whether the transform leaves page coordinates unchanged.
    #[must_use]
    pub const fn is_identity(self) -> bool {
        self.a == Self::IDENTITY.a
            && self.b == 0
            && self.c == 0
            && self.d == Self::IDENTITY.d
            && self.tx.raw() == 0
            && self.ty.raw() == 0
    }

    /// Maps a page-space displacement back through the linear matrix.
    pub fn inverse_vector(self, vector: Point) -> Result<Point> {
        let determinant =
            i128::from(self.a) * i128::from(self.d) - i128::from(self.b) * i128::from(self.c);
        if determinant == 0 {
            return Err(invalid_geometry("transform matrix is not invertible"));
        }
        let x = (i128::from(self.d) * i128::from(vector.x.raw())
            - i128::from(self.c) * i128::from(vector.y.raw()))
        .checked_mul(1_000_000)
        .ok_or_else(|| invalid_geometry("inverse transform overflow"))?;
        let y = (i128::from(self.a) * i128::from(vector.y.raw())
            - i128::from(self.b) * i128::from(vector.x.raw()))
        .checked_mul(1_000_000)
        .ok_or_else(|| invalid_geometry("inverse transform overflow"))?;
        Ok(Point {
            x: Unit::from_raw(divide_round_i128(x, determinant)?),
            y: Unit::from_raw(divide_round_i128(y, determinant)?),
        })
    }

    /// Transforms a point with checked fixed-point arithmetic.
    pub fn apply(self, point: Point) -> Result<Point> {
        let x = combine(point.x, self.a, point.y, self.c)?.checked_add(self.tx)?;
        let y = combine(point.x, self.b, point.y, self.d)?.checked_add(self.ty)?;
        Ok(Point { x, y })
    }

    /// Returns the axis-aligned bounds of a transformed rectangle.
    pub fn bounds(self, rect: Rect) -> Result<Rect> {
        let right = rect.right()?;
        let bottom = rect.bottom()?;
        let points = [
            self.apply(rect.origin)?,
            self.apply(Point {
                x: right,
                y: rect.origin.y,
            })?,
            self.apply(Point {
                x: rect.origin.x,
                y: bottom,
            })?,
            self.apply(Point {
                x: right,
                y: bottom,
            })?,
        ];
        let min_x = points
            .iter()
            .map(|point| point.x)
            .min()
            .unwrap_or(Unit::ZERO);
        let max_x = points
            .iter()
            .map(|point| point.x)
            .max()
            .unwrap_or(Unit::ZERO);
        let min_y = points
            .iter()
            .map(|point| point.y)
            .min()
            .unwrap_or(Unit::ZERO);
        let max_y = points
            .iter()
            .map(|point| point.y)
            .max()
            .unwrap_or(Unit::ZERO);
        Rect::new(
            min_x,
            min_y,
            max_x.checked_sub(min_x)?,
            max_y.checked_sub(min_y)?,
        )
    }
}

/// Geometry used for collision and vector output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    /// Axis-aligned rectangle.
    Rect {
        /// Rectangle bounds.
        bounds: Rect,
    },
    /// Ellipse contained in bounds.
    Ellipse {
        /// Ellipse bounds.
        bounds: Rect,
    },
    /// Closed polygon.
    Polygon {
        /// Polygon vertices.
        points: Vec<Point>,
    },
    /// Simple polyline/path bounds retained with commands elsewhere.
    Path {
        /// Conservative collision bounds.
        bounds: Rect,
        /// Resolved commands preserved for vector and raster exporters.
        commands: Vec<PathCommand>,
    },
}

impl Shape {
    /// Returns conservative axis-aligned collision bounds.
    pub fn bounds(&self) -> Result<Rect> {
        match self {
            Self::Rect { bounds } | Self::Ellipse { bounds } | Self::Path { bounds, .. } => {
                Ok(*bounds)
            }
            Self::Polygon { points } => polygon_bounds(points),
        }
    }
}

/// Distinct geometry bounds retained after measurement and layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BoundsSet {
    /// Content's natural measurement.
    pub intrinsic: Rect,
    /// Box participating in layout.
    pub layout: Rect,
    /// Geometry used by collision policies.
    pub collision: Rect,
    /// Geometry actually painted.
    pub visual: Rect,
    /// Optional clipping rectangle.
    pub clip: Option<Rect>,
}

fn combine(first: Unit, first_scale: i64, second: Unit, second_scale: i64) -> Result<Unit> {
    let left = i128::from(first.raw()) * i128::from(first_scale);
    let right = i128::from(second.raw()) * i128::from(second_scale);
    let raw = (left + right) / 1_000_000;
    i64::try_from(raw)
        .map(Unit::from_raw)
        .map_err(|_| invalid_geometry("transform overflow"))
}

fn divide_round_i128(numerator: i128, denominator: i128) -> Result<i64> {
    let (numerator, denominator) = if denominator < 0 {
        (
            numerator
                .checked_neg()
                .ok_or_else(|| invalid_geometry("inverse transform overflow"))?,
            denominator
                .checked_neg()
                .ok_or_else(|| invalid_geometry("inverse transform overflow"))?,
        )
    } else {
        (numerator, denominator)
    };
    let adjustment = denominator / 2;
    let adjusted = if numerator >= 0 {
        numerator.checked_add(adjustment)
    } else {
        numerator.checked_sub(adjustment)
    }
    .ok_or_else(|| invalid_geometry("inverse transform rounding overflow"))?;
    i64::try_from(adjusted / denominator)
        .map_err(|_| invalid_geometry("inverse transform is outside the supported range"))
}

fn polygon_bounds(points: &[Point]) -> Result<Rect> {
    let first = points
        .first()
        .ok_or_else(|| invalid_geometry("polygon requires at least one point"))?;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first.x, first.x, first.y, first.y);
    for point in &points[1..] {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    Rect::new(
        min_x,
        min_y,
        max_x.checked_sub(min_x)?,
        max_y.checked_sub(min_y)?,
    )
}

fn invalid_geometry(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::GeometryInvalid, message)
}
