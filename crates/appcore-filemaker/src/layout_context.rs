// =============================================================================
//        #######
//     ###       ###     F: layout_context.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded layout context contracts and behavior for this crate.

use std::collections::BTreeMap;

use crate::{
    CollisionPolicy, CollisionRule, ErrorCode, FileMakerError, LinearSpatialIndex,
    OperationControl, PageTemplate, ProgressPhase, Rect, ResolvedElement, ResolvedExclusion,
    ResolvedPage, ResolvedRegion, Result, Size, SpatialIndex,
};

pub(crate) struct LayoutContext {
    pub(crate) page_size: Size,
    pub(crate) page_template: Option<PageTemplate>,
    pub(crate) max_pages: usize,
    pub(crate) max_elements: usize,
    pub(crate) exclusions: Vec<(ResolvedExclusion, CollisionPolicy)>,
    pub(crate) regions: Vec<ResolvedRegion>,
    pub(crate) pages: Vec<ResolvedPage>,
    pub(crate) indexes: Vec<LinearSpatialIndex>,
    pub(crate) positions: BTreeMap<String, (usize, Rect)>,
    pub(crate) sequence: usize,
    collision_comparisons: usize,
}

impl LayoutContext {
    pub(crate) fn new(
        page_size: Size,
        page_template: Option<PageTemplate>,
        exclusions: Vec<(ResolvedExclusion, CollisionPolicy)>,
        max_pages: usize,
        max_elements: usize,
    ) -> Self {
        Self {
            page_size,
            page_template,
            exclusions,
            regions: Vec::new(),
            max_pages,
            max_elements,
            pages: Vec::new(),
            indexes: Vec::new(),
            positions: BTreeMap::new(),
            sequence: 0,
            collision_comparisons: 0,
        }
    }

    pub(crate) fn ensure_page(&mut self, index: usize) -> Result<()> {
        if index >= self.max_pages {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "generated page count exceeds configured limit",
            ));
        }
        while self.pages.len() <= index {
            let next = self.pages.len();
            let exclusion_instances = next
                .checked_add(1)
                .and_then(|pages| pages.checked_mul(self.exclusions.len()))
                .ok_or_else(|| {
                    FileMakerError::new(
                        ErrorCode::LimitExceeded,
                        "resolved exclusion count overflow",
                    )
                })?;
            if exclusion_instances
                .checked_add(self.sequence)
                .is_none_or(|count| count > self.max_elements)
            {
                return Err(FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    "resolved exclusions exceed configured element limit",
                ));
            }
            self.pages.push(ResolvedPage {
                index: next,
                role: crate::PageRole::Continuation,
                size: self.page_size,
                page_template: self.page_template.clone(),
                exclusions: self
                    .exclusions
                    .iter()
                    .map(|(exclusion, _)| exclusion.clone())
                    .collect(),
                regions: self.regions.clone(),
                elements: Vec::new(),
            });
            let mut spatial = LinearSpatialIndex::default();
            for (sequence, (exclusion, policy)) in self.exclusions.iter().enumerate() {
                spatial.insert(CollisionRule {
                    id: format!("exclusion.{}", exclusion.name),
                    bounds: exclusion.bounds,
                    policy: policy.clone(),
                    sequence,
                })?;
            }
            self.indexes.push(spatial);
        }
        Ok(())
    }

    pub(crate) fn next_sequence(&mut self) -> usize {
        let sequence = self.sequence;
        self.sequence = self.sequence.saturating_add(1);
        sequence
    }

    pub(crate) fn checkpoint(&self, control: &OperationControl, max_elements: usize) -> Result<()> {
        control.checkpoint(
            ProgressPhase::Layout,
            u64::try_from(self.sequence).unwrap_or(u64::MAX),
            Some(u64::try_from(max_elements).unwrap_or(u64::MAX)),
        )
    }

    pub(crate) fn first_collision(
        &mut self,
        page: usize,
        candidate: &CollisionRule,
        maximum: usize,
    ) -> Result<Option<CollisionRule>> {
        Ok(self.indexes[page]
            .first_applicable(candidate, &mut self.collision_comparisons, maximum)?
            .cloned())
    }

    pub(crate) fn commit(
        &mut self,
        page: usize,
        element: ResolvedElement,
        policy: CollisionPolicy,
    ) -> Result<()> {
        self.ensure_page(page)?;
        let exclusion_instances = self
            .pages
            .len()
            .checked_mul(self.exclusions.len())
            .and_then(|count| count.checked_add(self.sequence))
            .ok_or_else(|| {
                FileMakerError::new(ErrorCode::LimitExceeded, "resolved geometry count overflow")
            })?;
        if exclusion_instances > self.max_elements {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "resolved elements and exclusions exceed configured element limit",
            ));
        }
        if policy.enabled {
            self.indexes[page].insert(CollisionRule {
                id: element.id.as_str().to_owned(),
                bounds: element.bounds.collision,
                policy,
                sequence: element.sequence,
            })?;
        }
        self.pages[page].elements.push(element);
        Ok(())
    }
}
