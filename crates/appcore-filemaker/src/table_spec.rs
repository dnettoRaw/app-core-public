// =============================================================================
//        #######
//     ###       ###     F: table_spec.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Table specification validation, styles, and bounded dataset visitation.

use super::*;

impl TableSpec {
    /// Validates bounded table structure.
    pub fn validate(&self) -> Result<()> {
        let fields: std::collections::BTreeSet<_> =
            self.columns.iter().map(|column| &column.field).collect();
        let total_fields: std::collections::BTreeSet<_> = self.total_fields.iter().collect();
        if self.columns.is_empty()
            || self.columns.len() > 1_024
            || self.auto_sample_rows == 0
            || self.max_rows == 0
            || self.style_expression_steps == 0
            || self.max_row_fields == 0
            || self.max_cell_bytes == 0
            || self.columns.len() > self.max_row_fields
            || self.columns.iter().any(|column| column.field.is_empty())
            || self.columns.iter().any(|column| !column.padding.is_valid())
            || fields.len() != self.columns.len()
            || self
                .group_by
                .as_ref()
                .is_some_and(|field| !fields.contains(field))
            || self
                .keep_together_by
                .as_ref()
                .is_some_and(|field| field.is_empty())
            || self
                .row_anchor_field
                .as_ref()
                .is_some_and(|field| field.is_empty())
            || (self.row_anchor_field.is_none()
                && self
                    .conditional_styles
                    .iter()
                    .any(|rule| rule.reserve_after.is_some()))
            || self
                .total_fields
                .iter()
                .any(|field| !fields.contains(field))
            || total_fields.len() != self.total_fields.len()
            || self.conditional_styles.len() > 1_024
            || self.conditional_styles.iter().any(|rule| {
                rule.when.is_empty()
                    || [
                        rule.padding,
                        rule.padding_first_page,
                        rule.padding_continuation,
                    ]
                    .into_iter()
                    .flatten()
                    .any(|padding| !padding.is_valid())
                    || rule
                        .text_offset_y
                        .into_iter()
                        .chain(rule.text_offset_y_first_page)
                        .chain(rule.text_offset_y_continuation)
                        .any(|offset| matches!(offset, Length::Percent(_) | Length::Auto))
                    || [
                        rule.min_height,
                        rule.min_height_first_page,
                        rule.min_height_continuation,
                    ]
                    .into_iter()
                    .flatten()
                    .any(|height| match height {
                        Length::Absolute(value) => value <= crate::Unit::ZERO,
                        Length::Logical(value) => value <= 0,
                        Length::Percent(value) => value <= 0,
                        Length::Auto => true,
                    })
                    || rule.reserve_after.is_some_and(|reserve| {
                        !matches!(reserve, Length::Absolute(value) if value > crate::Unit::ZERO)
                    })
                    || rule.padding.is_some_and(|padding| !padding.is_valid())
            })
            || self
                .columns
                .iter()
                .any(|column| matches!(column.width, ColumnWidth::Flex(0)))
        {
            return Err(table_error("table specification is invalid"));
        }
        for rule in &self.conditional_styles {
            Expression::parse(&rule.when)?;
            rule.style.validate()?;
        }
        Ok(())
    }

    /// Computes the ordered conditional style layers for one row.
    pub fn style_for(&self, row: &DataRow) -> Result<Style> {
        let root = DataValue::Object(row.clone());
        let mut computed = Style::default();
        let mut budget = ExpressionBudget::new(self.style_expression_steps)?;
        for rule in &self.conditional_styles {
            if Expression::parse(&rule.when)?
                .evaluate(&root, &mut budget)?
                .is_truthy()
            {
                rule.style.validate()?;
                computed.overlay(&rule.style);
            }
        }
        Ok(computed)
    }

    /// Resolves the last matching conditional per-row cell padding.
    pub(crate) fn padding_for(&self, row: &DataRow, page_index: usize) -> Result<CellPadding> {
        let root = DataValue::Object(row.clone());
        let mut padding = CellPadding::default();
        let mut budget = ExpressionBudget::new(self.style_expression_steps)?;
        for rule in &self.conditional_styles {
            let page_padding = if page_index == 0 {
                rule.padding_first_page.or(rule.padding)
            } else {
                rule.padding_continuation.or(rule.padding)
            };
            if page_padding.is_some()
                && Expression::parse(&rule.when)?
                    .evaluate(&root, &mut budget)?
                    .is_truthy()
            {
                if let Some(next) = page_padding {
                    padding = next;
                }
            }
        }
        Ok(padding)
    }

