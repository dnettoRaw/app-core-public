// =============================================================================
//        #######
//     ###       ###     F: mesh_camera.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

pub fn matrix(yaw: f32, pitch: f32, distance: f32, aspect: f32) -> [f32; 16] {
    let mut rx = identity();
    let (c, s) = (pitch.cos(), pitch.sin());
    rx[5] = c;
    rx[6] = s;
    rx[9] = -s;
    rx[10] = c;
    let mut ry = identity();
    let (c, s) = (yaw.cos(), yaw.sin());
    ry[0] = c;
    ry[2] = -s;
    ry[8] = s;
    ry[10] = c;
    let mut eye = identity();
    eye[14] = -distance;
    let f = 1.0 / (std::f32::consts::PI / 8.0).tan();
    let near = 0.1;
    let far = 100.0;
    let projection = [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        far / (near - far),
        -1.0,
        0.0,
        0.0,
        near * far / (near - far),
        0.0,
    ];
    multiply(projection, multiply(eye, multiply(rx, ry)))
}
fn identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            for k in 0..4 {
                out[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k];
            }
        }
    }
    out
}
