// =============================================================================
//        #######
//     ###       ###     F: mesh_assets.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use appcore_render::*;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn mesh(assets: &mut AssetRegistry, faces: &[[[f32; 3]; 3]]) -> Result<ResourceId> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for face in faces {
        let a = std::array::from_fn::<_, 3, _>(|i| face[1][i] - face[0][i]);
        let b = std::array::from_fn::<_, 3, _>(|i| face[2][i] - face[0][i]);
        let mut n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let length = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        for component in &mut n {
            *component /= length;
        }
        for position in face {
            indices.push(vertices.len() as u32);
            vertices.push(MeshVertex::new(*position, n)?);
        }
    }
    Ok(assets.register_mesh(vertices, indices)?)
}
pub fn scene() -> Result<PreparedScene> {
    let mut assets = AssetRegistry::new(AssetLimits::default())?;
    let p = [
        [-1., -1., -1.],
        [1., -1., -1.],
        [1., 1., -1.],
        [-1., 1., -1.],
        [-1., -1., 1.],
        [1., -1., 1.],
        [1., 1., 1.],
        [-1., 1., 1.],
    ];
    let mut faces = Vec::new();
    for [a, b, c, d] in [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 4, 7, 3],
        [1, 2, 6, 5],
        [3, 7, 6, 2],
        [0, 1, 5, 4],
    ] {
        faces.push([p[a], p[b], p[c]]);
        faces.push([p[a], p[c], p[d]]);
    }
    let cube = mesh(&mut assets, &faces)?;
    let p = [
        [-1., -1., -1.],
        [1., -1., -1.],
        [1., -1., 1.],
        [-1., -1., 1.],
        [0., 1., 0.],
    ];
    let faces = [
        [p[0], p[4], p[1]],
        [p[1], p[4], p[2]],
        [p[2], p[4], p[3]],
        [p[3], p[4], p[0]],
        [p[0], p[1], p[2]],
        [p[0], p[2], p[3]],
    ];
    let pyramid = mesh(&mut assets, &faces)?;
    let colors = [
        [0.12, 0.8, 1., 1.],
        [0.95, 0.35, 0.13, 1.],
        [0.65, 0.25, 1., 1.],
    ];
    let materials = colors
        .into_iter()
        .map(|c| assets.register_material(c))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut world = SceneWorld::new(SceneLimits::default())?;
    for (id, count, mesh_id) in [(0, 16, cube), (1, 8, pyramid)] {
        for z in 0..count {
            for x in 0..count {
                let spacing = if id == 0 { 0.48 } else { 0.85 };
                let position = [
                    (x as f32 - (count - 1) as f32 / 2.) * spacing,
                    if id == 0 { -0.7 } else { 0.6 },
                    (z as f32 - (count - 1) as f32 / 2.) * spacing,
                ];
                world.spawn(
                    Transform::new(position, [0., 0., 0., 1.], [0.18; 3])?,
                    RenderBinding {
                        mesh: mesh_id,
                        material: materials[(x + z) % 3],
                        tint: [1.; 4],
                        animation: 0,
                        morph: 0,
                    },
                    None,
                )?;
            }
        }
    }
    Ok(assets.prepare(world.snapshot()?)?)
}
