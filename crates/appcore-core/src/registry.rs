// =============================================================================
//        #######
//     ###       ###     F: registry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Shared registry primitive for ordered, duplicate-safe name registration.

use crate::error::{RuntimeError, RuntimeResult};

/// Generic ordered registry.
#[derive(Debug)]
pub struct NameRegistry<T> {
    items: Vec<T>,
}

impl<T> Default for NameRegistry<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> NameRegistry<T>
where
    T: PartialEq,
{
    /// Creates an empty ordered registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one item and rejects equality-based duplicates.
    pub fn register(&mut self, item: T, kind: &str) -> RuntimeResult<()> {
        if self.items.contains(&item) {
            return Err(RuntimeError::DuplicateRegistryItem {
                kind: kind.to_string(),
            });
        }

        self.items.push(item);
        Ok(())
    }

    /// Reports whether an equal item is registered.
    pub fn contains(&self, item: &T) -> bool {
        self.items.contains(item)
    }

    /// Returns the number of registered items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Reports whether no items are registered.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns items in registration order.
    pub fn list(&self) -> &[T] {
        &self.items
    }
}
