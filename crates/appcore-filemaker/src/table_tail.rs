// =============================================================================
//        #######
//     ###       ###     F: table_tail.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Final-group reservation integrated into the existing bounded paginator.
use crate::adaptive_layout::OversizedGroup;
use crate::{
    DataRow, Dataset, ErrorCode, FileMakerError, OperationControl, Result, TablePageSink,
    TablePaginator, TableSpec, Unit,
};

pub(crate) struct TailReservation {
    pub(crate) first: Unit,
    pub(crate) diagnostics: Option<crate::pagination_diagnostics::DecisionRecorder>,
    pub(crate) table: crate::ElementId,
    pub(crate) header_height: Unit,
    pub(crate) repeat_header: bool,
    pub(crate) continuation: Unit,
    pub(crate) oversized: OversizedGroup,
    pub(crate) control: OperationControl,
}
impl TailReservation {
    pub(crate) fn at(&self, page: usize) -> Unit {
        if page == 0 {
            self.first
        } else {
            self.continuation
        }
    }
}
impl TablePaginator {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paginate_with_tail(
        &self,
        spec: &TableSpec,
        dataset: &dyn Dataset,
        measure: &mut dyn FnMut(usize, &DataRow) -> Result<Unit>,
        sink: &mut dyn TablePageSink,
        first_page_height: Unit,
        page_sensitive: bool,
        tail: Option<&TailReservation>,
    ) -> Result<()> {
        if self.available_height <= Unit::ZERO
            || first_page_height <= Unit::ZERO
            || self.header_height < Unit::ZERO
            || self.max_pages == 0
        {
            return Err(FileMakerError::new(
                ErrorCode::DataType,
                "table pagination dimensions are invalid",
            ));
        }
        crate::table_group_pagination::paginate_measured(
            self,
            spec,
            dataset,
            measure,
            sink,
            first_page_height,
            page_sensitive,
            tail,
        )
    }
}
