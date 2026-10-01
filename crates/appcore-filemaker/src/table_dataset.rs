// =============================================================================
//        #######
//     ###       ###     F: table_dataset.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Bounded in-memory, borrowed, and restartable table datasets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::table_error;
use crate::{DataValue, Result};

/// One deterministically ordered tabular row.
pub type DataRow = BTreeMap<String, DataValue>;

/// Restartable bounded dataset contract.
pub trait Dataset: Send + Sync {
    /// Optional exact row count.
    fn row_count_hint(&self) -> Option<u64>;
    /// Visits rows in stable order until the visitor returns `false`.
    fn visit_rows_until(
        &self,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
    ) -> Result<()>;

    /// Visits every row without requiring full materialization.
    fn visit_rows(&self, visitor: &mut dyn FnMut(u64, &DataRow) -> Result<()>) -> Result<()> {
        self.visit_rows_until(&mut |index, row| {
            visitor(index, row)?;
            Ok(true)
        })
    }
}

/// In-memory dataset for small bounded inputs.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct InMemoryDataset {
    /// Rows in source order.
    pub rows: Vec<DataRow>,
}

/// Borrowed dataset avoiding a duplicate row allocation for existing slices.
pub struct BorrowedDataset<'a> {
    rows: &'a [DataRow],
}

impl<'a> BorrowedDataset<'a> {
    /// Borrows rows in their existing stable source order.
    #[must_use]
    pub const fn new(rows: &'a [DataRow]) -> Self {
        Self { rows }
    }
}

impl Dataset for BorrowedDataset<'_> {
    fn row_count_hint(&self) -> Option<u64> {
        u64::try_from(self.rows.len()).ok()
    }

    fn visit_rows_until(
        &self,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
    ) -> Result<()> {
        visit_slice(self.rows, visitor)
    }
}

impl Dataset for InMemoryDataset {
    fn row_count_hint(&self) -> Option<u64> {
        u64::try_from(self.rows.len()).ok()
    }

    fn visit_rows_until(
        &self,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
    ) -> Result<()> {
        visit_slice(&self.rows, visitor)
    }
}

fn visit_slice(
    rows: &[DataRow],
    visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
) -> Result<()> {
    for (index, row) in rows.iter().enumerate() {
        let index = u64::try_from(index).map_err(|_| table_error("row index overflow"))?;
        if !visitor(index, row)? {
            break;
        }
    }
    Ok(())
}

/// Factory-backed dataset enabling restartable streaming.
pub struct StreamingDataset<F> {
    factory: F,
    row_count_hint: Option<u64>,
}

impl<F> StreamingDataset<F> {
    /// Creates a dataset from a factory returning a fresh iterator per visit.
    #[must_use]
    pub const fn new(factory: F, row_count_hint: Option<u64>) -> Self {
        Self {
            factory,
            row_count_hint,
        }
    }
}

impl<F, I> Dataset for StreamingDataset<F>
where
    F: Fn() -> I + Send + Sync,
    I: Iterator<Item = Result<DataRow>>,
{
    fn row_count_hint(&self) -> Option<u64> {
        self.row_count_hint
    }

    fn visit_rows_until(
        &self,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
    ) -> Result<()> {
        for (index, row) in (self.factory)().enumerate() {
            let row = row?;
            if !visitor(
                u64::try_from(index).map_err(|_| table_error("row index overflow"))?,
                &row,
            )? {
                break;
            }
        }
        Ok(())
    }
}
