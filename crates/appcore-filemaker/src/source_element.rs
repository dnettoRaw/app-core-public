// =============================================================================
//        #######
//     ###       ###     F: source_element.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{ElementIr, ElementSource, ErrorCode, FileMakerError, ResourceLimits, Result};

impl ElementSource {
    /// Validates and converts one static, self-contained source element to IR.
    ///
    /// This compact boundary is suitable for programmatic and tool-driven
    /// Canvas additions. Components, props, slots, named or conditional
    /// styles, bindings, conditions, and repeats require the complete
    /// [`crate::Compiler`] pipeline and are rejected here.
    pub fn to_ir(&self, limits: &ResourceLimits) -> Result<ElementIr> {
        validate_self_contained(self)?;
        crate::source_build::self_contained_element_to_ir(self, limits)
    }
}

fn validate_self_contained(root: &ElementSource) -> Result<()> {
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        if element.component.is_some()
            || !element.props.is_empty()
            || !element.slots.is_empty()
            || !element.styles.is_empty()
            || !element.style_rules.is_empty()
            || element.binding.is_some()
            || element.when.is_some()
            || element.repeat.is_some()
        {
            return Err(FileMakerError::new(
                ErrorCode::SchemaField,
                "compact element conversion does not run compiler expansion or data binding",
            )
            .at(element.id.clone()));
        }
        stack.extend(element.children.iter());
    }
    Ok(())
}
