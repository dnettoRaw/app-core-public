// =============================================================================
//        #######
//     ###       ###     F: storage_write_barrier.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/25 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/25 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Generic admission barrier for storage writes and update installation.

use super::{StorageError, StorageResult};
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const MAX_OWNER_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Admission {
    Open,
    Blocked,
    Sealed,
}

#[derive(Debug)]
struct BarrierState {
    admission: Admission,
    active: BTreeMap<String, usize>,
}

/// Lifecycle state of a storage write barrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteBarrierState {
    /// New writers are accepted.
    Open,
    /// New writers are blocked while existing writers drain.
    Blocked,
    /// Installation started or failed; the barrier remains closed until restart.
    Sealed,
}

/// One active owner and its number of permits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteBarrierOwner {
    /// Bounded caller-provided owner label.
    pub owner: String,
    /// Number of active root or nested permits held by this owner.
    pub permits: usize,
}

/// Bounded diagnostic snapshot of write-barrier state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteBarrierSnapshot {
    /// Current admission state.
    pub state: WriteBarrierState,
    /// Active owners and permit counts.
    pub owners: Vec<WriteBarrierOwner>,
}

/// Coordinates storage writers with update installation without claiming
/// database transaction semantics.
#[derive(Debug, Clone)]
pub struct StorageWriteBarrier {
    state: Arc<(Mutex<BarrierState>, Condvar)>,
}

impl Default for StorageWriteBarrier {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageWriteBarrier {
    /// Creates an open barrier with no active writers.
    pub fn new() -> Self {
        Self {
            state: Arc::new((
                Mutex::new(BarrierState {
                    admission: Admission::Open,
                    active: BTreeMap::new(),
                }),
                Condvar::new(),
            )),
        }
    }

    /// Opens one bounded writer admission permit.
    pub fn open(&self, owner: impl Into<String>) -> StorageResult<WritePermit> {
        let owner = validate_owner(owner.into())?;
        let (lock, _) = &*self.state;
        let mut state = lock.lock().map_err(|_| StorageError::NotAvailable)?;
        match state.admission {
            Admission::Open => {
                increment_owner(&mut state.active, &owner);
                Ok(WritePermit {
                    barrier: self.clone(),
                    owner,
                    released: false,
                })
            }
            Admission::Blocked => Err(StorageError::WriteBarrierBlocked),
            Admission::Sealed => Err(StorageError::WriteBarrierSealed),
        }
    }

    /// Blocks new writers while existing permits remain drainable.
    pub fn block_new_writers(&self) -> StorageResult<()> {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().map_err(|_| StorageError::NotAvailable)?;
        match state.admission {
            Admission::Open => state.admission = Admission::Blocked,
            Admission::Blocked => {}
            Admission::Sealed => return Err(StorageError::WriteBarrierSealed),
        }
        Ok(())
    }

    /// Waits for all existing writers to leave after blocking admission.
    pub fn drain(&self, timeout: Duration) -> StorageResult<()> {
        let (lock, condition) = &*self.state;
        let mut state = lock.lock().map_err(|_| StorageError::NotAvailable)?;
        if state.admission == Admission::Open {
            return Err(StorageError::WriteBarrierNotDrained);
        }
        let deadline = Instant::now() + timeout;
        while !state.active.is_empty() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(StorageError::WriteBarrierTimeout);
            }
            let (next, result) = condition
                .wait_timeout(state, remaining)
                .map_err(|_| StorageError::NotAvailable)?;
            state = next;
            if result.timed_out() && !state.active.is_empty() {
                return Err(StorageError::WriteBarrierTimeout);
            }
        }
        Ok(())
    }

    /// Seals the barrier after installation begins; it cannot be released in-process.
    pub fn seal_after_install_start(&self) -> StorageResult<()> {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().map_err(|_| StorageError::NotAvailable)?;
        if !state.active.is_empty() {
            return Err(StorageError::WriteBarrierNotDrained);
        }
        match state.admission {
            Admission::Blocked => {
                state.admission = Admission::Sealed;
                Ok(())
            }
            Admission::Sealed => Err(StorageError::WriteBarrierSealed),
            Admission::Open => Err(StorageError::WriteBarrierNotDrained),
        }
    }

    /// Releases a drained, non-sealed barrier for normal writes.
    pub fn release(&self) -> StorageResult<()> {
        let (lock, condition) = &*self.state;
        let mut state = lock.lock().map_err(|_| StorageError::NotAvailable)?;
        if state.admission == Admission::Sealed {
            return Err(StorageError::WriteBarrierSealed);
        }
        if !state.active.is_empty() {
            return Err(StorageError::WriteBarrierNotDrained);
        }
        state.admission = Admission::Open;
        condition.notify_all();
        Ok(())
    }

    /// Returns bounded owner and admission diagnostics without changing state.
    pub fn snapshot(&self) -> StorageResult<WriteBarrierSnapshot> {
        let (lock, _) = &*self.state;
        let state = lock.lock().map_err(|_| StorageError::NotAvailable)?;
        Ok(WriteBarrierSnapshot {
            state: state.admission.into(),
            owners: state
                .active
                .iter()
                .map(|(owner, permits)| WriteBarrierOwner {
                    owner: owner.clone(),
                    permits: *permits,
                })
                .collect(),
        })
    }

    fn close_permit(&self, owner: &str) {
        let (lock, condition) = &*self.state;
        if let Ok(mut state) = lock.lock() {
            if let Some(permits) = state.active.get_mut(owner) {
                *permits = permits.saturating_sub(1);
                if *permits == 0 {
                    state.active.remove(owner);
                }
            }
            condition.notify_all();
        }
    }
}

