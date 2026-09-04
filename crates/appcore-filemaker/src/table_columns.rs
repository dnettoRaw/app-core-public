// =============================================================================
//        #######
//     ###       ###     F: table_columns.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded table columns contracts and behavior for this crate.

use crate::{ColumnWidth, Dataset, ErrorCode, FileMakerError, Result, TableSpec, Unit};

/// One exporter-neutral table column with a fixed resolved width.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedTableColumn {
    /// Stable data field.
    pub field: String,
    /// Header text.
    pub header: String,
    /// Final fixed-point width.
    pub width: Unit,
}

/// Resolves fixed, bounded-auto, and weighted-flex columns in declaration order.
pub fn resolve_table_columns(
    spec: &TableSpec,
    dataset: &dyn Dataset,
    available_width: Unit,
    logical_unit: Unit,
    measure: &mut dyn FnMut(&str) -> Result<Unit>,
) -> Result<Vec<ResolvedTableColumn>> {
    spec.validate()?;
    if available_width <= Unit::ZERO || logical_unit <= Unit::ZERO {
        return Err(column_error("table column dimensions must be positive"));
    }
    let mut widths = Vec::with_capacity(spec.columns.len());
    let mut flex_total = 0_u64;
    for column in &spec.columns {
        let width = match column.width {
            ColumnWidth::Fixed(length) => length
                .resolve(available_width, logical_unit)?
                .ok_or_else(|| column_error("fixed table column cannot be auto"))?,
            ColumnWidth::Auto => checked_measure(measure, &column.header)?,
            ColumnWidth::Flex(weight) => {
                flex_total = flex_total
                    .checked_add(u64::from(weight))
                    .ok_or_else(|| column_error("table flex weight overflow"))?;
                Unit::ZERO
            }
        };
        if !matches!(column.width, ColumnWidth::Flex(_)) && width <= Unit::ZERO {
            return Err(column_error(
                "fixed and automatic column widths must be positive",
            ));
        }
        widths.push(width);
    }
    if spec
        .columns
        .iter()
        .any(|column| matches!(column.width, ColumnWidth::Auto))
    {
        spec.visit_bounded_until(dataset, &mut |index, row| {
            for (column_index, column) in spec.columns.iter().enumerate() {
                if matches!(column.width, ColumnWidth::Auto) {
                    let value = row
                        .get(&column.field)
                        .map_or_else(String::new, crate::DataValue::display);
                    widths[column_index] =
                        widths[column_index].max(checked_measure(measure, &value)?);
                }
            }
            Ok(index + 1 < u64::try_from(spec.auto_sample_rows).unwrap_or(u64::MAX))
        })?;
    }
    let fixed_total = widths
        .iter()
        .try_fold(Unit::ZERO, |total, width| total.checked_add(*width))?;
    let remaining = available_width.checked_sub(fixed_total)?;
    if remaining < Unit::ZERO {
        return Err(column_error(
            "fixed and automatic columns exceed table width",
        ));
    }
    if flex_total > 0 {
        distribute_flex(spec, &mut widths, remaining, flex_total)?;
    }
    if widths.iter().any(|width| *width <= Unit::ZERO) {
        return Err(column_error(
            "resolved table columns must have positive width",
        ));
    }
    Ok(spec
        .columns
        .iter()
        .zip(widths)
        .map(|(column, width)| ResolvedTableColumn {
            field: column.field.clone(),
            header: column.header.clone(),
            width,
        })
        .collect())
}

fn distribute_flex(
    spec: &TableSpec,
    widths: &mut [Unit],
    remaining: Unit,
    flex_total: u64,
) -> Result<()> {
    let last_flex = spec
        .columns
        .iter()
        .rposition(|column| matches!(column.width, ColumnWidth::Flex(_)))
        .ok_or_else(|| column_error("flex total has no flex column"))?;
    let mut assigned = Unit::ZERO;
    for (index, column) in spec.columns.iter().enumerate() {
        let ColumnWidth::Flex(weight) = column.width else {
            continue;
        };
        let width = if index == last_flex {
            remaining.checked_sub(assigned)?
        } else {
            Unit::from_ratio(
                i128::from(remaining.raw()) * i128::from(weight),
                i128::from(flex_total) * i128::from(Unit::PER_POINT),
            )?
        };
        widths[index] = width;
        assigned = assigned.checked_add(width)?;
    }
    Ok(())
}

fn checked_measure(measure: &mut dyn FnMut(&str) -> Result<Unit>, value: &str) -> Result<Unit> {
    let width = measure(value)?;
    if width < Unit::ZERO {
        return Err(column_error("table text measurement cannot be negative"));
    }
    Ok(width)
}

fn column_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}
