// =============================================================================
//        #######
//     ###       ###     F: inspect.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded inspect contracts and behavior for this crate.

use serde::{Deserialize, Serialize};

use crate::{
    BoundsSet, ElementId, ErrorCode, FileMakerError, LayoutTrace, OperationControl, Provenance,
    Rect, ResolvedElement, ResolvedPage, ResolvedScene, ResourceLimits, Result, Unit,
};

/// Read-only element inspection response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ElementInspection {
    /// Page containing the element.
    pub page: usize,
    /// Stable ID.
    pub id: ElementId,
    /// Every distinct bounds class.
    pub bounds: BoundsSet,
    /// Whether the element participates in collision geometry.
    pub collidable: bool,
    /// Visual layer.
    pub layer: String,
    /// Visual z index.
    pub z_index: i32,
    /// Source and patch provenance.
    pub provenance: Provenance,
    /// Source geometry and resolved collision/reflow inputs.
    pub layout_trace: LayoutTrace,
    /// Table fragment index when this element is a paginated table.
    pub table_fragment: Option<usize>,
    /// Number of rows retained in this table fragment.
    pub table_rows: Option<usize>,
}

/// Read-only page summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PageInspection {
    /// Page index.
    pub page: usize,
    /// Semantic physical-page role.
    pub role: crate::PageRole,
    /// Stable names of non-painted exclusion geometry.
    pub exclusions: Vec<String>,
    /// Stable names of resolved layout regions.
    pub regions: Vec<String>,
    /// Resolved safe area, when page metadata exists.
    pub safe: Option<Rect>,
    /// Number of resolved elements.
    pub elements: usize,
    /// Union of layout bounds, when non-empty.
    pub occupied: Option<Rect>,
    /// Elements overflowing page bounds.
    pub overflow: Vec<ElementId>,
}

/// Human/tool-readable explanation of a resolved placement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LayoutExplanation {
    /// Stable ID.
    pub id: ElementId,
    /// Final page.
    pub page: usize,
    /// Final geometry.
    pub bounds: BoundsSet,
    /// Whether intrinsic measurement differs from layout.
    pub measured: bool,
    /// Number of runtime patches contributing to the result.
    pub patch_count: usize,
    /// Component expansion chain.
    pub components: Vec<String>,
    /// Logical source.
    pub source: String,
    /// Concise deterministic reasoning steps.
    pub decisions: Vec<String>,
    /// Structured source geometry and collision/reflow inputs.
    pub trace: LayoutTrace,
}

/// Immutable scene inspection facade.
pub struct SceneInspector<'a> {
    scene: &'a ResolvedScene,
}

impl<'a> SceneInspector<'a> {
    /// Creates a read-only inspector.
    #[must_use]
    pub const fn new(scene: &'a ResolvedScene) -> Self {
        Self { scene }
    }

    /// Finds an element by exact ID.
    pub fn inspect_element(&self, id: &ElementId) -> Result<ElementInspection> {
        let (page, element) = self.find(id)?;
        Ok(ElementInspection {
            page,
            id: element.id.clone(),
            bounds: element.bounds,
            collidable: element.collidable,
            layer: element.layer.clone(),
            z_index: element.z_index,
            provenance: element.provenance.clone(),
            layout_trace: element.layout_trace.clone(),
            table_fragment: element.table.as_ref().map(|table| table.index),
            table_rows: element.table.as_ref().map(|table| table.rows.len()),
        })
    }

    /// Summarizes a page and reports overflow.
    pub fn inspect_page(&self, page: usize) -> Result<PageInspection> {
        let page_ref = self.page(page)?;
        let page_bounds = Rect::new(
            Unit::ZERO,
            Unit::ZERO,
            page_ref.size.width,
            page_ref.size.height,
        )?;
        let mut occupied: Option<Rect> = None;
        let mut overflow = Vec::new();
        for element in &page_ref.elements {
            occupied = Some(match occupied {
                Some(bounds) => bounds.union(element.bounds.layout)?,
                None => element.bounds.layout,
            });
            if !contains_rect(page_bounds, element.bounds.visual)? {
                overflow.push(element.id.clone());
            }
        }
        Ok(PageInspection {
            page,
            role: page_ref.role,
            exclusions: page_ref
                .exclusions
                .iter()
                .map(|exclusion| exclusion.name.clone())
                .collect(),
            regions: page_ref
                .regions
                .iter()
                .map(|region| region.name.clone())
                .collect(),
            safe: page_ref
                .page_template
                .as_ref()
                .map(crate::PageTemplate::safe_bounds)
                .transpose()?,
            elements: page_ref.elements.len(),
            occupied,
            overflow,
        })
    }