impl From<Admission> for WriteBarrierState {
    fn from(value: Admission) -> Self {
        match value {
            Admission::Open => Self::Open,
            Admission::Blocked => Self::Blocked,
            Admission::Sealed => Self::Sealed,
        }
    }
}

/// Active root writer permit; dropping it drains one admission count.
#[derive(Debug)]
pub struct WritePermit {
    barrier: StorageWriteBarrier,
    owner: String,
    released: bool,
}

impl WritePermit {
    /// Opens one nested permit for the same logical transaction owner.
    pub fn nested(&self) -> StorageResult<NestedWritePermit> {
        let (lock, _) = &*self.barrier.state;
        let mut state = lock.lock().map_err(|_| StorageError::NotAvailable)?;
        if !state.active.contains_key(&self.owner) {
            return Err(StorageError::NotAvailable);
        }
        increment_owner(&mut state.active, &self.owner);
        Ok(NestedWritePermit {
            barrier: self.barrier.clone(),
            owner: self.owner.clone(),
        })
    }

    /// Returns this permit's bounded owner label.
    pub fn owner(&self) -> &str {
        &self.owner
    }
}

impl Drop for WritePermit {
    fn drop(&mut self) {
        if !self.released {
            self.released = true;
            self.barrier.close_permit(&self.owner);
        }
    }
}

/// Nested admission permit for one logical writer transaction.
#[derive(Debug)]
pub struct NestedWritePermit {
    barrier: StorageWriteBarrier,
    owner: String,
}

impl NestedWritePermit {
    /// Returns this nested permit's bounded owner label.
    pub fn owner(&self) -> &str {
        &self.owner
    }
}

impl Drop for NestedWritePermit {
    fn drop(&mut self) {
        self.barrier.close_permit(&self.owner);
    }
}

fn increment_owner(active: &mut BTreeMap<String, usize>, owner: &str) {
    let permits = active.entry(owner.to_string()).or_default();
    *permits = permits.saturating_add(1);
}

fn validate_owner(owner: String) -> StorageResult<String> {
    if owner.trim().is_empty()
        || owner.len() > MAX_OWNER_BYTES
        || owner.chars().any(char::is_control)
    {
        return Err(StorageError::WriteBarrierInvalidOwner);
    }
    Ok(owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_nested_writers_and_reports_owner() {
        let barrier = StorageWriteBarrier::new();
        let permit = barrier.open("runtime").unwrap();
        let nested = permit.nested().unwrap();
        assert_eq!(barrier.snapshot().unwrap().owners[0].permits, 2);
        barrier.block_new_writers().unwrap();
        assert!(matches!(
            barrier.open("other"),
            Err(StorageError::WriteBarrierBlocked)
        ));
        drop(nested);
        drop(permit);
        barrier.drain(Duration::from_millis(10)).unwrap();
        barrier.release().unwrap();
        assert_eq!(barrier.snapshot().unwrap().state, WriteBarrierState::Open);
    }

    #[test]
    fn seals_after_drain_and_cannot_be_released() {
        let barrier = StorageWriteBarrier::new();
        let permit = barrier.open("installer").unwrap();
        barrier.block_new_writers().unwrap();
        drop(permit);
        barrier.drain(Duration::from_millis(10)).unwrap();
        barrier.seal_after_install_start().unwrap();
        assert!(matches!(
            barrier.release(),
            Err(StorageError::WriteBarrierSealed)
        ));
        assert!(matches!(
            barrier.open("after-install"),
            Err(StorageError::WriteBarrierSealed)
        ));
    }

    #[test]
    fn drain_times_out_without_clearing_owner() {
        let barrier = StorageWriteBarrier::new();
        let permit = barrier.open("stuck").unwrap();
        barrier.block_new_writers().unwrap();
        assert!(matches!(
            barrier.drain(Duration::from_millis(1)),
            Err(StorageError::WriteBarrierTimeout)
        ));
        assert_eq!(barrier.snapshot().unwrap().owners[0].owner, "stuck");
        drop(permit);
    }
}
