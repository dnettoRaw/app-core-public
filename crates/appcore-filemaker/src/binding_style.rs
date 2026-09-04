// =============================================================================
//        #######
//     ###       ###     F: binding_style.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{DataValue, ElementIr, Expression, ExpressionBudget, ResourceLimits, Result};

pub(crate) fn apply_data_style_rules(
    element: &mut ElementIr,
    data: &DataValue,
    limits: &ResourceLimits,
) -> Result<()> {
    for rule in &element.style_rules {
        let mut budget = ExpressionBudget::new(limits.max_expression_steps)?;
        if Expression::parse(&rule.when)?
            .evaluate(data, &mut budget)?
            .is_truthy()
        {
            element.style.overlay(&rule.style);
        }
    }
    element.style_rules.clear();
    Ok(())
}
