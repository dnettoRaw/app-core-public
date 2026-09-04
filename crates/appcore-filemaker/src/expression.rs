// =============================================================================
//        #######
//     ###       ###     F: expression.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded expression contracts and behavior for this crate.

use crate::{DataValue, ErrorCode, FileMakerError, Result};

/// A parsed bounded expression without IO or arbitrary evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression {
    source: String,
}

impl Expression {
    /// Parses a supported deterministic expression.
    pub fn parse(source: impl Into<String>) -> Result<Self> {
        let source = source.into();
        if source.is_empty() || source.len() > 4_096 {
            return Err(expression_error("expression is empty or too long"));
        }
        if source.contains(['(', ')', ';', '`']) {
            return Err(expression_error(
                "functions and arbitrary evaluation are not supported",
            ));
        }
        Ok(Self { source })
    }

    /// Returns the original normalized expression.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns root data fields referenced by the expression in lexical order.
    ///
    /// This is used to build the computed-field dependency graph without
    /// evaluating the expression or consulting external state.
    pub fn dependencies(&self) -> Vec<String> {
        let mut dependencies = std::collections::BTreeSet::new();
        collect_dependencies(self.source.trim(), &mut dependencies);
        dependencies.into_iter().collect()
    }

    /// Evaluates path lookup, literals, comparisons, boolean operators, and concatenation.
    pub fn evaluate(&self, root: &DataValue, budget: &mut ExpressionBudget) -> Result<DataValue> {
        evaluate_expression(self.source.trim(), root, budget)
    }
}

fn collect_dependencies(source: &str, dependencies: &mut std::collections::BTreeSet<String>) {
    for operator in ["||", "&&", "==", "!=", "+"] {
        if let Some((left, right)) = split_operator(source, operator) {
            collect_dependencies(left.trim(), dependencies);
            collect_dependencies(right.trim(), dependencies);
            return;
        }
    }
    let atom = source.trim();
    if atom.is_empty()
        || atom.starts_with('"')
        || matches!(atom, "true" | "false" | "null")
        || atom.parse::<i64>().is_ok()
    {
        return;
    }
    let path = atom.strip_prefix("data.").unwrap_or(atom);
    if let Some(root) = path.split('.').next().filter(|root| !root.is_empty()) {
        dependencies.insert(root.to_owned());
    }
}

/// Per-expression operation budget.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpressionBudget {
    remaining: usize,
}

impl ExpressionBudget {
    /// Creates a non-zero operation budget.
    pub fn new(max_steps: usize) -> Result<Self> {
        if max_steps == 0 {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "expression step budget must be non-zero",
            ));
        }
        Ok(Self {
            remaining: max_steps,
        })
    }

    fn step(&mut self) -> Result<()> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            FileMakerError::new(ErrorCode::LimitExceeded, "expression step budget exceeded")
        })?;
        Ok(())
    }
}

fn evaluate_expression(
    source: &str,
    root: &DataValue,
    budget: &mut ExpressionBudget,
) -> Result<DataValue> {
    budget.step()?;
    if let Some((left, right)) = split_operator(source, "||") {
        return Ok(DataValue::Boolean(
            evaluate_expression(left, root, budget)?.is_truthy()
                || evaluate_expression(right, root, budget)?.is_truthy(),
        ));
    }
    if let Some((left, right)) = split_operator(source, "&&") {
        return Ok(DataValue::Boolean(
            evaluate_expression(left, root, budget)?.is_truthy()
                && evaluate_expression(right, root, budget)?.is_truthy(),
        ));
    }
    if let Some((left, right)) = split_operator(source, "==") {
        return Ok(DataValue::Boolean(
            evaluate_expression(left, root, budget)? == evaluate_expression(right, root, budget)?,
        ));
    }
    if let Some((left, right)) = split_operator(source, "!=") {
        return Ok(DataValue::Boolean(
            evaluate_expression(left, root, budget)? != evaluate_expression(right, root, budget)?,
        ));
    }
    if let Some((left, right)) = split_operator(source, "+") {
        let left = evaluate_expression(left, root, budget)?;
        let right = evaluate_expression(right, root, budget)?;
        return add(left, right);
    }
    atom(source, root)
}

fn atom(source: &str, root: &DataValue) -> Result<DataValue> {
    let source = source.trim();
    if let Some(value) = source
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        return Ok(DataValue::String(value.to_owned()));
    }
    match source {
        "true" => return Ok(DataValue::Boolean(true)),
        "false" => return Ok(DataValue::Boolean(false)),
        "null" => return Ok(DataValue::Null),
        _ => {}
    }
    if let Ok(integer) = source.parse::<i64>() {
        return Ok(DataValue::Integer(integer));
    }
    let path = source.strip_prefix("data.").unwrap_or(source);
    root.get_path(path)
        .cloned()
        .ok_or_else(|| expression_error(format!("binding path `{path}` was not found")))
}

fn add(left: DataValue, right: DataValue) -> Result<DataValue> {
    match (left, right) {
        (DataValue::Integer(left), DataValue::Integer(right)) => left
            .checked_add(right)
            .map(DataValue::Integer)
            .ok_or_else(|| expression_error("integer expression overflow")),
        (left, right) => Ok(DataValue::String(format!(
            "{}{}",
            left.display(),
            right.display()
        ))),
    }
}

fn split_operator<'a>(source: &'a str, operator: &str) -> Option<(&'a str, &'a str)> {
    let mut quoted = false;
    let bytes = source.as_bytes();
    let operator = operator.as_bytes();
    let mut index = 0;
    while index + operator.len() <= bytes.len() {
        if bytes[index] == b'"' {
            quoted = !quoted;
        }
        if !quoted && &bytes[index..index + operator.len()] == operator {
            return Some((&source[..index], &source[index + operator.len()..]));
        }
        index += 1;
    }
    None
}

fn expression_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::DataType, message)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn evaluates_paths_and_bounded_operators() {
        let root = DataValue::Object(BTreeMap::from([
            ("name".to_owned(), DataValue::String("Ada".to_owned())),
            ("active".to_owned(), DataValue::Boolean(true)),
        ]));
        let expression = Expression::parse("data.name + \"!\"").unwrap();
        assert_eq!(
            expression
                .evaluate(&root, &mut ExpressionBudget::new(8).unwrap())
                .unwrap(),
            DataValue::String("Ada!".to_owned())
        );
        let condition = Expression::parse("active == true").unwrap();
        assert!(condition
            .evaluate(&root, &mut ExpressionBudget::new(8).unwrap())
            .unwrap()
            .is_truthy());
        assert_eq!(
            Expression::parse("data.name + active")
                .unwrap()
                .dependencies(),
            vec!["active".to_owned(), "name".to_owned()]
        );
    }
}
