// =============================================================================
//        #######
//     ###       ###     F: layout_geometry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded layout geometry contracts and behavior for this crate.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Alignment, CollisionBounds, ElementIr, ElementKind, ErrorCode, FileMakerError, LayoutMode,
    PathCommand, PathCommandIr, Point, Rect, Result, Shape, Size, Transform, Unit,
};

pub(crate) fn propose_rect(
    element: &ElementIr,
    container: Rect,
    parent_layout: LayoutMode,
    flow: Point,
    positions: &BTreeMap<String, (usize, Rect)>,
    guides: &BTreeMap<String, crate::Length>,
    logical_unit: Unit,
) -> Result<Rect> {
    let default_height = element
        .style
        .font_size
        .unwrap_or(Unit::points(12)?)
        .checked_scale(1_200_000)?;
    let size = crate::constraints::resolve_constrained_size(
        element.geometry.width,
        element.geometry.height,
        element.geometry.constraints,
        container.size,
        Size::new(container.size.width, default_height)?,
        logical_unit,
    )?;
    let explicit_x = resolve_dimension(element.geometry.x, container.size.width, logical_unit)?;
    let explicit_y = resolve_dimension(element.geometry.y, container.size.height, logical_unit)?;
    let x = match element.geometry.align_x {
        Some(Alignment::Center) => container.origin.x.checked_add(Unit::from_raw(
            (container.size.width.raw() - size.width.raw()) / 2,
        ))?,
        Some(Alignment::End) => container.right()?.checked_sub(size.width)?,
        Some(Alignment::Start) => container.origin.x,
        None if parent_layout == LayoutMode::FlowHorizontal => explicit_x.unwrap_or(flow.x),
        None => container
            .origin
            .x
            .checked_add(explicit_x.unwrap_or(Unit::ZERO))?,
    };
    let y = match element.geometry.align_y {
        Some(Alignment::Center) => container.origin.y.checked_add(Unit::from_raw(
            (container.size.height.raw() - size.height.raw()) / 2,
        ))?,
        Some(Alignment::End) => container.bottom()?.checked_sub(size.height)?,
        Some(Alignment::Start) => container.origin.y,
        None if parent_layout == LayoutMode::FlowVertical => explicit_y.unwrap_or(flow.y),
        None => container
            .origin
            .y
            .checked_add(explicit_y.unwrap_or(Unit::ZERO))?,
    };
    apply_anchors(
        element,
        Rect::new(x, y, size.width, size.height)?,
        positions,
        guides,
        container,
        logical_unit,
    )
}

pub(crate) fn select_collision_bounds(
    selected: CollisionBounds,
    layout: Rect,
    intrinsic: Rect,
    visual: Rect,
) -> Rect {
    match selected {
        CollisionBounds::Layout => layout,
        CollisionBounds::Visual => visual,
        CollisionBounds::Intrinsic => intrinsic,
    }
}

pub(crate) fn resolve_layout_rect(
    selected: CollisionBounds,
    proposed_layout: Rect,
    proposed_collision: Rect,
    resolved_collision: Rect,
    parent_transform: Transform,
) -> Result<Rect> {
    if selected != CollisionBounds::Layout && proposed_collision.size != resolved_collision.size {
        return Err(layout_error(
            "non-layout collision bounds cannot resize the layout box",
        ));
    }
    let page_delta_x = resolved_collision
        .origin
        .x
        .checked_sub(proposed_collision.origin.x)?;
    let page_delta_y = resolved_collision
        .origin
        .y
        .checked_sub(proposed_collision.origin.y)?;
    let local_delta = parent_transform.inverse_vector(Point {
        x: page_delta_x,
        y: page_delta_y,
    })?;
    let width = if selected == CollisionBounds::Layout {
        proposed_layout.size.width.checked_add(
            resolved_collision
                .size
                .width
                .checked_sub(proposed_collision.size.width)?,
        )?
    } else {
        proposed_layout.size.width
    };
    let height = if selected == CollisionBounds::Layout {
        proposed_layout.size.height.checked_add(
            resolved_collision
                .size
                .height
                .checked_sub(proposed_collision.size.height)?,
        )?
    } else {
        proposed_layout.size.height
    };
    Rect::new(
        proposed_layout.origin.x.checked_add(local_delta.x)?,
        proposed_layout.origin.y.checked_add(local_delta.y)?,
        width,
        height,
    )
}