    /// Resolves the last matching paint-only vertical text offset.
    pub(crate) fn text_offset_y_for(
        &self,
        row: &DataRow,
        logical_unit: crate::Unit,
        page_index: usize,
    ) -> Result<crate::Unit> {
        let root = DataValue::Object(row.clone());
        let mut offset = crate::Unit::ZERO;
        let mut budget = ExpressionBudget::new(self.style_expression_steps)?;
        for rule in &self.conditional_styles {
            let page_offset = if page_index == 0 {
                rule.text_offset_y_first_page
            } else {
                rule.text_offset_y_continuation
            };
            if let Some(value) = page_offset.or(rule.text_offset_y) {
                if Expression::parse(&rule.when)?
                    .evaluate(&root, &mut budget)?
                    .is_truthy()
                {
                    offset = match value {
                        Length::Absolute(value) => value,
                        Length::Logical(value) => logical_unit.checked_scale(value)?,
                        Length::Percent(_) | Length::Auto => {
                            return Err(table_error(
                                "table text offset must be an absolute or logical length",
                            ));
                        }
                    };
                }
            }
        }
        Ok(offset)
    }

    /// Computes the largest conditional minimum height for one data row.
    pub(crate) fn minimum_row_height_for(
        &self,
        row: &DataRow,
        page_index: usize,
        reference: crate::Unit,
        logical_unit: crate::Unit,
    ) -> Result<Option<crate::Unit>> {
        let root = DataValue::Object(row.clone());
        let mut budget = ExpressionBudget::new(self.style_expression_steps)?;
        let mut minimum = None;
        for rule in &self.conditional_styles {
            let page_minimum = if page_index == 0 {
                rule.min_height_first_page
            } else {
                rule.min_height_continuation
            };
            if (rule.min_height.is_some() || page_minimum.is_some())
                && Expression::parse(&rule.when)?
                    .evaluate(&root, &mut budget)?
                    .is_truthy()
            {
                for height in [rule.min_height, page_minimum].into_iter().flatten() {
                    let Some(candidate) = height.resolve(reference, logical_unit)? else {
                        return Err(table_error("conditional minimum row height cannot be auto"));
                    };
                    minimum = Some(
                        minimum.map_or(candidate, |current: crate::Unit| current.max(candidate)),
                    );
                }
            }
        }
        Ok(minimum)
    }

    /// Resolves page capacity reserved after a row that publishes a named anchor.
    pub(crate) fn reserve_after_for(&self, row: &DataRow) -> Result<crate::Unit> {
        let Some(field) = self.row_anchor_field.as_deref() else {
            return Ok(crate::Unit::ZERO);
        };
        if !matches!(row.get(field), Some(DataValue::String(name)) if !name.is_empty()) {
            return Ok(crate::Unit::ZERO);
        }
        let root = DataValue::Object(row.clone());
        let mut budget = ExpressionBudget::new(self.style_expression_steps)?;
        self.conditional_styles
            .iter()
            .filter_map(|rule| rule.reserve_after.map(|reserve| (&rule.when, reserve)))
            .try_fold(crate::Unit::ZERO, |largest, (when, reserve)| {
                if Expression::parse(when)?
                    .evaluate(&root, &mut budget)?
                    .is_truthy()
                {
                    let Length::Absolute(candidate) = reserve else {
                        return Err(table_error(
                            "row anchor reserve must be an absolute positive length",
                        ));
                    };
                    Ok(largest.max(candidate))
                } else {
                    Ok(largest)
                }
            })
    }

    /// Streams rows with an enforced hard maximum.
    pub fn visit_bounded(
        &self,
        dataset: &dyn Dataset,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<()>,
    ) -> Result<()> {
        self.visit_bounded_until(dataset, &mut |index, row| {
            visitor(index, row)?;
            Ok(true)
        })
    }

    /// Streams rows until a bounded visitor asks to stop.
    pub fn visit_bounded_until(
        &self,
        dataset: &dyn Dataset,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
    ) -> Result<()> {
        self.validate()?;
        let mut row_anchors = std::collections::BTreeSet::new();
        dataset.visit_rows_until(&mut |index, row| {
            if index >= self.max_rows {
                return Err(FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    "dataset row limit exceeded",
                ));
            }
            if row.len() > self.max_row_fields
                || row.iter().any(|(field, value)| {
                    field.len() > self.max_cell_bytes
                        || value.display().len() > self.max_cell_bytes
                        || matches!(value, DataValue::Array(_) | DataValue::Object(_))
                })
            {
                return Err(FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    "dataset row exceeds its field, cell, or scalar-value limit",
                ));
            }
            if let Some(field) = &self.row_anchor_field {
                if let Some(value) = row.get(field) {
                    match value {
                        DataValue::Null => {}
                        DataValue::String(name)
                            if !name.is_empty()
                                && name.len() <= self.max_cell_bytes
                                && !name.contains('.')
                                && !name.contains('+')
                                && row_anchors.insert(name.clone()) => {}
                        _ => {
                            return Err(table_error(
                                "row anchor names must be unique bounded strings without anchor delimiters",
                            ));
                        }
                    }
                }
            }
            visitor(index, row)
        })
    }
}
