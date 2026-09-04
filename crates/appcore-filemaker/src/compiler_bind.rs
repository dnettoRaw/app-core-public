// =============================================================================
//        #######
//     ###       ###     F: compiler_bind.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded compiler bind contracts and behavior for this crate.

use crate::{
    DataValue, ElementIr, ErrorCode, Expression, ExpressionBudget, FileMakerError,
    OperationControl, ProgressPhase, ResourceLimits, Result,
};

pub(crate) fn bind_elements(
    elements: &[ElementIr],
    data: &DataValue,
    limits: &ResourceLimits,
    control: &OperationControl,
) -> Result<Vec<ElementIr>> {
    let mut count = 0_usize;
    bind_list(elements, data, limits, control, &mut count)
}

fn bind_list(
    elements: &[ElementIr],
    data: &DataValue,
    limits: &ResourceLimits,
    control: &OperationControl,
    count: &mut usize,
) -> Result<Vec<ElementIr>> {
    let mut result = Vec::new();
    for element in elements {
        if let Some(repeat) = &element.repeat {
            let mut budget = ExpressionBudget::new(limits.max_expression_steps)?;
            let value = Expression::parse(repeat)?.evaluate(data, &mut budget)?;
            let DataValue::Array(items) = value else {
                return Err(FileMakerError::new(
                    ErrorCode::DataType,
                    "repeat expression must return an array",
                ));
            };
            for (index, item) in items.iter().enumerate() {
                reserve_element(count, limits, control)?;
                let mut clone = element.clone();
                prefix_ir_id(&mut clone, &index.to_string())?;
                clone.repeat = None;
                bind_element(&mut clone, item, limits, control, count)?;
                result.push(clone);
            }
        } else {
            reserve_element(count, limits, control)?;
            let mut clone = element.clone();
            bind_element(&mut clone, data, limits, control, count)?;
            result.push(clone);
        }
    }
    Ok(result)
}

fn bind_element(
    element: &mut ElementIr,
    data: &DataValue,
    limits: &ResourceLimits,
    control: &OperationControl,
    count: &mut usize,
) -> Result<()> {
    if let Some(condition) = &element.when {
        let mut budget = ExpressionBudget::new(limits.max_expression_steps)?;
        element.hidden |= !Expression::parse(condition)?
            .evaluate(data, &mut budget)?
            .is_truthy();
    }
    crate::binding_style::apply_data_style_rules(element, data, limits)?;
    if let Some(binding) = &element.binding {
        let mut budget = ExpressionBudget::new(limits.max_expression_steps)?;
        let value = Expression::parse(binding)?.evaluate(data, &mut budget)?;
        if let Some(table) = &mut element.table {
            bind_table(table, value, control)?;
        } else {
            let text = value.display();
            if text.len() > limits.max_text_bytes {
                return Err(limit_error("bound text exceeds configured limit"));
            }
            element.text = Some(text);
        }
    }
    element.children = bind_list(&element.children, data, limits, control, count)?;
    Ok(())
}

fn bind_table(
    table: &mut crate::TableIr,
    value: DataValue,
    control: &OperationControl,
) -> Result<()> {
    let DataValue::Array(items) = value else {
        return Err(FileMakerError::new(
            ErrorCode::DataType,
            "table binding must resolve to an array of objects",
        ));
    };
    if u64::try_from(items.len()).unwrap_or(u64::MAX) > table.spec.max_rows {
        return Err(limit_error("bound table exceeds its row limit"));
    }
    let mut rows = Vec::with_capacity(items.len());
    for item in items {
        control.cancellation().check()?;
        let DataValue::Object(row) = item else {
            return Err(FileMakerError::new(
                ErrorCode::DataType,
                "every bound table row must be an object",
            ));
        };
        rows.push(row);
    }
    let dataset = crate::InMemoryDataset { rows };
    table
        .spec
        .visit_bounded(&dataset, &mut |_, _| control.cancellation().check())?;
    table.rows = dataset.rows;
    Ok(())
}

fn reserve_element(
    count: &mut usize,
    limits: &ResourceLimits,
    control: &OperationControl,
) -> Result<()> {
    *count = count
        .checked_add(1)
        .ok_or_else(|| limit_error("bound element count overflow"))?;
    if *count > limits.max_elements {
        return Err(limit_error("bound elements exceed configured limit"));
    }
    control.checkpoint(
        ProgressPhase::BindElements,
        u64::try_from(*count).unwrap_or(u64::MAX),
        u64::try_from(limits.max_elements).ok(),
    )
}

fn prefix_ir_id(element: &mut ElementIr, prefix: &str) -> Result<()> {
    element.id = crate::ElementId::new(format!("{}/{prefix}", element.id.as_str()))?;
    for child in &mut element.children {
        prefix_ir_id(child, prefix)?;
    }
    Ok(())
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}
