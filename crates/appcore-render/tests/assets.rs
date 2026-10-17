// =============================================================================
//        #######
//     ###       ###     F: assets.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::*;
fn triangle() -> Result<Vec<MeshVertex>, RenderError> {
    [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        .into_iter()
        .map(|p| MeshVertex::new(p, [0.0, 0.0, 1.0]))
        .collect()
}
fn binding(mesh: ResourceId, material: ResourceId) -> RenderBinding {
    RenderBinding {
        mesh,
        material,
        tint: [1.0, 0.5, 1.0, 1.0],
        animation: 0,
        morph: 0,
    }
}
#[test]
fn preparation_resolves_types_and_retains_retired_shared_payloads() -> Result<(), RenderError> {
    let mut assets = AssetRegistry::new(AssetLimits::default())?;
    let mesh = assets.register_mesh(triangle()?, vec![0, 1, 2])?;
    let material = assets.register_material([0.8, 0.6, 0.4, 1.0])?;
    let mut world = SceneWorld::new(SceneLimits::default())?;
    let transform = Transform::new([5.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [2.0, 3.0, 1.0])?;
    let entity = world.spawn(transform, binding(mesh, material), None)?;
    let prepared = assets.prepare(world.snapshot()?)?;
    let instance = &prepared.instances()[0];
    assert_eq!(instance.entity(), entity);
    assert_eq!(instance.color(), [0.8, 0.3, 0.4, 1.0]);
    assert_eq!(instance.world_bounds().center, [6.0, 1.5, 0.0]);
    assert!(instance.world_bounds().radius >= 3.0 * (0.5f32).sqrt());
    let bytes = assets.managed_bytes();
    assets.remove_mesh(mesh)?;
    assets.remove_material(material)?;
    assert_eq!(assets.managed_bytes(), bytes);
    assert_eq!(prepared.instances()[0].mesh().indices(), [0, 1, 2]);
    assert!(matches!(
        assets.prepare(world.snapshot()?),
        Err(RenderError::InvalidResource)
    ));
    drop(prepared);
    assert_eq!(assets.managed_bytes(), 0);
    Ok(())
}
#[test]
fn invalid_geometry_and_bindings_fail_before_upload_or_mutation() -> Result<(), RenderError> {
    let mut assets = AssetRegistry::new(AssetLimits::default())?;
    assert_eq!(
        assets.register_mesh(triangle()?, vec![0, 1, 3]),
        Err(RenderError::InvalidLimits)
    );
    assert_eq!(
        assets.register_mesh(triangle()?, vec![0, 0, 1]),
        Err(RenderError::InvalidLimits)
    );
    assert_eq!(assets.managed_bytes(), 0);
    let mesh = assets.register_mesh(triangle()?, vec![0, 1, 2])?;
    let material = assets.register_material([1.0; 4])?;
    let mut foreign = AssetRegistry::new(AssetLimits::default())?;
    let foreign_mesh = foreign.register_mesh(triangle()?, vec![0, 1, 2])?;
    let mut world = SceneWorld::new(SceneLimits::default())?;
    for bad in [
        binding(material, mesh),
        binding(foreign_mesh, material),
        RenderBinding {
            animation: 1,
            ..binding(mesh, material)
        },
    ] {
        let entity = world.spawn(Transform::default(), bad, None)?;
        assert!(matches!(
            assets.prepare(world.snapshot()?),
            Err(RenderError::InvalidResource)
        ));
        world.despawn(entity)?;
    }
    Ok(())
}
#[test]
fn prepared_frames_hold_budget_until_last_owner_drops() -> Result<(), RenderError> {
    let mut assets = AssetRegistry::new(AssetLimits {
        managed_cpu_bytes: 256,
        ..Default::default()
    })?;
    let mesh = assets.register_mesh(triangle()?, vec![0, 1, 2])?;
    let material = assets.register_material([1.0; 4])?;
    let mut world = SceneWorld::new(SceneLimits::default())?;
    world.spawn(Transform::default(), binding(mesh, material), None)?;
    let prepared = assets.prepare(world.snapshot()?)?;
    assets.remove_mesh(mesh)?;
    let held = assets.managed_bytes();
    assert!(held > 16);
    assert_eq!(
        assets.register_mesh(triangle()?, vec![0, 1, 2]),
        Err(RenderError::BudgetExceeded)
    );
    drop(prepared);
    assert_eq!(assets.managed_bytes(), 16);
    let replacement = assets.register_mesh(triangle()?, vec![0, 1, 2])?;
    assert_ne!(replacement, mesh);
    assets.remove_mesh(replacement)?;
    assets.remove_material(material)?;
    assert_eq!(assets.managed_bytes(), 0);
    Ok(())
}
