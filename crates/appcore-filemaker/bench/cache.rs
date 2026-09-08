// =============================================================================
//        #######
//     ###       ###     F: cache.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Measures cache residency when consumers retain evicted scene leases.

use appcore_filemaker::{DocumentFingerprint, FingerprintBuilder, ResourceLimits, SceneCache};
use std::hint::black_box;

pub(super) const CASE: &str = "cache_retired_leases_8x256";

pub(super) fn run(selected: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    if selected.is_some_and(|value| value != CASE) {
        return Ok(());
    }
    let limits = ResourceLimits::default();
    let scene = super::diagnostic_scene(&limits)?;
    super::measure(CASE, 10, || {
        let mut cache = SceneCache::with_byte_capacity(8, 64 * 1024 * 1024)?;
        let mut leases = Vec::with_capacity(8);
        for index in 0..8_u8 {
            leases.push(cache.insert(fingerprint(index)?, scene.clone(), &limits)?);
        }
        assert!(cache
            .insert(fingerprint(8)?, scene.clone(), &limits)
            .is_err());
        assert_eq!(cache.len(), 0);
        assert!(cache.retired_bytes() > 0);
        black_box((cache, leases));
        Ok(())
    })
}

fn fingerprint(value: u8) -> Result<DocumentFingerprint, appcore_filemaker::FileMakerError> {
    let mut builder = FingerprintBuilder::new();
    builder.field("cache-benchmark", &[value])?;
    Ok(builder.finish())
}
