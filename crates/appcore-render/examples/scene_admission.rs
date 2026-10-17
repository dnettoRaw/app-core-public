// =============================================================================
//        #######
//     ###       ###     F: scene_admission.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

//! Bounded CPU slot-admission profile, excluding world/asset allocation and GPU work.
use appcore_render::*;
use std::{hint::black_box, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn sample(count: u32, binding: RenderBinding) -> Result<(u128, u128, u64)> {
    let limits = SceneLimits {
        max_entities: count,
        max_snapshot_instances: count,
        max_snapshots: 1,
        managed_cpu_bytes: 512 * 1024 * 1024,
    };
    let bytes = limits.reservation_bytes()?;
    let mut world = SceneWorld::new(limits)?;
    let mut entities = Vec::with_capacity(count as usize);
    let started = Instant::now();
    for _ in 0..count {
        entities.push(world.spawn(black_box(Transform::default()), black_box(binding), None)?);
    }
    let initial = started.elapsed().as_nanos();
    let started = Instant::now();
    for id in entities.drain(..) {
        world.despawn(black_box(id))?;
    }
    for _ in 0..count {
        entities.push(world.spawn(black_box(Transform::default()), black_box(binding), None)?);
    }
    let reuse = started.elapsed().as_nanos();
    black_box((&world, &entities));
    assert_eq!(world.len(), count as usize);
    Ok((initial, reuse, bytes))
}
fn main() -> Result<()> {
    let mut assets = ResourceArena::new(RenderLimits::default())?;
    let binding = RenderBinding {
        mesh: assets.insert((), 1)?,
        material: assets.insert((), 1)?,
        tint: [1.; 4],
        animation: 0,
        morph: 0,
    };
    eprintln!("CPU admission only; target={}/{}; 1 warmup + 5 samples per size; release build required; allocation/drop excluded; reuse=despawn all + spawn all", std::env::consts::OS, std::env::consts::ARCH);
    println!("entities,initial_median_ns,initial_min_ns,initial_max_ns,reuse_median_ns,reuse_min_ns,reuse_max_ns,reservation_bytes");
    for count in [1024, 8192, 32768] {
        black_box(sample(count, binding)?);
        let mut initial = [0; 5];
        let mut reuse = [0; 5];
        let mut bytes = 0;
        for i in 0..5 {
            let result = sample(count, binding)?;
            (initial[i], reuse[i], bytes) = result;
        }
        initial.sort_unstable();
        reuse.sort_unstable();
        println!(
            "{count},{},{},{},{},{},{},{bytes}",
            initial[2], initial[0], initial[4], reuse[2], reuse[0], reuse[4]
        );
    }
    Ok(())
}
