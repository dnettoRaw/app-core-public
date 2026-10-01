// =============================================================================
//        #######
//     ###       ###     F: source_elements.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Bounded validation for nested version-one element sources.

use crate::source::{ElementSource, TextSourceOptions};
use crate::source_layout::validate_layout_source;
use crate::source_text::validate_text_options;
use crate::source_transform::validate_transform;
use crate::{
    ElementId, ElementKind, ErrorCode, FileMakerError, ResourceLimits, Result, TextOverflow,
};

pub(crate) fn validate_elements(
    elements: &[ElementSource],
    limits: &ResourceLimits,
    count: &mut usize,
    path_commands: &mut usize,
) -> Result<()> {
    for element in elements {
        *count = count.saturating_add(1);
        if *count > limits.max_elements {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "source element count exceeds configured limit",
            ));
        }
        validate_element(element, limits, path_commands)?;
        validate_elements(&element.children, limits, count, path_commands)?;
        for slot in element.slots.values() {
            validate_elements(slot, limits, count, path_commands)?;
        }
    }
    Ok(())
}

fn validate_element(
    element: &ElementSource,
    limits: &ResourceLimits,
    path_commands: &mut usize,
) -> Result<()> {
    ElementId::new(element.id.clone())?;
    let kind = if element.element_type != "slot" {
        Some(ElementKind::parse(&element.element_type)?)
    } else {
        None
    };
    *path_commands = path_commands.saturating_add(element.path.len());
    if *path_commands > limits.max_path_commands {
        return Err(FileMakerError::new(
            ErrorCode::LimitExceeded,
            "source path command count exceeds configured limit",
        ));
    }
    validate_element_path(element, kind)?;
    validate_element_options(element, kind, limits)?;
    validate_element_kind(element, kind)
}

fn validate_element_path(element: &ElementSource, kind: Option<ElementKind>) -> Result<()> {
    if matches!(kind, Some(ElementKind::Path | ElementKind::Polygon)) && element.path.is_empty() {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "path and polygon elements require path commands",
        )
        .at(element.id.clone()));
    }
    if !element.path.is_empty()
        && !matches!(
            kind,
            Some(ElementKind::Path | ElementKind::Polygon | ElementKind::Line)
        )
    {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "path commands are only valid on line, path, or polygon elements",
        )
        .at(element.id.clone()));
    }
    Ok(())
}

fn validate_element_options(
    element: &ElementSource,
    kind: Option<ElementKind>,
    limits: &ResourceLimits,
) -> Result<()> {
    if let Some(text) = &element.text {
        ResourceLimits::check("text bytes", text.len(), limits.max_text_bytes)?;
    }
    if element.text_segments.len() > 128 {
        return Err(FileMakerError::new(
            ErrorCode::LimitExceeded,
            "inline text segment count exceeds 128",
        )
        .at(element.id.clone()));
    }
    let mut total_segment_bytes = 0_usize;
    for segment in &element.text_segments {
        if segment.text.is_some() == segment.binding.is_some() {
            return Err(FileMakerError::new(
                ErrorCode::SchemaField,
                "each inline text segment requires exactly one of text or binding",
            )
            .at(element.id.clone()));
        }
        if segment
            .text
            .as_ref()
            .is_some_and(|text| text.contains(['\n', '\r']))
        {
            return Err(FileMakerError::new(
                ErrorCode::SchemaField,
                "inline text segments must be single-line",
            )
            .at(element.id.clone()));
        }
        if segment.binding.as_ref().is_some_and(String::is_empty) {
            return Err(FileMakerError::new(
                ErrorCode::SchemaField,
                "inline text segment binding cannot be empty",
            )
            .at(element.id.clone()));
        }
        if !matches!(segment.gap_after, crate::Length::Absolute(value) if value >= crate::Unit::ZERO)
            && !matches!(segment.gap_after, crate::Length::Logical(value) if value >= 0)
        {
            return Err(FileMakerError::new(
                ErrorCode::SchemaField,
                "inline text segment gaps require nonnegative absolute or logical lengths",
            )
            .at(element.id.clone()));
        }
        if let Some(text) = &segment.text {
            total_segment_bytes = total_segment_bytes.saturating_add(text.len());
            ResourceLimits::check(
                "inline text bytes",
                total_segment_bytes,
                limits.max_text_bytes,
            )?;
        }
        if let Some(binding) = &segment.binding {
            crate::Expression::parse(binding).map_err(|error| error.at(element.id.clone()))?;
        }
    }
    if !element.text_segments.is_empty()
        && (element.text.is_some()
            || element.binding.is_some()
            || kind != Some(ElementKind::Text)
            || element.text_options.overflow != TextOverflow::Error
            || element.text_options.max_lines != Some(1)
            || element.text_options.writing_mode != crate::WritingMode::Horizontal)
    {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "inline text segments require a text element with no text/binding, horizontal writing, overflow:error, and max_lines: 1",
        )
        .at(element.id.clone()));
    }
    element.image.validate()?;
    validate_transform(&element.transform)?;
    validate_text_options(&element.text_options)?;
    validate_layout_source(element)?;
    validate_style_rules(element)?;
    validate_text_option_kind(element, kind)
}

fn validate_style_rules(element: &ElementSource) -> Result<()> {
    if element.style_rules.len() > 64 {
        return Err(FileMakerError::new(
            ErrorCode::LimitExceeded,
            "element conditional style count exceeds 64",
        )
        .at(element.id.clone()));
    }
    for rule in &element.style_rules {
        if rule.when.is_empty() {
            return Err(FileMakerError::new(
                ErrorCode::SchemaField,
                "conditional style expression cannot be empty",
            )
            .at(element.id.clone()));
        }
        crate::Expression::parse(&rule.when).map_err(|error| error.at(element.id.clone()))?;
    }
    Ok(())
}

fn validate_text_option_kind(element: &ElementSource, kind: Option<ElementKind>) -> Result<()> {
    if element.text_options == TextSourceOptions::default() {
        return Ok(());
    }
    match kind {
        Some(ElementKind::Text) => Ok(()),
        Some(ElementKind::Table)
            if element.text_options.overflow == TextOverflow::Wrap
                && element.text_options.max_lines.is_none()
                && element.text_options.align_x == crate::Alignment::Start
                && element.text_options.padding == crate::TextBlockPadding::default() =>
        {
            Ok(())
        }
        Some(ElementKind::Table) => Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "table text_options do not support block padding; cell alignment belongs to table.columns[].align_x and cell insets belong to table.columns[].padding",
        )
        .at(element.id.clone())),
        _ => Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "text_options are only valid on text and table elements",
        )
        .at(element.id.clone())),
    }
}

fn validate_element_kind(element: &ElementSource, kind: Option<ElementKind>) -> Result<()> {
    if (kind == Some(ElementKind::Table)) != element.table.is_some() {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "type table requires `table`, which is invalid on every other element type",
        )
        .at(element.id.clone()));
    }
    if kind == Some(ElementKind::Table) && element.binding.is_none() {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "table elements require an array binding",
        )
        .at(element.id.clone()));
    }
    if kind == Some(ElementKind::Table)
        && (!element.children.is_empty() || !element.slots.is_empty())
    {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "table elements cannot contain children or slots",
        )
        .at(element.id.clone()));
    }
    Ok(())
}
