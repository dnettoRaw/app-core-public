// =============================================================================
//        #######
//     ###       ###     F: validation_data.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded validation data contracts and behavior for this crate.

use crate::{
    DataValue, ElementIr, Expression, ExpressionBudget, ResourceLimits, TemplateIr, ValidationCode,
    ValidationReport, ValidationSeverity,
};

pub(crate) fn inspect_template(
    template: &TemplateIr,
    limits: &ResourceLimits,
    report: &mut ValidationReport,
) {
    if let Err(error) = template.validate(limits.max_elements) {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Contract,
            None,
            None,
            error.to_string(),
            1_000,
        );
    }
    inspect_schema_expressions(template, report);
    inspect_element_expressions(&template.elements, report);
}

pub(crate) fn inspect_data(
    template: &TemplateIr,
    data: &DataValue,
    limits: &ResourceLimits,
    report: &mut ValidationReport,
) {
    if let Err(error) = data.validate(64, limits.max_elements, limits.max_text_bytes) {
        push_data_error(report, None, error.to_string());
        return;
    }
    let resolved = match crate::resolve_computed_fields(
        &template.data_schema,
        data,
        limits.max_expression_steps,
    ) {
        Ok(resolved) => resolved,
        Err(error) => {
            push_data_error(report, None, error.to_string());
            return;
        }
    };
    inspect_bound_elements(&template.elements, &resolved, limits, report);
}

fn inspect_schema_expressions(template: &TemplateIr, report: &mut ValidationReport) {
    for (name, field) in &template.data_schema {
        let Some(source) = &field.computed else {
            continue;
        };
        match Expression::parse(source) {
            Ok(expression) => inspect_dependencies(
                expression.dependencies(),
                &template.data_schema,
                Some(name),
                report,
            ),
            Err(error) => push_binding_error(report, Some(name), error.to_string()),
        }
    }
    let mut resolved = std::collections::BTreeSet::new();
    let mut visiting = std::collections::BTreeSet::new();
    for name in template.data_schema.keys() {
        if let Some(cycle) =
            find_schema_cycle(name, &template.data_schema, &mut resolved, &mut visiting)
        {
            push_binding_error(
                report,
                Some(&cycle),
                format!("computed data dependency cycle includes `{cycle}`"),
            );
            break;
        }
    }
}

fn find_schema_cycle(
    name: &str,
    schema: &crate::DataSchema,
    resolved: &mut std::collections::BTreeSet<String>,
    visiting: &mut std::collections::BTreeSet<String>,
) -> Option<String> {
    if resolved.contains(name) {
        return None;
    }
    let field = schema.get(name)?;
    let source = field.computed.as_ref()?;
    if !visiting.insert(name.to_owned()) {
        return Some(name.to_owned());
    }
    if let Ok(expression) = Expression::parse(source) {
        for dependency in expression.dependencies() {
            if schema
                .get(&dependency)
                .is_some_and(|field| field.computed.is_some())
            {
                if let Some(cycle) = find_schema_cycle(&dependency, schema, resolved, visiting) {
                    return Some(cycle);
                }
            }
        }
    }
    visiting.remove(name);
    resolved.insert(name.to_owned());
    None
}

fn inspect_element_expressions(elements: &[ElementIr], report: &mut ValidationReport) {
    let mut stack = elements.iter().collect::<Vec<_>>();
    while let Some(element) = stack.pop() {
        for source in [&element.binding, &element.when, &element.repeat]
            .into_iter()
            .flatten()
            .chain(element.style_rules.iter().map(|rule| &rule.when))
        {
            match Expression::parse(source) {
                Ok(_) => {}
                Err(error) => {
                    push_binding_error(report, Some(element.id.as_str()), error.to_string())
                }
            }
        }
        stack.extend(element.children.iter());
    }
}

fn inspect_dependencies(
    dependencies: Vec<String>,
    schema: &crate::DataSchema,
    owner: Option<&str>,
    report: &mut ValidationReport,
) {
    for dependency in dependencies {
        if !schema.contains_key(&dependency) {
            push_binding_error(
                report,
                owner,
                format!("expression references undeclared data field `{dependency}`"),
            );
        }
    }
}

fn inspect_bound_elements(
    elements: &[ElementIr],
    data: &DataValue,
    limits: &ResourceLimits,
    report: &mut ValidationReport,
) {
    for element in elements {
        if let Some(repeat) = &element.repeat {
            match evaluate(repeat, data, limits) {
                Ok(DataValue::Array(items)) => {
                    for item in items {
                        inspect_bound_element(element, &item, limits, report);
                    }
                }
                Ok(_) => push_data_error(
                    report,
                    Some(element.id.as_str()),
                    "repeat expression must return an array",
                ),
                Err(error) => {
                    push_binding_error(report, Some(element.id.as_str()), error.to_string())
                }
            }
        } else {
            inspect_bound_element(element, data, limits, report);
        }
    }
}

fn inspect_bound_element(
    element: &ElementIr,
    data: &DataValue,
    limits: &ResourceLimits,
    report: &mut ValidationReport,
) {
    for source in [&element.when, &element.binding]
        .into_iter()
        .flatten()
        .chain(element.style_rules.iter().map(|rule| &rule.when))
    {
        if let Err(error) = evaluate(source, data, limits) {
            push_binding_error(report, Some(element.id.as_str()), error.to_string());
        }
    }
    if let Some(binding) = &element.binding {
        if let Ok(value) = evaluate(binding, data, limits) {
            if element.table.is_some()
                && !matches!(value, DataValue::Array(ref rows) if rows.iter().all(|row| matches!(row, DataValue::Object(_))))
            {
                push_data_error(
                    report,
                    Some(element.id.as_str()),
                    "table binding must resolve to an array of objects",
                );
            }
        }
    }
    inspect_bound_elements(&element.children, data, limits, report);
}

fn evaluate(source: &str, data: &DataValue, limits: &ResourceLimits) -> crate::Result<DataValue> {
    Expression::parse(source)?.evaluate(
        data,
        &mut ExpressionBudget::new(limits.max_expression_steps)?,
    )
}

fn push_binding_error(report: &mut ValidationReport, element: Option<&str>, message: String) {
    report.push(
        ValidationSeverity::Error,
        ValidationCode::Binding,
        None,
        element,
        message,
        1_000,
    );
}

fn push_data_error(
    report: &mut ValidationReport,
    element: Option<&str>,
    message: impl Into<String>,
) {
    report.push(
        ValidationSeverity::Error,
        ValidationCode::Data,
        None,
        element,
        message,
        1_000,
    );
}