    /// Explains final geometry, measurement, page assignment, and provenance.
    pub fn explain_layout(&self, id: &ElementId) -> Result<LayoutExplanation> {
        let (page, element) = self.find(id)?;
        let mut decisions = vec![format!(
            "layout=({}, {}, {}, {})",
            element.bounds.layout.origin.x.raw(),
            element.bounds.layout.origin.y.raw(),
            element.bounds.layout.size.width.raw(),
            element.bounds.layout.size.height.raw()
        )];
        if element.bounds.intrinsic != element.bounds.layout {
            decisions.push(format!(
                "measurement intrinsic=({}, {}, {}, {}) constrained by layout",
                element.bounds.intrinsic.origin.x.raw(),
                element.bounds.intrinsic.origin.y.raw(),
                element.bounds.intrinsic.size.width.raw(),
                element.bounds.intrinsic.size.height.raw()
            ));
        } else {
            decisions.push("measurement matched proposed layout".to_owned());
        }
        decisions.push(format!(
            "source x={:?} y={:?} width={:?} height={:?} region={:?}",
            element.layout_trace.geometry.x,
            element.layout_trace.geometry.y,
            element.layout_trace.geometry.width,
            element.layout_trace.geometry.height,
            element.layout_trace.geometry.region
        ));
        if !element.layout_trace.geometry.anchors.is_empty() {
            decisions.push(format!(
                "anchors={:?}",
                element.layout_trace.geometry.anchors
            ));
        }
        decisions.push(format!(
            "collision enabled={} bounds={:?} policy={:?} reflowed={}",
            element.layout_trace.collision_policy.enabled,
            element.layout_trace.collision_policy.bounds,
            element.layout_trace.collision_policy.resolution,
            element.layout_trace.reflowed
        ));
        if element.layout_trace.initial_page != page {
            decisions.push(format!(
                "page/reflow moved placement from page {} to page {}",
                element.layout_trace.initial_page + 1,
                page + 1
            ));
        } else {
            decisions.push(format!("page/reflow retained page {}", page + 1));
        }
        if let Some(table) = &element.table {
            decisions.push(format!(
                "table fragment {} contains {} rows",
                table.index,
                table.rows.len()
            ));
        }
        Ok(LayoutExplanation {
            id: element.id.clone(),
            page,
            bounds: element.bounds,
            measured: element.bounds.intrinsic != element.bounds.layout,
            patch_count: element.provenance.patches.len(),
            components: element.provenance.components.clone(),
            source: element.provenance.source.clone(),
            decisions,
            trace: element.layout_trace.clone(),
        })
    }

    /// Returns disjoint free rectangles after subtracting collision bounds and exclusions.
    pub fn query_free_regions(&self, page: usize, minimum: crate::Size) -> Result<Vec<Rect>> {
        self.query_free_regions_bounded(page, minimum, &ResourceLimits::default())
    }

    /// Returns free regions under the caller's diagnostic geometry budget.
    pub fn query_free_regions_bounded(
        &self,
        page: usize,
        minimum: crate::Size,
        limits: &ResourceLimits,
    ) -> Result<Vec<Rect>> {
        self.free_regions(page, minimum, limits, None)
    }

    /// Queries resolved collision geometry with cooperative cancellation.
    ///
    /// Reports completed rectangle subtractions in the `Preflight` phase.
    /// Scene validation and final filtering/sorting are checked around, not
    /// interrupted internally. Cancellation discards the partial result and
    /// never changes the scene. Observers must return promptly.
    pub fn query_free_regions_controlled(
        &self,
        page: usize,
        minimum: crate::Size,
        limits: &ResourceLimits,
        control: &OperationControl,
    ) -> Result<Vec<Rect>> {
        self.free_regions(page, minimum, limits, Some(control))
    }

    fn free_regions(
        &self,
        page: usize,
        minimum: crate::Size,
        limits: &ResourceLimits,
        control: Option<&OperationControl>,
    ) -> Result<Vec<Rect>> {
        let mut completed = 0;
        free_checkpoint(control, completed)?;
        crate::resolved::validate_scene_contract(self.scene, limits)?;
        free_checkpoint(control, completed)?;
        let page_ref = self.page(page)?;
        let mut budget = crate::diagnostic_budget::DiagnosticBudget::new(limits)?;
        let mut free = vec![Rect::new(
            Unit::ZERO,
            Unit::ZERO,
            page_ref.size.width,
            page_ref.size.height,
        )?];
        for element in page_ref
            .elements
            .iter()
            .filter(|element| element.collidable)
        {
            let mut next = Vec::new();
            for region in free {
                budget.operation()?;
                let pieces = subtract(region, element.bounds.collision)?;
                budget.retained(next.len().saturating_add(pieces.len()))?;
                next.extend(pieces);
                completed += 1;
                free_checkpoint(control, completed)?;
            }
            free = next;
        }
        for exclusion in &page_ref.exclusions {
            let mut next = Vec::new();
            for region in free {
                budget.operation()?;
                let pieces = subtract(region, exclusion.bounds)?;
                budget.retained(next.len().saturating_add(pieces.len()))?;
                next.extend(pieces);
                completed += 1;
                free_checkpoint(control, completed)?;
            }
            free = next;
        }
        free_checkpoint(control, completed)?;
        free.retain(|region| {
            region.size.width >= minimum.width && region.size.height >= minimum.height
        });
        free.sort_by_key(|region| {
            (
                region.origin.y,
                region.origin.x,
                region.size.height,
                region.size.width,
            )
        });
        free_checkpoint(control, completed)?;
        Ok(free)
    }