pub(crate) fn resolve_transform(
    element: &ElementIr,
    bounds: Rect,
    logical_unit: Unit,
) -> Result<Transform> {
    let intent = element.transform;
    let tx = intent
        .translate_x
        .resolve(bounds.size.width, logical_unit)?
        .ok_or_else(|| layout_error("transform translation cannot be auto"))?;
    let ty = intent
        .translate_y
        .resolve(bounds.size.height, logical_unit)?
        .ok_or_else(|| layout_error("transform translation cannot be auto"))?;
    let origin_x = intent
        .origin_x
        .resolve(bounds.size.width, logical_unit)?
        .ok_or_else(|| layout_error("transform origin cannot be auto"))?;
    let origin_y = intent
        .origin_y
        .resolve(bounds.size.height, logical_unit)?
        .ok_or_else(|| layout_error("transform origin cannot be auto"))?;
    let origin = Point {
        x: bounds.origin.x.checked_add(origin_x)?,
        y: bounds.origin.y.checked_add(origin_y)?,
    };
    Transform::scale(intent.scale_x, intent.scale_y)?
        .then(Transform::rotation_degrees(intent.rotate)?)?
        .around(origin)?
        .then(Transform::translation(tx, ty))
}

pub(crate) fn resolve_dimension(
    length: Option<crate::Length>,
    percent_base: Unit,
    logical_unit: Unit,
) -> Result<Option<Unit>> {
    length.map_or(Ok(None), |value| value.resolve(percent_base, logical_unit))
}

pub(crate) fn apply_anchors(
    element: &ElementIr,
    mut bounds: Rect,
    positions: &BTreeMap<String, (usize, Rect)>,
    guides: &BTreeMap<String, crate::Length>,
    container: Rect,
    logical_unit: Unit,
) -> Result<Rect> {
    for (edge, expression) in &element.geometry.anchors {
        if let Some(expression) = expression.strip_prefix("guide:") {
            let (name, offset) = parse_guide(expression, logical_unit)?;
            let base = if matches!(edge.as_str(), "left" | "right") {
                container.size.width
            } else {
                container.size.height
            };
            let value = guides
                .get(name)
                .ok_or_else(|| layout_error(format!("guide `{name}` was not found")))?
                .resolve(base, logical_unit)?
                .ok_or_else(|| layout_error("guide cannot be auto"))?
                .checked_add(offset)?;
            apply_anchor_value(edge, value, &mut bounds)?;
            continue;
        }
        let (reference, reference_edge, offset) = parse_anchor(expression, logical_unit)?;
        let (_, target) = positions.get(reference).ok_or_else(|| {
            layout_error(format!(
                "anchor target `{reference}` is unresolved or cyclic"
            ))
        })?;
        let value = match reference_edge {
            "left" => target.origin.x,
            "right" => target.right()?,
            "top" => target.origin.y,
            "bottom" => target.bottom()?,
            _ => return Err(layout_error("anchor target edge is invalid")),
        }
        .checked_add(offset)?;
        apply_anchor_value(edge, value, &mut bounds)?;
    }
    Ok(bounds)
}

fn apply_anchor_value(edge: &str, value: Unit, bounds: &mut Rect) -> Result<()> {
    match edge {
        "left" => bounds.origin.x = value,
        "top" => bounds.origin.y = value,
        "right" => bounds.origin.x = value.checked_sub(bounds.size.width)?,
        "bottom" => bounds.origin.y = value.checked_sub(bounds.size.height)?,
        _ => return Err(layout_error("anchor edge is invalid")),
    }
    Ok(())
}

fn parse_guide(expression: &str, logical_unit: Unit) -> Result<(&str, Unit)> {
    let (name, offset) = if let Some(index) = expression.rfind('+') {
        (&expression[..index], &expression[index + 1..])
    } else {
        (expression, "0pt")
    };
    if name.is_empty() {
        return Err(layout_error("guide anchor requires `guide:name[+offset]`"));
    }
    let offset = offset
        .parse::<crate::Length>()?
        .resolve(Unit::ZERO, logical_unit)?
        .ok_or_else(|| layout_error("guide offset cannot be auto"))?;
    Ok((name, offset))
}

fn parse_anchor(expression: &str, logical_unit: Unit) -> Result<(&str, &str, Unit)> {
    let (base, offset) = if let Some(index) = expression.rfind('+') {
        (&expression[..index], &expression[index + 1..])
    } else {
        (expression, "0pt")
    };
    let (reference, edge) = base
        .rsplit_once('.')
        .ok_or_else(|| layout_error("anchor requires `element.edge[+offset]`"))?;
    let offset = offset
        .parse::<crate::Length>()?
        .resolve(Unit::ZERO, logical_unit)?
        .ok_or_else(|| layout_error("anchor offset cannot be auto"))?;
    Ok((reference, edge, offset))
}

