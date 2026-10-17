// =============================================================================
//        #######
//     ###       ###     F: lod_assets.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use appcore_render::*;
type Result<T> = std::result::Result<T, RenderError>;
fn normalize(p: [f32; 3]) -> [f32; 3] {
    let n = p.iter().map(|v| v * v).sum::<f32>().sqrt();
    p.map(|v| v / n)
}
fn midpoint(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    normalize(std::array::from_fn(|i| (a[i] + b[i]) * 0.5))
}
fn octahedron() -> Vec<[[f32; 3]; 3]> {
    let p = [
        [1., 0., 0.],
        [-1., 0., 0.],
        [0., 1., 0.],
        [0., -1., 0.],
        [0., 0., 1.],
        [0., 0., -1.],
    ];
    [
        [2, 0, 4],
        [2, 4, 1],
        [2, 1, 5],
        [2, 5, 0],
        [3, 4, 0],
        [3, 1, 4],
        [3, 5, 1],
        [3, 0, 5],
    ]
    .map(|i| i.map(|i| p[i]))
    .to_vec()
}
fn mesh(assets: &mut AssetRegistry, faces: &[[[f32; 3]; 3]], smooth: bool) -> Result<ResourceId> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for face in faces {
        let a = std::array::from_fn::<_, 3, _>(|i| face[1][i] - face[0][i]);
        let b = std::array::from_fn::<_, 3, _>(|i| face[2][i] - face[0][i]);
        let mut n = normalize([
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]);
        if (0..3).map(|i| n[i] * face[0][i]).sum::<f32>() < 0. {
            n = n.map(|v| -v);
        }
        for p in face {
            indices.push(vertices.len() as u32);
            vertices.push(MeshVertex::new(*p, if smooth { normalize(*p) } else { n })?);
        }
    }
    assets.register_mesh(vertices, indices)
}
pub fn scene() -> Result<PreparedScene> {
    let mut assets = AssetRegistry::new(AssetLimits::default())?;
    let coarse = octahedron();
    let mut fine = coarse.clone();
    for _ in 0..3 {
        fine = fine
            .into_iter()
            .flat_map(|[a, b, c]| {
                let (ab, bc, ca) = (midpoint(a, b), midpoint(b, c), midpoint(c, a));
                [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]
            })
            .collect();
    }
    let sphere = mesh(&mut assets, &fine, true)?;
    let octa = mesh(&mut assets, &coarse, false)?;
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
    let mut cube = Vec::new();
    for [a, b, c, d] in [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 4, 7, 3],
        [1, 2, 6, 5],
        [3, 7, 6, 2],
        [0, 1, 5, 4],
    ] {
        cube.extend([[p[a], p[b], p[c]], [p[a], p[c], p[d]]]);
    }
    let cube = mesh(&mut assets, &cube, false)?;
    let p = [[1., 1., 1.], [-1., -1., 1.], [-1., 1., -1.], [1., -1., -1.]];
    let tetra = mesh(
        &mut assets,
        &[
            [p[0], p[1], p[2]],
            [p[0], p[3], p[1]],
            [p[0], p[2], p[3]],
            [p[1], p[3], p[2]],
        ],
        false,
    )?;
    let materials = [
        [0.12, 0.8, 1., 1.],
        [0.95, 0.35, 0.13, 1.],
        [0.65, 0.25, 1., 1.],
    ]
    .into_iter()
    .map(|color| assets.register_material(color))
    .collect::<Result<Vec<_>>>()?;
    let mut world = SceneWorld::new(SceneLimits::default())?;
    for (kind, count, id) in [(0, 16, sphere), (1, 8, cube)] {
        for z in 0..count {
            for x in 0..count {
                let spacing = if kind == 0 { 0.48 } else { 0.85 };
                let p = [
                    (x as f32 - (count - 1) as f32 * 0.5) * spacing,
                    if kind == 0 { -0.7 } else { 0.6 },
                    (z as f32 - (count - 1) as f32 * 0.5) * spacing,
                ];
                world.spawn(
                    Transform::new(p, [0., 0., 0., 1.], [0.18; 3])?,
                    RenderBinding {
                        mesh: id,
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
    for id in [octa, tetra] {
        world.spawn(
            Transform::default(),
            RenderBinding {
                mesh: id,
                material: materials[0],
                tint: [1.; 4],
                animation: 0,
                morph: 0,
            },
            None,
        )?;
    }
    assets.prepare(world.snapshot()?)
}
