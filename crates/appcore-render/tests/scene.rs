// =============================================================================
//        #######
//     ###       ###     F: scene.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::*;
fn assets() -> Result<(ResourceArena<()>, RenderBinding), RenderError> {
    let mut arena = ResourceArena::new(RenderLimits::default())?;
    let binding = RenderBinding {
        mesh: arena.insert((), 1)?,
        material: arena.insert((), 1)?,
        tint: [1.0; 4],
        animation: 0,
        morph: 0,
    };
    Ok((arena, binding))
}
#[test]
fn migration_and_compaction_preserve_other_entity_locations() -> Result<(), RenderError> {
    let (_assets, binding) = assets()?;
    let mut world = SceneWorld::new(SceneLimits {
        max_entities: 4,
        max_snapshot_instances: 4,
        ..Default::default()
    })?;
    let a = world.spawn(Transform::default(), binding, None)?;
    let b = world.spawn(
        Transform::default().with_position([2.0, 0.0, 0.0])?,
        binding,
        None,
    )?;
    let c = world.spawn(
        Transform::default().with_position([3.0, 0.0, 0.0])?,
        binding,
        None,
    )?;
    world.set_velocity(a, Some(Velocity::new([1.0, 0.0, 0.0])?))?;
    assert_eq!(world.transform(c)?.position(), [3.0, 0.0, 0.0]);
    world.despawn(b)?;
    world.set_velocity(c, Some(Velocity::new([0.0, 2.0, 0.0])?))?;
    world.set_velocity(a, None)?;
    world.step_velocity(0.05)?;
    assert_eq!(world.transform(a)?.position(), [0.0; 3]);
    assert_eq!(world.transform(c)?.position(), [3.0, 0.1, 0.0]);
    world.despawn(a)?;
    let fresh = world.spawn(Transform::default(), binding, None)?;
    assert_ne!(a, fresh);
    assert_eq!(world.transform(a), Err(RenderError::InvalidResource));
    let mut foreign = SceneWorld::new(SceneLimits::default())?;
    let id = foreign.spawn(Transform::default(), binding, None)?;
    assert_eq!(world.despawn(id), Err(RenderError::InvalidResource));
    Ok(())
}
#[test]
fn snapshots_are_owned_bounded_and_release_their_permits() -> Result<(), RenderError> {
    let (_assets, binding) = assets()?;
    let mut world = SceneWorld::new(SceneLimits {
        max_entities: 2,
        max_snapshot_instances: 2,
        max_snapshots: 2,
        ..Default::default()
    })?;
    let entity = world.spawn(Transform::default(), binding, None)?;
    let first = world.snapshot()?;
    let second = world.snapshot()?;
    assert!(matches!(
        world.snapshot(),
        Err(RenderError::SnapshotsPending)
    ));
    world.set_transform(entity, Transform::default().with_position([1.0, 0.0, 0.0])?)?;
    world.despawn(entity)?;
    assert_eq!(first.instances()[0].transform.position(), [0.0; 3]);
    drop(second);
    let empty = world.snapshot()?;
    assert!(empty.instances().is_empty());
    drop(world);
    assert_eq!(first.instances()[0].entity, entity);
    Ok(())
}
#[test]
fn limits_and_failed_velocity_step_leave_world_unchanged() -> Result<(), RenderError> {
    assert!(SceneWorld::new(SceneLimits {
        managed_cpu_bytes: 1,
        ..Default::default()
    })
    .is_err());
    assert!(Transform::new([0.0; 3], [0.0; 4], [1.0; 3]).is_err());
    assert!(Velocity::new([f32::NAN, 0.0, 0.0]).is_err());
    let (_assets, binding) = assets()?;
    let mut world = SceneWorld::new(SceneLimits {
        max_entities: 2,
        max_snapshot_instances: 1,
        ..Default::default()
    })?;
    let a = world.spawn(
        Transform::default(),
        binding,
        Some(Velocity::new([1.0, 0.0, 0.0])?),
    )?;
    let b = world.spawn(
        Transform::default().with_position([1_000_000.0, 0.0, 0.0])?,
        binding,
        Some(Velocity::new([100.0, 0.0, 0.0])?),
    )?;
    assert_eq!(world.step_velocity(0.05), Err(RenderError::InvalidLimits));
    assert_eq!(world.transform(a)?.position(), [0.0; 3]);
    assert_eq!(world.transform(b)?.position(), [1_000_000.0, 0.0, 0.0]);
    assert!(matches!(world.snapshot(), Err(RenderError::BudgetExceeded)));
    assert!(matches!(
        world.spawn(Transform::default(), binding, None),
        Err(RenderError::BudgetExceeded)
    ));
    assert_eq!(world.len(), 2);
    Ok(())
}
#[test]
fn disjoint_columns_and_deferred_commands_keep_rows_aligned() -> Result<(), RenderError> {
    let (_assets, binding) = assets()?;
    let mut world = SceneWorld::new(SceneLimits::default())?;
    let a = world.spawn(
        Transform::default(),
        binding,
        Some(Velocity::new([1.0, 0.0, 0.0])?),
    )?;
    let b = world.spawn(
        Transform::default(),
        binding,
        Some(Velocity::new([2.0, 0.0, 0.0])?),
    )?;
    {
        let columns = world.moving_columns();
        assert_eq!(columns.entities.len(), columns.transforms.len());
        assert_eq!(columns.transforms.len(), columns.velocities.len());
        let zero = Velocity::new([0.0; 3])?;
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for transform in columns.transforms {
                    *transform = Transform::default();
                }
            });
            scope.spawn(|| {
                for velocity in columns.velocities {
                    *velocity = zero;
                }
            });
        });
    }
    let mut commands = SceneCommands::new(3)?;
    commands.push(SceneCommand::Despawn(a))?;
    commands.push(SceneCommand::Despawn(a))?;
    commands.push(SceneCommand::SetVelocity(b, None))?;
    assert_eq!(
        commands.push(SceneCommand::Despawn(b)),
        Err(RenderError::BudgetExceeded)
    );
    let outcomes = commands.apply(&mut world)?;
    assert_eq!(
        outcomes,
        [Ok(()), Err(RenderError::InvalidResource), Ok(())]
    );
    assert_eq!(world.len(), 1);
    assert!(world.moving_columns().entities.is_empty());
    assert_eq!(world.transform(b)?.position(), [0.0; 3]);
    Ok(())
}

