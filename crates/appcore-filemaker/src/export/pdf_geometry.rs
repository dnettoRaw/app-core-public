// =============================================================================
//        #######
//     ###       ###     F: pdf_geometry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use super::pdf_font::unit;

pub(super) fn transform(transform: crate::Transform, page_height: f32) -> [f32; 6] {
    let a = transform.a as f32 / 1_000_000.0;
    let b = transform.b as f32 / 1_000_000.0;
    let c = transform.c as f32 / 1_000_000.0;
    let d = transform.d as f32 / 1_000_000.0;
    [
        a,
        -b,
        -c,
        d,
        c * page_height + unit(transform.tx),
        (1.0 - d) * page_height - unit(transform.ty),
    ]
}
