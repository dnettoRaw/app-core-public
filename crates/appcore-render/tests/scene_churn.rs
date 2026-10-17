// =============================================================================
//        #######
//     ###       ###     F: scene_churn.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::*;

#[test]
fn bounded_churn_preserves_live_rows_snapshots_and_stale_id_rejection() -> Result<(), RenderError> {
    let count = 4096;
    let limits = SceneLimits {
        max_entities: count,
        max_snapshot_instances: count,
        ..Default::default()
    };
    let bytes = limits.reservation_bytes()?;
    assert!(matches!(
        SceneWorld::new(SceneLimits {
            managed_cpu_bytes: bytes - 1,
            ..limits
        }),
        Err(RenderError::BudgetExceeded)
    ));
    let mut world = SceneWorld::new(SceneLimits {
        managed_cpu_bytes: bytes,
        ..limits
    })?;
    let mut assets = ResourceArena::new(RenderLimits::default())?;
    let binding = RenderBinding {
        mesh: assets.insert((), 1)?,
        material: assets.insert((), 1)?,
        tint: [1.; 4],
        animation: 0,
        morph: 0,
    };
    let moving = Some(Velocity::new([0., 1., 0.])?);
    let mut expected = Vec::new();
    for i in 0..count {
        let pose = Transform::default().with_position([i as f32, 0., 0.])?;
        expected.push((world.spawn(pose, binding, None)?, pose));
    }
    let original = world.snapshot()?;
    let mut state = 0x16a2_390bu32;
    for round in 1..=8 {
        for _ in 0..count {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let i = (state as usize >> 8) % count as usize;
            let (old, _) = expected[i];
            let pose = Transform::default().with_position([i as f32, round as f32, 0.])?;
            match state % 3 {
                0 => world.set_velocity(old, moving)?,
                1 => world.set_velocity(old, None)?,
                _ => {
                    world.despawn(old)?;
                    assert_eq!(world.despawn(old), Err(RenderError::InvalidResource));
                    let invalid = RenderBinding {
                        tint: [f32::NAN; 4],
                        ..binding
                    };
                    assert_eq!(
                        world.spawn(pose, invalid, None),
                        Err(RenderError::InvalidLimits)
                    );
                    let fresh = world.spawn(pose, binding, moving)?;
                    assert_ne!(fresh, old);
                    assert_eq!(world.transform(old), Err(RenderError::InvalidResource));
                    expected[i].0 = fresh;
                }
            }
            world.set_transform(expected[i].0, pose)?;
            expected[i].1 = pose;
        }
        let frame = world.snapshot()?;
        assert_eq!(frame.instances().len(), count as usize);
        let mut seen = vec![false; count as usize];
        for instance in frame.instances() {
            let i = instance.transform.position()[0] as usize;
            assert!(!seen[i]);
            seen[i] = true;
            assert_eq!(instance.entity, expected[i].0);
            assert_eq!(instance.transform, expected[i].1);
            assert_eq!(instance.binding, binding);
        }
        assert!(seen.iter().all(|value| *value));
    }
    assert!(original
        .instances()
        .iter()
        .all(|instance| instance.transform.position()[1] == 0.));
    drop(original);
    for (id, _) in &expected {
        world.despawn(*id)?;
    }
    assert!(world.is_empty());
    assert!(world.snapshot()?.instances().is_empty());
    for _ in 0..count {
        world.spawn(Transform::default(), binding, None)?;
    }
    assert_eq!(
        world.spawn(Transform::default(), binding, None),
        Err(RenderError::BudgetExceeded)
    );
    assert_eq!(world.len(), count as usize);
    Ok(())
}
