// =============================================================================
//        #######
//     ###       ###     F: export.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Exporter implementations consume only fully resolved scenes.

pub(crate) mod bounded_string;
mod core;
mod csv;
mod html;
mod markup;
mod pdf;
mod pdf_font;
mod pdf_geometry;
mod pdf_image;
mod pdf_outline;
mod pdf_paint;
mod pdf_render;
pub(crate) mod pdf_stream;
mod progress;
mod raster;
mod raster_encode;
mod raster_outline;
mod raster_plan;
mod raster_text;
mod svg;
mod table_html;
mod table_pdf;
mod table_raster;
mod table_svg;

pub(crate) use core::validate_request;
pub use core::{
    export, export_bytes, export_controlled, ExportCapabilities, ExportContext, ExportFormat,
    ExportLoss, ExportLossKind, ExportLossReport, ExportOutcome, ExportRequest,
    ExportStyleOverride, Fidelity, HtmlMode, PdfMode,
};
pub use csv::{export_dataset_csv, export_dataset_csv_bytes};
pub(crate) use raster_encode::encode_png_tiled;
pub(crate) use raster_plan::bounded_tile_rows;
