// =============================================================================
//        #######
//     ###       ###     F: external_log_paging.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: working-tree by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================
// appcore-norm: test

//! Consumer-side proof of default paging behavior, not bounded provider reads.

use appcore_sync::{InMemoryReplicationLog, ReplicationLog, SyncResult};
use std::cell::Cell;

#[derive(Default)]
struct ExternalLog {
    inner: InMemoryReplicationLog,
    materialized: Cell<usize>,
    first_payload: Cell<usize>,
}

impl ReplicationLog for ExternalLog {
    fn append(&mut self, record: Vec<u8>) -> SyncResult<usize> {
        self.inner.append(record)
    }
    fn append_with_sequence(&mut self, record: Vec<u8>, sequence: u64) -> SyncResult<usize> {
        self.inner.append_with_sequence(record, sequence)
    }
    fn events_since(&self, index: usize) -> SyncResult<Vec<Vec<u8>>> {
        let events = self.inner.events_since(index)?;
        self.materialized.set(events.len());
        self.first_payload
            .set(events.first().map_or(0, |event| event.as_ptr() as usize));
        Ok(events)
    }
    fn last_index(&self) -> SyncResult<usize> {
        Ok(self.inner.last_index())
    }
    fn len(&self) -> SyncResult<usize> {
        self.inner.len()
    }
    fn is_empty(&self) -> SyncResult<bool> {
        self.inner.is_empty()
    }
}

#[test]
fn default_moves_selected_payload_but_still_materializes_provider_tail() {
    let mut log = ExternalLog::default();
    for value in 0..8 {
        log.append(vec![value; 16]).unwrap();
    }
    assert!(log.events_page(0, 0, 16).is_err());
    assert_eq!(log.materialized.get(), 0);
    let page = log.events_page(2, 2, 16).unwrap();
    assert_eq!(page, vec![vec![2; 16]]);
    assert_eq!(log.materialized.get(), 6);
    assert_eq!(page[0].as_ptr() as usize, log.first_payload.get());
    assert!(log.events_page(0, 1, 15).is_err());
    assert!(log.events_page(8, 1, 16).unwrap().is_empty());
    assert!(log.events_page(9, 1, 16).is_err());
}
