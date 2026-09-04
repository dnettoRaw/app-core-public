// =============================================================================
//        #######
//     ###       ###     F: resolved_table.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use serde::{Deserialize, Serialize};

use crate::{ComputedStyle, Rect, ResolvedTableColumn, TextLayout};

/// One exporter-ready table cell with final geometry and shaped text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedTableCell {
    /// Stable source field.
    pub field: String,
    /// Display text derived before export.
    pub text: String,
    /// Cell rectangle in page coordinates.
    pub bounds: Rect,
    /// Final style after the data-rule layer.
    pub style: ComputedStyle,
    /// Shaped text constrained to the cell.
    pub text_layout: TextLayout,
}

/// One exporter-ready table row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedTableRow {
    /// Zero-based source row index across fragments.
    pub source_index: u64,
    /// Row rectangle in page coordinates.
    pub bounds: Rect,
    /// Group key when this row begins a group.
    pub group_start: Option<String>,
    /// Final row style after ordered conditional rules.
    pub style: ComputedStyle,
    /// Cells in visual column order.
    pub cells: Vec<ResolvedTableCell>,
}

/// One physical-page fragment of a first-class table.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedTableFragment {
    /// Zero-based fragment index.
    pub index: usize,
    /// Final column widths shared by every fragment.
    pub columns: Vec<ResolvedTableColumn>,
    /// Header cells, empty when the header is not repeated.
    pub header: Vec<ResolvedTableCell>,
    /// Bounded rows assigned to this fragment.
    pub rows: Vec<ResolvedTableRow>,
    /// Final exact totals row, empty before the last fragment.
    pub totals: Vec<ResolvedTableCell>,
    /// Group active at the first row of this fragment.
    pub starting_group: Option<String>,
}
