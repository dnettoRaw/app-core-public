// =============================================================================
//        #######
//     ###       ###     F: svg_path.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Writes bounded SVG path command data.

use super::bounded_string::{output_limit_error, FormattedOutput};
use super::markup::points;
use crate::{PathCommand, Point, Result, Unit};

pub(super) fn write_path_data(
    output: &mut dyn FormattedOutput,
    commands: &[PathCommand],
    origin: Option<Point>,
) -> Result<()> {
    let origin = origin.unwrap_or(Point {
        x: Unit::ZERO,
        y: Unit::ZERO,
    });
    for (index, command) in commands.iter().enumerate() {
        if index > 0 {
            output.push_str(" ")?;
        }
        match command {
            PathCommand::Move { to } => write_coordinate(output, "M", *to, origin)?,
            PathCommand::Line { to } => write_coordinate(output, "L", *to, origin)?,
            PathCommand::Curve {
                control_1,
                control_2,
                to,
            } => {
                write_coordinate(output, "C", *control_1, origin)?;
                write_coordinate(output, "", *control_2, origin)?;
                write_coordinate(output, "", *to, origin)?;
            }
            PathCommand::Close => output.push_str("Z")?,
        }
    }
    Ok(())
}

fn write_coordinate(
    output: &mut dyn FormattedOutput,
    command: &str,
    point: Point,
    origin: Point,
) -> Result<()> {
    if command.is_empty() {
        output.push_str(" ")?;
    } else {
        write!(output, "{command} ").map_err(format_error)?;
    }
    write!(
        output,
        "{} {}",
        points(point.x.checked_sub(origin.x)?),
        points(point.y.checked_sub(origin.y)?)
    )
    .map_err(format_error)
}

fn format_error(_: std::fmt::Error) -> crate::FileMakerError {
    output_limit_error()
}