pub(crate) fn validate_anchor_graph(elements: &[ElementIr]) -> Result<()> {
    let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut stack: Vec<&ElementIr> = elements.iter().collect();
    while let Some(element) = stack.pop() {
        let dependencies = element
            .geometry
            .anchors
            .values()
            .filter(|value| !value.starts_with("guide:"))
            .filter_map(|value| value.split(['.', '+']).next())
            .collect();
        edges.insert(element.id.as_str(), dependencies);
        stack.extend(&element.children);
    }
    for node in edges.keys() {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        visit_anchor(node, &edges, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_anchor<'a>(
    node: &'a str,
    edges: &BTreeMap<&'a str, Vec<&'a str>>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> Result<()> {
    if visited.contains(node) {
        return Ok(());
    }
    if !visiting.insert(node) {
        return Err(FileMakerError::new(
            ErrorCode::LayoutNonConvergent,
            format!("anchor cycle includes `{node}`"),
        ));
    }
    if let Some(dependencies) = edges.get(node) {
        for dependency in dependencies {
            if edges.contains_key(dependency) {
                visit_anchor(dependency, edges, visiting, visited)?;
            }
        }
    }
    visiting.remove(node);
    visited.insert(node);
    Ok(())
}

pub(crate) fn visual_bounds(layout: Rect, stroke_width: Unit) -> Result<Rect> {
    let half = Unit::from_raw(stroke_width.raw() / 2);
    Rect::new(
        layout.origin.x.checked_sub(half)?,
        layout.origin.y.checked_sub(half)?,
        layout.size.width.checked_add(stroke_width)?,
        layout.size.height.checked_add(stroke_width)?,
    )
}

pub(crate) fn shape_for(element: &ElementIr, bounds: Rect, logical_unit: Unit) -> Result<Shape> {
    Ok(match element.kind {
        ElementKind::Circle => {
            if bounds.size.width != bounds.size.height {
                return Err(
                    layout_error("circle requires equal resolved width and height")
                        .at(element.id.as_str()),
                );
            }
            Shape::Ellipse { bounds }
        }
        ElementKind::Ellipse => Shape::Ellipse { bounds },
        ElementKind::Path | ElementKind::Line => Shape::Path {
            bounds,
            commands: resolve_path(&element.path, bounds, logical_unit)?,
        },
        ElementKind::Polygon => Shape::Polygon {
            points: polygon_points(&element.path, bounds, logical_unit)?,
        },
        _ => Shape::Rect { bounds },
    })
}

fn resolve_path(
    commands: &[PathCommandIr],
    bounds: Rect,
    logical_unit: Unit,
) -> Result<Vec<PathCommand>> {
    if commands.is_empty() {
        return Ok(vec![
            PathCommand::Move { to: bounds.origin },
            PathCommand::Line {
                to: Point {
                    x: bounds.right()?,
                    y: bounds.bottom()?,
                },
            },
        ]);
    }
    commands
        .iter()
        .map(|command| match command {
            PathCommandIr::Move { x, y } => Ok(PathCommand::Move {
                to: resolve_path_point(*x, *y, bounds, logical_unit)?,
            }),
            PathCommandIr::Line { x, y } => Ok(PathCommand::Line {
                to: resolve_path_point(*x, *y, bounds, logical_unit)?,
            }),
            PathCommandIr::Curve {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => Ok(PathCommand::Curve {
                control_1: resolve_path_point(*x1, *y1, bounds, logical_unit)?,
                control_2: resolve_path_point(*x2, *y2, bounds, logical_unit)?,
                to: resolve_path_point(*x, *y, bounds, logical_unit)?,
            }),
            PathCommandIr::Close => Ok(PathCommand::Close),
        })
        .collect()
}

fn polygon_points(
    commands: &[PathCommandIr],
    bounds: Rect,
    logical_unit: Unit,
) -> Result<Vec<Point>> {
    let mut points = Vec::new();
    for command in resolve_path(commands, bounds, logical_unit)? {
        match command {
            PathCommand::Move { to } | PathCommand::Line { to } => points.push(to),
            PathCommand::Curve { .. } => {
                return Err(layout_error("polygon does not accept curve commands"));
            }
            PathCommand::Close => {}
        }
    }
    if points.len() < 3 {
        return Err(layout_error("polygon requires at least three vertices"));
    }
    Ok(points)
}

fn resolve_path_point(
    x: crate::Length,
    y: crate::Length,
    bounds: Rect,
    logical_unit: Unit,
) -> Result<Point> {
    let x = x
        .resolve(bounds.size.width, logical_unit)?
        .ok_or_else(|| layout_error("path x coordinate cannot be auto"))?;
    let y = y
        .resolve(bounds.size.height, logical_unit)?
        .ok_or_else(|| layout_error("path y coordinate cannot be auto"))?;
    Ok(Point {
        x: bounds.origin.x.checked_add(x)?,
        y: bounds.origin.y.checked_add(y)?,
    })
}

pub(crate) fn non_convergent(element: &ElementIr, message: &str) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutNonConvergent, message).at(element.id.as_str())
}

pub(crate) fn layout_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}
