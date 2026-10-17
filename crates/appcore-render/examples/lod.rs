// =============================================================================
//        #######
//     ###       ###     F: lod.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

//! CPU reference for the packed three-level geometry fixture; no graphics dependency.
use appcore_render::*;
fn main() -> Result<(), RenderError> {
    let mut levels = [
        LodLevel {
            index_count: 24,
            first_index: 0,
            base_vertex: 0,
            geometric_error: 0.,
            resident: true,
        },
        LodLevel {
            index_count: 6,
            first_index: 24,
            base_vertex: 9,
            geometric_error: 0.22,
            resident: true,
        },
        LodLevel {
            index_count: 3,
            first_index: 30,
            base_vertex: 13,
            geometric_error: 0.3,
            resident: true,
        },
    ];
    let view = LodView {
        eye: [0.; 3],
        forward: [0., 0., -1.],
        near_distance: 0.1,
        focal_pixels: 256.,
        viewport_half_diagonal: 363.,
        maximum_error_pixels: 32.,
    };
    let frustum = Frustum::from_matrix([
        1.,
        0.,
        0.,
        0.,
        0.,
        1.,
        0.,
        0.,
        0.,
        0.,
        100. / (-99.9),
        -1.,
        0.,
        0.,
        10. / (-99.9),
        0.,
    ])?;
    let objects = [[-1.2, 0., -2.], [0., 0., -5.], [6., 0., -10.]].map(|center| LodObject {
        bounds: BoundingSphere {
            center,
            radius: 0.5,
        },
        max_scale: 1.,
    });
    for resident in [true, false, true] {
        levels[1].resident = resident;
        let choices = objects
            .into_iter()
            .map(|object| choose_lod(&levels, object, view, frustum))
            .collect::<Result<Vec<_>, _>>()?;
        println!("middle_resident={resident} choices={choices:?}");
    }
    Ok(())
}
