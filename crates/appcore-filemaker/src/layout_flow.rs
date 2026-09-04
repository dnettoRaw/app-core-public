// =============================================================================
//        #######
//     ###       ###     F: layout_flow.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded layout flow contracts and behavior for this crate.

use crate::{
    Distribution, ElementIr, ErrorCode, FileMakerError, LayoutMode, Rect, Result, Size, Unit,
};

pub(crate) struct FlowPlan {
    pub(crate) x: Unit,
    pub(crate) y: Unit,
    pub(crate) gap: Unit,
}

#[derive(Clone, Copy)]
enum FlowAxis {
    Vertical,
    Horizontal,
}

pub(crate) fn plan_flow(
    elements: &[ElementIr],
    container: Rect,
    mode: LayoutMode,
    distribution: Distribution,
    base_gap: Unit,
    logical_unit: Unit,
) -> Result<FlowPlan> {
    let mut plan = FlowPlan {
        x: container.origin.x,
        y: container.origin.y,
        gap: base_gap,
    };
    let axis = match mode {
        LayoutMode::Absolute => return Ok(plan),
        LayoutMode::FlowVertical => FlowAxis::Vertical,
        LayoutMode::FlowHorizontal => FlowAxis::Horizontal,
    };
    if distribution == Distribution::Start {
        return Ok(plan);
    }
    let visible_count = elements.iter().filter(|element| !element.hidden).count();
    if visible_count == 0 {
        return Ok(plan);
    }
    let mut primary_total = Unit::ZERO;
    for element in elements.iter().filter(|element| !element.hidden) {
        if !has_primary_intent(element, axis) {
            return Err(flow_error(
                "distributed flow children require explicit or preferred primary size",
            ));
        }
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
        primary_total = primary_total.checked_add(match axis {
            FlowAxis::Vertical => size.height,
            FlowAxis::Horizontal => size.width,
        })?;
    }
    let intervals = visible_count.saturating_sub(1);
    let interval_count = i64::try_from(intervals)
        .map_err(|_| flow_error("flow child count exceeds integer range"))?;
    let declared_gaps = base_gap.checked_scale(
        interval_count
            .checked_mul(1_000_000)
            .ok_or_else(|| flow_error("flow gap count overflow"))?,
    )?;
    let used = primary_total.checked_add(declared_gaps)?;
    let available = match axis {
        FlowAxis::Vertical => container.size.height,
        FlowAxis::Horizontal => container.size.width,
    };
    let remaining = available.checked_sub(used)?;
    if remaining < Unit::ZERO {
        return Err(flow_error("distributed flow exceeds its container"));
    }
    let (offset, extra_gap) = distribution_spacing(distribution, remaining, visible_count)?;
    plan.gap = base_gap.checked_add(extra_gap)?;
    match axis {
        FlowAxis::Vertical => plan.y = plan.y.checked_add(offset)?,
        FlowAxis::Horizontal => plan.x = plan.x.checked_add(offset)?,
    }
    Ok(plan)
}

fn has_primary_intent(element: &ElementIr, axis: FlowAxis) -> bool {
    match axis {
        FlowAxis::Vertical => {
            element.geometry.height.is_some()
                || element.geometry.constraints.preferred_height.is_some()
                || element.geometry.constraints.aspect_ratio.is_some()
                    && (element.geometry.width.is_some()
                        || element.geometry.constraints.preferred_width.is_some())
        }
        FlowAxis::Horizontal => {
            element.geometry.width.is_some()
                || element.geometry.constraints.preferred_width.is_some()
                || element.geometry.constraints.aspect_ratio.is_some()
                    && (element.geometry.height.is_some()
                        || element.geometry.constraints.preferred_height.is_some())
        }
    }
}

fn distribution_spacing(
    distribution: Distribution,
    remaining: Unit,
    count: usize,
) -> Result<(Unit, Unit)> {
    let divide = |parts: usize| {
        let denominator = i128::try_from(parts)
            .map_err(|_| flow_error("flow divisor overflow"))?
            .checked_mul(i128::from(Unit::PER_POINT))
            .ok_or_else(|| flow_error("flow divisor overflow"))?;
        Unit::from_ratio(i128::from(remaining.raw()), denominator)
    };
    match distribution {
        Distribution::Start => Ok((Unit::ZERO, Unit::ZERO)),
        Distribution::Center => Ok((divide(2)?, Unit::ZERO)),
        Distribution::End => Ok((remaining, Unit::ZERO)),
        Distribution::SpaceBetween if count > 1 => Ok((Unit::ZERO, divide(count - 1)?)),
        Distribution::SpaceBetween => Ok((Unit::ZERO, Unit::ZERO)),
        Distribution::SpaceAround => {
            let gap = divide(count)?;
            Ok((Unit::from_raw(gap.raw() / 2), gap))
        }
        Distribution::SpaceEvenly => {
            let slots = count
                .checked_add(1)
                .ok_or_else(|| flow_error("flow slot count overflow"))?;
            let gap = divide(slots)?;
            Ok((gap, gap))
        }
    }
}

fn flow_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_distribution_has_fixed_point_spacing() {
        let remaining = Unit::points(90).unwrap();
        assert_eq!(
            distribution_spacing(Distribution::Center, remaining, 2).unwrap(),
            (Unit::points(45).unwrap(), Unit::ZERO)
        );
        assert_eq!(
            distribution_spacing(Distribution::End, remaining, 2).unwrap(),
            (remaining, Unit::ZERO)
        );
        assert_eq!(
            distribution_spacing(Distribution::SpaceBetween, remaining, 2).unwrap(),
            (Unit::ZERO, remaining)
        );
        assert_eq!(
            distribution_spacing(Distribution::SpaceAround, remaining, 2).unwrap(),
            (Unit::from_raw(22_500_000), Unit::points(45).unwrap())
        );
        assert_eq!(
            distribution_spacing(Distribution::SpaceEvenly, remaining, 2).unwrap(),
            (Unit::points(30).unwrap(), Unit::points(30).unwrap())
        );
    }
}