    fn find(&self, id: &ElementId) -> Result<(usize, &ResolvedElement)> {
        self.scene
            .pages
            .iter()
            .find_map(|page| {
                page.elements
                    .iter()
                    .find(|element| &element.id == id)
                    .map(|element| (page.index, element))
            })
            .ok_or_else(|| inspect_error(format!("element `{}` was not found", id.as_str())))
    }

    fn page(&self, index: usize) -> Result<&ResolvedPage> {
        self.scene
            .pages
            .get(index)
            .ok_or_else(|| inspect_error(format!("page {index} was not found")))
    }
}

fn free_checkpoint(control: Option<&OperationControl>, completed: u64) -> Result<()> {
    if let Some(control) = control {
        control.checkpoint(crate::ProgressPhase::Preflight, completed, None)?;
    }
    Ok(())
}

pub(crate) fn subtract(
    region: Rect,
    occupied: Rect,
) -> Result<impl ExactSizeIterator<Item = Rect>> {
    // A rectangle difference has at most four disjoint strips, in stable order.
    // Keep the temporary pieces inline; only the caller's retained output grows.
    let mut result = [region; 4];
    let Some(overlap) = region.intersection(occupied)? else {
        return Ok(result.into_iter().take(1));
    };
    let mut count = 0;
    if overlap.origin.y > region.origin.y {
        result[count] = Rect::new(
            region.origin.x,
            region.origin.y,
            region.size.width,
            overlap.origin.y.checked_sub(region.origin.y)?,
        )?;
        count += 1;
    }
    if overlap.bottom()? < region.bottom()? {
        result[count] = Rect::new(
            region.origin.x,
            overlap.bottom()?,
            region.size.width,
            region.bottom()?.checked_sub(overlap.bottom()?)?,
        )?;
        count += 1;
    }
    if overlap.origin.x > region.origin.x {
        result[count] = Rect::new(
            region.origin.x,
            overlap.origin.y,
            overlap.origin.x.checked_sub(region.origin.x)?,
            overlap.size.height,
        )?;
        count += 1;
    }
    if overlap.right()? < region.right()? {
        result[count] = Rect::new(
            overlap.right()?,
            overlap.origin.y,
            region.right()?.checked_sub(overlap.right()?)?,
            overlap.size.height,
        )?;
        count += 1;
    }
    Ok(result.into_iter().take(count))
}

fn contains_rect(outer: Rect, inner: Rect) -> Result<bool> {
    Ok(inner.origin.x >= outer.origin.x
        && inner.origin.y >= outer.origin.y
        && inner.right()? <= outer.right()?
        && inner.bottom()? <= outer.bottom()?)
}

fn inspect_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i64, y: i64, width: i64, height: i64) -> Rect {
        Rect::new(
            Unit::from_raw(x),
            Unit::from_raw(y),
            Unit::from_raw(width),
            Unit::from_raw(height),
        )
        .unwrap()
    }

    #[test]
    fn subtraction_preserves_strip_order_and_exact_iterator_length() {
        let region = rect(0, 0, 20, 20);
        let mut pieces = subtract(region, rect(4, 6, 8, 10)).unwrap();
        for (index, expected) in [
            rect(0, 0, 20, 6),
            rect(0, 16, 20, 4),
            rect(0, 6, 4, 10),
            rect(12, 6, 8, 10),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(pieces.len(), 4 - index);
            assert_eq!(pieces.next(), Some(expected));
        }
        assert_eq!(pieces.len(), 0);
        assert_eq!(pieces.next(), None);
        assert_eq!(subtract(region, region).unwrap().len(), 0);
        assert_eq!(
            subtract(region, rect(20, 0, 2, 2)).unwrap().next(),
            Some(region)
        );
    }

    #[test]
    fn subtraction_partitions_free_cells_without_overlap() {
        let region = rect(0, 0, 20, 20);
        for x in [-4, 0, 6, 16, 20] {
            for y in [-4, 0, 6, 16, 20] {
                for width in [0, 2, 10, 40] {
                    for height in [0, 2, 10, 40] {
                        let occupied = rect(x, y, width, height);
                        let pieces: Vec<_> = subtract(region, occupied).unwrap().collect();
                        assert!(pieces.len() <= 4);
                        for cell_x in (1..20).step_by(2) {
                            for cell_y in (1..20).step_by(2) {
                                let point = crate::Point {
                                    x: Unit::from_raw(cell_x),
                                    y: Unit::from_raw(cell_y),
                                };
                                let count = pieces
                                    .iter()
                                    .filter(|piece| piece.contains(point).unwrap())
                                    .count();
                                assert_eq!(count, usize::from(!occupied.contains(point).unwrap()));
                            }
                        }
                    }
                }
            }
        }
    }
}
