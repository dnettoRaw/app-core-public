// =============================================================================
//        #######
//     ###       ###     F: ir_validation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Validates bounded document IR metadata.

use std::collections::BTreeSet;

use crate::{ElementIr, ErrorCode, ExclusionIr, FileMakerError, Length, Result, TemplateIr};

impl TemplateIr {
    /// Verifies global ID uniqueness and a caller-supplied element bound.
    pub fn validate(&self, max_elements: usize) -> Result<()> {
        let mut ids = BTreeSet::new();
        let mut count = self.exclusions.len();
        if count > max_elements {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                format!("element and exclusion count exceeds {max_elements}"),
            ));
        }
        for (name, exclusion) in &self.exclusions {
            validate_exclusion(name, exclusion)?;
        }
        let mut stack: Vec<&ElementIr> = self.elements.iter().rev().collect();
        while let Some(element) = stack.pop() {
            count = count.saturating_add(1);
            if count > max_elements {
                return Err(FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    format!("element count exceeds {max_elements}"),
                ));
            }
            if !ids.insert(element.id.as_str()) {
                return Err(FileMakerError::new(
                    ErrorCode::SchemaField,
                    format!("duplicate element ID `{}`", element.id.as_str()),
                ));
            }
            stack.extend(element.children.iter().rev());
        }
        Ok(())
    }
}

pub(crate) fn validate_exclusion(name: &str, exclusion: &ExclusionIr) -> Result<()> {
    validate_exclusion_name("exclusion", name, 118)?;
    validate_exclusion_name("exclusion group", &exclusion.group, 128)?;
    if exclusion.collides_with.len() > 64 {
        return Err(FileMakerError::new(
            ErrorCode::LimitExceeded,
            "exclusion collision-group list exceeds 64",
        ));
    }
    for group in &exclusion.collides_with {
        validate_exclusion_name("exclusion collision group", group, 128)?;
    }
    if [exclusion.x, exclusion.y, exclusion.width, exclusion.height].contains(&Length::Auto) {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "exclusion geometry cannot be auto",
        ));
    }
    Ok(())
}

fn validate_exclusion_name(label: &str, value: &str, max_bytes: usize) -> Result<()> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            format!("{label} name is invalid"),
        ));
    }
    Ok(())
}
