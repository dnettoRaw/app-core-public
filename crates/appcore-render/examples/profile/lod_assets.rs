// =============================================================================
//        #######
//     ###       ###     F: lod_assets.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::*;
pub fn frame() -> Result<PreparedScene, RenderError> {
    let mut assets = AssetRegistry::new(AssetLimits::default())?;
    let positions = [
        [
            [-0.023, -0.023, 0.],
            [0.023, -0.023, 0.],
            [0.023, 0.023, 0.],
            [-0.023, 0.023, 0.],
        ],
        [
            [0., -0.023, 0.],
            [0.023, 0., 0.],
            [0., 0.023, 0.],
            [-0.023, 0., 0.],
        ],
    ];
    let mut fine = Vec::new();
    let mut coarse = Vec::new();
    for points in positions {
        let vertices = || {
            points
                .into_iter()
                .map(|p| MeshVertex::new(p, [0., 0., 1.]))
                .collect::<Result<Vec<_>, _>>()
        };
        fine.push(assets.register_mesh(vertices()?, vec![0, 1, 2, 0, 2, 3])?);
        coarse.push(assets.register_mesh(vertices()?, vec![0, 1, 2])?);
    }
    let material = assets.register_material([0.2, 0.7, 1., 1.])?;
    let binding = |mesh| RenderBinding {
        mesh,
        material,
        tint: [1.; 4],
        animation: 0,
        morph: 0,
    };
    let mut world = SceneWorld::new(SceneLimits::default())?;
    for (batch, mesh) in fine.into_iter().enumerate() {
        for i in 0..1024 {
            world.spawn(
                Transform::default().with_position([
                    -0.93 + (i % 32) as f32 * 0.06 + batch as f32 * 0.025,
                    -0.93 + (i / 32) as f32 * 0.06,
                    if (i + batch) % 2 == 0 { 2. } else { 12. },
                ])?,
                binding(mesh),
                None,
            )?;
        }
    }
    for mesh in coarse {
        world.spawn(Transform::default(), binding(mesh), None)?;
    }
    assets.prepare(world.snapshot()?)
}
