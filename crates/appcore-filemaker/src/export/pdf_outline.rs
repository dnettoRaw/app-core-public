// =============================================================================
//        #######
//     ###       ###     F: pdf_outline.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use pdf_writer::Content;
use skrifa::outline::OutlinePen;

pub(super) struct PdfOutline<'a> {
    pub(super) content: &'a mut Content,
    origin_x: f32,
    origin_y: f32,
    scale: f32,
    current: (f32, f32),
}

impl<'a> PdfOutline<'a> {
    pub(super) fn new(content: &'a mut Content, origin_x: f32, origin_y: f32, scale: f32) -> Self {
        Self {
            content,
            origin_x,
            origin_y,
            scale,
            current: (0.0, 0.0),
        }
    }

    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.origin_x + x * self.scale,
            self.origin_y + y * self.scale,
        )
    }
}

impl OutlinePen for PdfOutline<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.current = (x, y);
        let point = self.point(x, y);
        self.content.move_to(point.0, point.1);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current = (x, y);
        let point = self.point(x, y);
        self.content.line_to(point.0, point.1);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x0, y0) = self.current;
        let c1 = self.point(x0 + (x1 - x0) * 2.0 / 3.0, y0 + (y1 - y0) * 2.0 / 3.0);
        let c2 = self.point(x + (x1 - x) * 2.0 / 3.0, y + (y1 - y) * 2.0 / 3.0);
        let end = self.point(x, y);
        self.content.cubic_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
        self.current = (x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = self.point(x1, y1);
        let c2 = self.point(x2, y2);
        let end = self.point(x, y);
        self.content.cubic_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
        self.current = (x, y);
    }

    fn close(&mut self) {
        self.content.close_path();
    }
}
