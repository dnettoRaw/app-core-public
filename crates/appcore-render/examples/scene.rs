// =============================================================================
//        #######
//     ###       ###     F: scene.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::{
    RenderBinding, RenderLimits, ResourceArena, SceneCommand, SceneCommands, SceneLimits,
    SceneWorld, Transform, Velocity,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Opaque asset identities only; this example qualifies CPU scene ownership, not drawing.
    let mut assets = ResourceArena::new(RenderLimits::default())?;
    let binding = RenderBinding {
        mesh: assets.insert((), 1)?,
        material: assets.insert((), 1)?,
        tint: [1.0; 4],
        animation: 0,
        morph: 0,
    };
    let mut world = SceneWorld::new(SceneLimits {
        max_entities: 64,
        max_snapshot_instances: 64,
        ..Default::default()
    })?;
    let mut first = None;
    for i in 0..64 {
        let id = world.spawn(
            Transform::default().with_position([i as f32, 0.0, 0.0])?,
            binding,
            Some(Velocity::new([1.0, 0.0, 0.0])?),
        )?;
        first = first.or(Some(id));
    }
    let before = world.snapshot()?;
    world.step_velocity(0.016)?;
    let after = world.snapshot()?;
    assert_eq!(before.instances()[0].transform.position()[0], 0.0);
    assert_eq!(after.instances()[0].transform.position()[0], 0.016);
    let mut commands = SceneCommands::new(8)?;
    if let Some(first) = first {
        commands.push(SceneCommand::SetVelocity(first, None))?;
    }
    for outcome in commands.apply(&mut world)? {
        outcome?;
    }
    println!(
        "entities={} owned_snapshots=2 moving_rows={}",
        world.len(),
        world.moving_columns().entities.len()
    );
    Ok(())
}
