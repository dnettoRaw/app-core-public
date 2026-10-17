// =============================================================================
//        #######
//     ###       ###     F: assets.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::*;
pub fn frame() -> Result<PreparedScene, RenderError> {
    let mut assets = AssetRegistry::new(AssetLimits::default())?;
    let vertices = |positions: &[[f32; 3]]| {
        positions
            .iter()
            .map(|p| MeshVertex::new(*p, [0., 0., 1.]))
            .collect::<Result<Vec<_>, _>>()
    };
    let quad = assets.register_mesh(
        vertices(&[
            [-0.023, -0.023, 0.],
            [0.023, -0.023, 0.],
            [0.023, 0.023, 0.],
            [-0.023, 0.023, 0.],
        ])?,
        vec![0, 1, 2, 0, 2, 3],
    )?;
    let triangle = assets.register_mesh(
        vertices(&[[-0.008, -0.008, 0.], [0.008, -0.008, 0.], [0., 0.008, 0.]])?,
        vec![0, 1, 2],
    )?;
    let materials = [
        assets.register_material([1., 0.2, 0.1, 1.])?,
        assets.register_material([0.1, 0.5, 1., 1.])?,
    ];
    let mut world = SceneWorld::new(SceneLimits::default())?;
    for (batch, mesh) in [quad, triangle].into_iter().enumerate() {
        for i in 0..1024 {
            let position = [
                -0.93 + (i % 32) as f32 * 0.06 + if batch == 1 && i % 2 == 0 { 0.025 } else { 0. },
                -0.93 + (i / 32) as f32 * 0.06,
                if batch == 0 { 0.3 } else { 0.6 },
            ];
            world.spawn(
                Transform::default().with_position(position)?,
                RenderBinding {
                    mesh,
                    material: materials[batch],
                    tint: [1.; 4],
                    animation: 0,
                    morph: 0,
                },
                None,
            )?;
        }
    }
    assets.prepare(world.snapshot()?)
}