#[test]
fn parallel_snapshot_admission_does_not_overbook() -> Result<(), RenderError> {
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Barrier,
    };
    let world = SceneWorld::new(SceneLimits::default())?;
    let barrier = Arc::new(Barrier::new(9));
    let accepted = Arc::new(AtomicU32::new(0));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let (world, barrier, accepted) = (&world, barrier.clone(), accepted.clone());
            scope.spawn(move || {
                let snapshot = world.snapshot();
                if snapshot.is_ok() {
                    accepted.fetch_add(1, Ordering::AcqRel);
                }
                barrier.wait();
                barrier.wait();
                drop(snapshot);
            });
        }
        barrier.wait();
        assert_eq!(accepted.load(Ordering::Acquire), 2);
        barrier.wait();
    });
    assert!(world.snapshot()?.instances().is_empty());
    assert_eq!(std::mem::align_of::<Transform>(), 16);
    assert_eq!(std::mem::align_of::<Velocity>(), 16);
    let a = SceneLimits {
        max_snapshots: 1,
        ..Default::default()
    }
    .reservation_bytes()?;
    let b = SceneLimits {
        max_snapshots: 2,
        ..Default::default()
    }
    .reservation_bytes()?;
    assert_eq!(
        b - a,
        u64::from(SceneLimits::default().max_snapshot_instances)
            * std::mem::size_of::<SceneInstance>() as u64
    );
    Ok(())
}
