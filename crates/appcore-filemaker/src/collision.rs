// =============================================================================
//        #######
//     ###       ###     F: collision.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded collision contracts and behavior for this crate.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{Rect, Result};

/// Which resolved box participates in collision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionBounds {
    /// Layout box.
    #[default]
    Layout,
    /// Visual box including stroke/effects.
    Visual,
    /// Intrinsic content box.
    Intrinsic,
}

/// Resolution applied when geometry overlaps.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionResolution {
    /// Move the lower-priority movable node forward in flow.
    #[default]
    Push,
    /// Reject layout.
    Error,
    /// Accept overlap explicitly.
    Overlay,
    /// Move the candidate to the next page.
    NextPage,
    /// Reduce the candidate within its minimum bounds.
    Shrink,
}

/// Inheritable collision policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CollisionPolicy {
    /// Whether geometry participates.
    pub enabled: bool,
    /// Collision group.
    pub group: String,
    /// Groups collided with; empty means every group.
    pub collides_with: BTreeSet<String>,
    /// IDs ignored explicitly.
    pub ignore: BTreeSet<String>,
    /// Higher value wins movement conflict.
    pub priority: i32,
    /// Whether the resolver may reposition the node.
    pub movable: bool,
    /// Selected resolved box.
    pub bounds: CollisionBounds,
    /// Overlap resolution.
    pub resolution: CollisionResolution,
}

impl Default for CollisionPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            group: "default".to_owned(),
            collides_with: BTreeSet::new(),
            ignore: BTreeSet::new(),
            priority: 0,
            movable: true,
            bounds: CollisionBounds::Layout,
            resolution: CollisionResolution::Push,
        }
    }
}

/// Indexed collision rule and geometry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollisionRule {
    /// Stable element ID.
    pub id: String,
    /// Page-local collision bounds.
    pub bounds: Rect,
    /// Effective policy.
    pub policy: CollisionPolicy,
    /// Stable insertion sequence.
    pub sequence: usize,
}

impl CollisionRule {
    /// Whether this rule can collide with another effective rule.
    #[must_use]
    pub fn applies_to(&self, other: &Self) -> bool {
        self.policy.enabled
            && other.policy.enabled
            && !self.policy.ignore.contains(&other.id)
            && !other.policy.ignore.contains(&self.id)
            && (self.policy.collides_with.is_empty()
                || self.policy.collides_with.contains(&other.policy.group))
            && (other.policy.collides_with.is_empty()
                || other.policy.collides_with.contains(&self.policy.group))
    }
}

/// Deterministic spatial query contract.
pub trait SpatialIndex {
    /// Inserts one resolved rule.
    fn insert(&mut self, rule: CollisionRule) -> Result<()>;
    /// Returns overlapping rules in stable insertion order.
    fn query(&self, bounds: Rect) -> Result<Vec<&CollisionRule>>;
    /// Number of indexed rules.
    fn len(&self) -> usize;
    /// Whether no rules are indexed.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Simple deterministic linear index; suitable baseline and test oracle.
#[derive(Clone, Debug, Default)]
pub struct LinearSpatialIndex {
    rules: Vec<CollisionRule>,
}

impl SpatialIndex for LinearSpatialIndex {
    fn insert(&mut self, rule: CollisionRule) -> Result<()> {
        self.rules.push(rule);
        self.rules.sort_by_key(|entry| entry.sequence);
        Ok(())
    }

    fn query(&self, bounds: Rect) -> Result<Vec<&CollisionRule>> {
        self.rules
            .iter()
            .filter_map(|rule| match rule.bounds.intersects(bounds) {
                Ok(true) => Some(Ok(rule)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    fn len(&self) -> usize {
        self.rules.len()
    }
}

impl LinearSpatialIndex {
    pub(crate) fn first_applicable(
        &self,
        candidate: &CollisionRule,
        comparisons: &mut usize,
        maximum: usize,
    ) -> Result<Option<&CollisionRule>> {
        for rule in &self.rules {
            *comparisons = comparisons.checked_add(1).ok_or_else(|| {
                crate::FileMakerError::new(
                    crate::ErrorCode::LimitExceeded,
                    "layout collision comparison count overflow",
                )
            })?;
            if *comparisons > maximum {
                return Err(crate::FileMakerError::new(
                    crate::ErrorCode::LimitExceeded,
                    "layout collision comparison budget exhausted",
                ));
            }
            if candidate.applies_to(rule) && rule.bounds.intersects(candidate.bounds)? {
                return Ok(Some(rule));
            }
        }
        Ok(None)
    }
}
