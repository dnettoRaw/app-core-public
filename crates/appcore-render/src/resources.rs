// =============================================================================
//        #######
//     ###       ###     F: resources.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{RenderError, RenderLimits};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

/// Opaque arena-scoped generational identity; carries no pointer or authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceId {
    owner: u64,
    index: u32,
    generation: u64,
}
struct Slot<T> {
    generation: u64,
    value: Option<T>,
    bytes: u64,
}
/// Bounded resource ownership. GPU users must remove only after last submission completes.
/// This arena does not infer GPU completion or provide a submission timeline.
pub struct ResourceArena<T> {
    owner: u64,
    limits: RenderLimits,
    slots: Vec<Slot<T>>,
    bytes: u64,
}
impl<T> ResourceArena<T> {
    /// Create a validated empty arena without preallocating payload storage.
    pub fn new(limits: RenderLimits) -> Result<Self, RenderError> {
        let limits = limits.validate()?;
        let owner = NEXT_OWNER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| RenderError::IdentityExhausted)?;
        Ok(Self {
            owner,
            limits,
            slots: Vec::new(),
            bytes: 0,
        })
    }
    /// Admit a resource with its accounted bytes; rejects before arena mutation.
    pub fn insert(&mut self, value: T, bytes: u64) -> Result<ResourceId, RenderError> {
        let total = self
            .bytes
            .checked_add(bytes)
            .ok_or(RenderError::BudgetExceeded)?;
        if total > self.limits.managed_bytes {
            return Err(RenderError::BudgetExceeded);
        }
        let index = if let Some(i) = self
            .slots
            .iter()
            .position(|s| s.value.is_none() && s.generation < u64::MAX)
        {
            i
        } else {
            if self.slots.len() >= self.limits.resource_slots as usize {
                return Err(RenderError::BudgetExceeded);
            }
            self.slots.push(Slot {
                generation: 0,
                value: None,
                bytes: 0,
            });
            self.slots.len() - 1
        };
        let slot = &mut self.slots[index];
        slot.generation += 1;
        slot.value = Some(value);
        slot.bytes = bytes;
        self.bytes = total;
        Ok(ResourceId {
            owner: self.owner,
            index: index as u32,
            generation: slot.generation,
        })
    }
    fn slot(&self, id: ResourceId) -> Result<&Slot<T>, RenderError> {
        if id.owner != self.owner {
            return Err(RenderError::InvalidResource);
        }
        self.slots
            .get(id.index as usize)
            .filter(|s| s.generation == id.generation && s.value.is_some())
            .ok_or(RenderError::InvalidResource)
    }
    /// Resolve only a live handle from this arena.
    pub fn get(&self, id: ResourceId) -> Result<&T, RenderError> {
        self.slot(id)?
            .value
            .as_ref()
            .ok_or(RenderError::InvalidResource)
    }
    /// Remove a resource. Caller must first prove GPU completion for GPU-owned values.
    pub fn remove(&mut self, id: ResourceId) -> Result<T, RenderError> {
        self.slot(id)?;
        let slot = &mut self.slots[id.index as usize];
        let value = slot.value.take().ok_or(RenderError::InvalidResource)?;
        self.bytes -= slot.bytes;
        slot.bytes = 0;
        Ok(value)
    }
    /// Current accounted live bytes; not total driver allocation.
    pub fn managed_bytes(&self) -> u64 {
        self.bytes
    }
}
