// =============================================================================
//        #######
//     ###       ###     F: layout_adaptive.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Exact final-group reservation without guessed global page-count iterations.
use crate::layout_geometry::layout_error;
use crate::{
    AdaptiveLayoutPlan, DocumentIr, LayoutEngine, OperationControl, Rect, ResolvedScene, Result,
    TablePageBody, Unit,
};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct AdaptiveRuntime {
    pub(crate) plan: AdaptiveLayoutPlan,
    pub(crate) diagnostics: Option<crate::pagination_diagnostics::DecisionRecorder>,
    pub(crate) closing_height: Unit,
    pub(crate) closing_ids: Vec<crate::ElementId>,
    pub(crate) reserve: Unit,
    pub(crate) control: OperationControl,
}
impl LayoutEngine<'_> {
    /// Resolves first/intermediate/last/single table regions and a closing block.
    ///
    /// The separate strict YAML plan is opt-in; V1 templates, `resolve`, and
    /// public struct literals remain unchanged. Uses intrinsic measurement,
    /// existing row grouping and one final-group lookahead. Only the final
    /// row carries the closing reserve; moving it re-evaluates that reserve
    /// against its destination page. No guessed page-count fixed point occurs.
    ///
    /// This bounded contract supports one root table starting on page zero,
    /// optionally starting the first/single body after an earlier measured
    /// element with `first_after`, preserving the declared body bottom;
    /// followed by one final body block anchored to its bottom plus the plan's
    /// gap. Width stays fixed. Last/single effective capacities must not exceed
    /// intermediate/first capacities. Existing `page_bodies`, row reserves,
    /// built-in table totals, table transforms and other body tables are
    /// rejected explicitly. Totals may be data-bound in the closing block.
    /// Oversized groups follow the required `error` or `split_rows` policy;
    /// an indivisible row plus reserve that cannot fit returns an error.
    ///
    /// Hidden closing blocks reserve nothing. Normal page bands keep their V1
    /// semantics, including `last` being inactive on a one-page document.
    /// Perform layout preflight before export. Use separate scene cache keys
    /// for each plan and layout mode; no cached adaptive entrypoint is implied.
    pub fn resolve_adaptive(
        &self,
        document: &DocumentIr,
        plan: &AdaptiveLayoutPlan,
    ) -> Result<ResolvedScene> {
        self.resolve_adaptive_recorded(document, plan, None)
    }

    /// Resolves adaptive layout with bounded, numeric pagination evidence.
    ///
    /// `max_decisions` must be in `1..=limits.max_elements`. Exhaustion returns
    /// `LimitExceeded` in the report, retaining earlier decisions. Failure before
    /// pagination can produce an empty record list. Each call owns an independent
    /// recorder; ordinary resolution allocates no recorder. This does not replace
    /// scene audit, preflight or visual review.
    pub fn diagnose_adaptive(
        &self,
        document: &DocumentIr,
        plan: &AdaptiveLayoutPlan,
        max_decisions: usize,
    ) -> crate::AdaptiveLayoutReport {
        let recorder = crate::pagination_diagnostics::DecisionRecorder::new(max_decisions);
        let mut outcome = if max_decisions == 0 || max_decisions > self.limits.max_elements {
            Err(crate::pagination_diagnostics::diagnostic_error(
                "invalid pagination decision limit",
            ))
        } else {
            self.resolve_adaptive_recorded(document, plan, Some(recorder.clone()))
        };
        let decisions = match recorder.snapshot() {
            Ok(entries) => entries,
            Err(error) => {
                outcome = Err(error);
                Vec::new()
            }
        };
        crate::AdaptiveLayoutReport { outcome, decisions }
    }

    fn resolve_adaptive_recorded(
        &self,
        document: &DocumentIr,
        plan: &AdaptiveLayoutPlan,
        diagnostics: Option<crate::pagination_diagnostics::DecisionRecorder>,
    ) -> Result<ResolvedScene> {
        let mut prepared = self.prepare_intrinsic(document)?;
        let mut runtime = crate::layout_adaptive_validation::prepare(self, &mut prepared, plan)?;
        runtime.diagnostics = diagnostics;
        let mut engine = LayoutEngine::new_controlled(
            self.limits,
            self.fonts,
            self.options.clone(),
            self.control.clone(),
        )?;
        engine.assets = self.assets;
        engine.adaptive = Some(Arc::new(runtime));
        let scene = engine.resolve(&prepared)?;
        crate::layout_adaptive_validation::verify(
            &scene,
            engine
                .adaptive
                .as_deref()
                .ok_or_else(|| layout_error("adaptive plan is missing"))?,
        )?;
        Ok(scene)
    }
}
impl AdaptiveRuntime {
    pub(crate) fn after_first_anchor(
        &self,
        bounds: Rect,
        positions: &std::collections::BTreeMap<String, (usize, Rect)>,
        logical: Unit,
    ) -> Result<Self> {
        let mut resolved = self.clone();
        if let Some(anchor) = &self.plan.first_after {
            let (page, target) = positions.get(anchor.element.as_str()).ok_or_else(|| {
                layout_error("first_after requires an earlier measured element")
                    .at(anchor.element.as_str())
            })?;
            if *page != 0 {
                return Err(layout_error(
                    "first_after target must be on the first physical page",
                ));
            }
            let gap = match anchor.gap {
                crate::Length::Absolute(v) => v,
                crate::Length::Logical(v) => logical.checked_scale(v)?,
                _ => return Err(layout_error("first_after gap must be absolute or logical")),
            };
            if gap < Unit::ZERO {
                return Err(layout_error("first_after gap cannot be negative"));
            }
            let top = target.bottom()?.checked_add(gap)?;
            for body in [
                &mut resolved.plan.bodies.first,
                &mut resolved.plan.bodies.single,
            ] {
                let region = resolve_body(body, bounds, logical)?;
                let start = region.origin.y.max(top);
                let height = region.bottom()?.checked_sub(start)?;
                if height <= Unit::ZERO {
                    return Err(layout_error(
                        "first_after leaves no usable first-page table body",
                    ));
                }
                body.offset_y = crate::Length::Absolute(start.checked_sub(bounds.origin.y)?);
                body.height = crate::Length::Absolute(height);
            }
        }
        Ok(resolved)
    }

