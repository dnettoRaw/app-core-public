// =============================================================================
//        #######
//     ###       ###     F: transform_math.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{ErrorCode, FileMakerError, Result};

pub(crate) fn matrix_term(left_a: i64, right_a: i64, left_b: i64, right_b: i64) -> Result<i64> {
    let value = i128::from(left_a)
        .checked_mul(i128::from(right_a))
        .and_then(|first| {
            i128::from(left_b)
                .checked_mul(i128::from(right_b))
                .and_then(|second| first.checked_add(second))
        })
        .ok_or_else(|| transform_error("transform composition overflow"))?;
    i64::try_from(round_fixed(value, 1_000_000)?)
        .map_err(|_| transform_error("transform coefficient is outside the supported range"))
}

pub(crate) fn fixed_sin_cos(degrees: i32) -> Result<(i64, i64)> {
    let normalized = i64::from(degrees).rem_euclid(360);
    match normalized {
        0 => return Ok((0, 1_000_000)),
        90 => return Ok((1_000_000, 0)),
        180 => return Ok((0, -1_000_000)),
        270 => return Ok((-1_000_000, 0)),
        _ => {}
    }
    let mut angle = normalized * 1_000_000;
    let sign = if angle > 90_000_000 && angle < 270_000_000 {
        angle -= 180_000_000;
        -1_i64
    } else {
        if angle >= 270_000_000 {
            angle -= 360_000_000;
        }
        1_i64
    };
    let mut x = 607_252_935_i64;
    let mut y = 0_i64;
    let mut remaining = angle;
    for (shift, step) in CORDIC_DEGREES_MICRO.iter().enumerate() {
        let direction = if remaining >= 0 { 1_i64 } else { -1_i64 };
        let next_x = x
            .checked_sub(
                direction
                    .checked_mul(y >> shift)
                    .ok_or_else(|| transform_error("rotation iteration overflow"))?,
            )
            .ok_or_else(|| transform_error("rotation iteration overflow"))?;
        let next_y = y
            .checked_add(
                direction
                    .checked_mul(x >> shift)
                    .ok_or_else(|| transform_error("rotation iteration overflow"))?,
            )
            .ok_or_else(|| transform_error("rotation iteration overflow"))?;
        x = next_x;
        y = next_y;
        remaining = remaining
            .checked_sub(direction * step)
            .ok_or_else(|| transform_error("rotation angle overflow"))?;
    }
    let cos = i64::try_from(round_fixed(i128::from(x * sign), 1_000)?)
        .map_err(|_| transform_error("rotation coefficient overflow"))?;
    let sin = i64::try_from(round_fixed(i128::from(y * sign), 1_000)?)
        .map_err(|_| transform_error("rotation coefficient overflow"))?;
    Ok((sin, cos))
}

fn round_fixed(value: i128, divisor: i128) -> Result<i128> {
    let adjustment = divisor / 2;
    let adjusted = if value >= 0 {
        value.checked_add(adjustment)
    } else {
        value.checked_sub(adjustment)
    }
    .ok_or_else(|| transform_error("fixed-point rounding overflow"))?;
    Ok(adjusted / divisor)
}

fn transform_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::GeometryInvalid, message)
}

const CORDIC_DEGREES_MICRO: [i64; 27] = [
    45_000_000, 26_565_051, 14_036_243, 7_125_016, 3_576_334, 1_789_911, 895_174, 447_614, 223_811,
    111_906, 55_953, 27_976, 13_988, 6_994, 3_497, 1_749, 874, 437, 219, 109, 55, 27, 14, 7, 3, 2,
    1,
];
