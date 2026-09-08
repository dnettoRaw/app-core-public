// =============================================================================
//        #######
//     ###       ###     F: raster_candidates.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 12:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Experimental candidate indexing only; not a production renderer or layout engine.

use appcore_filemaker::{FontManager, ResolvedScene, ResourceLimits};
use std::hint::black_box;

pub(super) const CASES: [&str; 4] = [
    "raster_candidates_linear_8",
    "raster_candidates_indexed_8",
    "raster_candidates_linear_256",
    "raster_candidates_indexed_256",
];

pub(super) fn run(selected: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    if selected.is_some_and(|name| !CASES.contains(&name)) {
        return Ok(());
    }
    let scene =
        super::raster_strips::scene(&ResourceLimits::default(), &FontManager::default(), true)?;
    for (case_index, case) in CASES.into_iter().enumerate() {
        if selected.is_some_and(|name| name != case) {
            continue;
        }
        let rows = if case_index < 2 { 8 } else { 256 };
        let expected = linear(&scene, rows)?;
        let fixture = buckets(&scene, rows)?;
        validate_candidates(&scene, rows, &fixture)?;
        assert_eq!(indexed(&fixture), expected);
        let bytes = fixture.capacity() * std::mem::size_of::<Vec<usize>>()
            + fixture
                .iter()
                .map(|bucket| bucket.capacity() * std::mem::size_of::<usize>())
                .sum::<usize>();
        println!("appcore-filemaker::{case} index_capacity_bytes={bytes}");
        drop(fixture);
        super::measure(case, 10, || {
            // Rebuild inside timing: comparison includes one-shot index construction.
            let actual = if case_index % 2 == 0 {
                linear(&scene, rows)?
            } else {
                indexed(&buckets(&scene, rows)?)
            };
            assert_eq!(black_box(actual), expected);
            Ok(())
        })?;
    }
    Ok(())
}

fn span(element: &appcore_filemaker::ResolvedElement) -> appcore_filemaker::Result<(f32, f32)> {
    // Mirrors the production one-page 96-DPI predicate, including antialias padding.
    let bounds = element.bounds.visual;
    let scale = 96.0_f32 / 72.0;
    Ok((
        (bounds.origin.y.as_points_f64() as f32 * scale).floor() - 1.0,
        (bounds.bottom()?.as_points_f64() as f32 * scale).ceil() + 1.0,
    ))
}

fn linear(scene: &ResolvedScene, rows: usize) -> appcore_filemaker::Result<(usize, u64)> {
    let mut result = (0, 0);
    for top in (0..1080).step_by(rows) {
        let bottom = (top + rows).min(1080);
        for (index, element) in scene.pages[0].elements.iter().enumerate() {
            let (start, end) = span(element)?;
            if end > top as f32 && start < bottom as f32 {
                record(&mut result, index);
            }
        }
    }
    Ok(result)
}

fn buckets(scene: &ResolvedScene, rows: usize) -> appcore_filemaker::Result<Vec<Vec<usize>>> {
    let count = 1080_usize.div_ceil(rows);
    let mut buckets = vec![Vec::new(); count];
    let mut memberships = 0;
    for (index, element) in scene.pages[0].elements.iter().enumerate() {
        let (start, end) = span(element)?;
        let first = (start.max(0.0) as usize / rows).min(count);
        let last = (end.max(0.0) as usize).div_ceil(rows).min(count);
        for bucket in &mut buckets[first..last] {
            memberships += 1;
            if memberships > 65_536 {
                return Err(appcore_filemaker::FileMakerError::new(
                    appcore_filemaker::ErrorCode::LimitExceeded,
                    "benchmark index membership cap",
                ));
            }
            bucket.push(index);
        }
    }
    Ok(buckets)
}

fn validate_candidates(
    scene: &ResolvedScene,
    rows: usize,
    buckets: &[Vec<usize>],
) -> appcore_filemaker::Result<()> {
    for (strip, bucket) in buckets.iter().enumerate() {
        let top = strip * rows;
        let bottom = (top + rows).min(1080);
        let mut expected = Vec::new();
        for (index, element) in scene.pages[0].elements.iter().enumerate() {
            let (start, end) = span(element)?;
            if end > top as f32 && start < bottom as f32 {
                expected.push(index);
            }
        }
        assert_eq!(*bucket, expected);
    }
    Ok(())
}

fn indexed(buckets: &[Vec<usize>]) -> (usize, u64) {
    let mut result = (0, 0);
    for bucket in buckets {
        for index in bucket {
            record(&mut result, *index);
        }
    }
    result
}

fn record(result: &mut (usize, u64), index: usize) {
    result.0 += 1;
    result.1 = result.1.wrapping_mul(31).wrapping_add(index as u64);
}