    pub(crate) fn body(
        &self,
        bounds: Rect,
        page: usize,
        final_page: bool,
        logical: Unit,
    ) -> Result<Rect> {
        let bodies = &self.plan.bodies;
        let body = match (page == 0, final_page) {
            (true, true) => &bodies.single,
            (true, false) => &bodies.first,
            (false, true) => &bodies.last,
            (false, false) => &bodies.intermediate,
        };
        let mut resolved = resolve_body(body, bounds, logical)?;
        if final_page {
            resolved.size.height = resolved.size.height.checked_sub(self.reserve)?;
        }
        if resolved.size.height <= Unit::ZERO {
            return Err(layout_error("closing reserve leaves no final table body")
                .at(self.plan.table.as_str()));
        }
        Ok(resolved)
    }

    pub(crate) fn tail(
        &self,
        bounds: Rect,
        logical: Unit,
    ) -> Result<crate::table_tail::TailReservation> {
        let first = self.body(bounds, 0, false, logical)?.size.height;
        let middle = self.body(bounds, 1, false, logical)?.size.height;
        let single = self.body(bounds, 0, true, logical)?.size.height;
        let last = self.body(bounds, 1, true, logical)?.size.height;
        if single > first || last > middle {
            return Err(layout_error(
                "single/last effective body height cannot exceed first/intermediate height",
            )
            .at(self.plan.table.as_str()));
        }
        Ok(crate::table_tail::TailReservation {
            first: first.checked_sub(single)?,
            diagnostics: self.diagnostics.clone(),
            table: self.plan.table.clone(),
            header_height: Unit::ZERO,
            repeat_header: false,
            continuation: middle.checked_sub(last)?,
            oversized: self.plan.oversized_group,
            control: self.control.clone(),
        })
    }
}
fn resolve_body(body: &TablePageBody, bounds: Rect, logical: Unit) -> Result<Rect> {
    let offset = body
        .offset_y
        .resolve(bounds.size.height, logical)?
        .ok_or_else(|| layout_error("adaptive body offset cannot be auto"))?;
    let height = body
        .height
        .resolve(bounds.size.height, logical)?
        .ok_or_else(|| layout_error("adaptive body height cannot be auto"))?;
    if offset < Unit::ZERO
        || height <= Unit::ZERO
        || offset.checked_add(height)? > bounds.size.height
    {
        return Err(layout_error("adaptive body must fit inside table bounds"));
    }
    Rect::new(
        bounds.origin.x,
        bounds.origin.y.checked_add(offset)?,
        bounds.size.width,
        height,
    )
}

impl LayoutEngine<'_> {
    pub(crate) fn table_fragment_body(
        &self,
        element: &crate::ElementIr,
        bounds: Rect,
        index: usize,
        final_page: bool,
        adaptive: Option<&AdaptiveRuntime>,
    ) -> Result<Rect> {
        if let Some(adaptive) = adaptive {
            return adaptive.body(bounds, index, final_page, self.options.logical_unit);
        }
        let table = element
            .table
            .as_ref()
            .ok_or_else(|| layout_error("table intent is missing"))?;
        crate::layout_table::page_body_bounds(table, bounds, index, self.options.logical_unit)
    }
}
