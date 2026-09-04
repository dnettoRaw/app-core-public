// =============================================================================
//        #######
//     ###       ###     F: source_build.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded source build contracts and behavior for this crate.

use std::collections::BTreeMap;

use crate::source::*;
use crate::source_layout::{convert_exclusions, validate_exclusions, validate_layout_source};
use crate::source_page_build::{
    append_page_layer_elements, resolve_page, validate_page, validate_page_layer_elements,
};
use crate::source_style::convert_style;
use crate::source_table::convert_table;
use crate::source_text::{convert_text_options, validate_text_options};
use crate::source_transform::{convert_transform, validate_transform};
use crate::{
    AiPolicy, CollisionPolicy, CollisionResolution, DataField, DataSchema, DataType, ElementId,
    ElementIr, ElementKind, ErrorCode, FileMakerError, GeometryIr, PathCommandIr, PresetRegistry,
    Provenance, RegionIr, ResourceLimits, Result, TemplateIr, TextOverflow, FILEMAKER_SCHEMA_V1,
};

impl TemplateSourceV1 {
    /// Parses a bounded strict YAML document and validates the exact schema version.
    pub fn parse_yaml(bytes: &[u8], limits: &ResourceLimits) -> Result<Self> {
        limits.validate()?;
        ResourceLimits::check("template bytes", bytes.len(), limits.max_template_bytes)?;
        let source: Self = serde_yaml::from_slice(bytes).map_err(|error| {
            FileMakerError::new(ErrorCode::SchemaSyntax, format!("invalid YAML: {error}"))
        })?;
        source.validate(limits)?;
        Ok(source)
    }

    /// Validates frontend-only invariants before include/component expansion.
    pub fn validate(&self, limits: &ResourceLimits) -> Result<()> {
        if self.filemaker != FILEMAKER_SCHEMA_V1 {
            return Err(FileMakerError::new(
                ErrorCode::SchemaVersion,
                format!(
                    "unsupported filemaker schema `{}`; expected `{FILEMAKER_SCHEMA_V1}`",
                    self.filemaker
                ),
            ));
        }
        validate_safe_name("template ID", &self.id)?;
        if self.includes.len() > limits.max_include_depth {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "top-level include count exceeds include depth budget",
            ));
        }
        let mut count = self.exclusions.len();
        let mut path_commands = 0_usize;
        validate_elements(&self.elements, limits, &mut count, &mut path_commands)?;
        if let Some(page) = &self.page {
            if page.has_layer_elements() && self.model != ModelKind::Document {
                return Err(FileMakerError::new(
                    ErrorCode::SchemaField,
                    "page layers are valid only for the document model",
                ));
            }
            for elements in page.element_lists() {
                validate_elements(elements, limits, &mut count, &mut path_commands)?;
                validate_page_layer_elements(elements)?;
            }
        }
        for component in self.components.values() {
            validate_elements(&component.elements, limits, &mut count, &mut path_commands)?;
            for slot in component.slots.values() {
                validate_elements(slot, limits, &mut count, &mut path_commands)?;
            }
        }
        validate_page(self.page.as_ref())?;
        validate_exclusions(&self.exclusions, limits.max_elements)?;
        validate_ai_policy(self.ai.as_ref())?;
        Ok(())
    }

    /// Expands the already validated source into a Rust-native template IR.
    ///
    /// Includes and components with arguments are handled by `Compiler`; this
    /// direct conversion accepts only a source that is already self-contained.
    pub fn to_ir(&self, presets: &PresetRegistry, limits: &ResourceLimits) -> Result<TemplateIr> {
        self.validate(limits)?;
        if !self.includes.is_empty() {
            return Err(FileMakerError::new(
                ErrorCode::SchemaField,
                "direct IR conversion requires includes to be expanded",
            ));
        }
        let (page_size, orientation, page_template) =
            resolve_page(&self.id, self.page.as_ref(), presets)?;
        let mut elements = Vec::with_capacity(self.elements.len());
        for element in &self.elements {
            elements.push(element_to_ir(element, "template", limits)?);
        }
        if let Some(page) = &self.page {
            append_page_layer_elements(page, limits, &mut elements, element_to_ir)?;
        }
        let ir = TemplateIr {
            id: self.id.clone(),
            model: self.model,
            page_size,
            orientation,
            page_template,
            collision: self.collision.as_ref().map(convert_collision).transpose()?,
            page_collision: self
                .page
                .as_ref()
                .and_then(|page| page.collision.as_ref())
                .map(convert_collision)
                .transpose()?,
            guides: self.guides.clone(),
            regions: convert_regions(&self.regions)?,
            exclusions: convert_exclusions(&self.exclusions),
            data_schema: convert_data_schema(&self.data_schema),
            ai_policy: convert_ai_policy(self.ai.as_ref()),
            elements,
        };
        ir.validate(limits.max_elements)?;
        Ok(ir)
    }
}

pub(crate) fn self_contained_element_to_ir(
    source: &ElementSource,
    limits: &ResourceLimits,
) -> Result<ElementIr> {
    limits.validate()?;
    let mut count = 0;
    let mut path_commands = 0;
    validate_elements(
        std::slice::from_ref(source),
        limits,
        &mut count,
        &mut path_commands,
    )?;
    element_to_ir(source, "element", limits)
}

fn validate_ai_policy(policy: Option<&AiSourcePolicy>) -> Result<()> {
    let Some(policy) = policy else {
        return Ok(());
    };
    if policy.purpose.len() > 1_024
        || policy.rules.len() > 64
        || policy.rules.iter().any(|rule| rule.len() > 1_024)
        || policy.editable.len() > 10_000
        || policy.locked.len() > 10_000
    {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "AI policy exceeds its bounded text or ID limits",
        ));
    }
    for id in policy.editable.iter().chain(&policy.locked) {
        ElementId::new(id)?;
    }
    Ok(())
}

fn convert_ai_policy(policy: Option<&AiSourcePolicy>) -> AiPolicy {
    policy.map_or_else(AiPolicy::default, |policy| AiPolicy {
        purpose: policy.purpose.clone(),
        rules: policy.rules.clone(),
        editable: policy.editable.iter().cloned().collect(),
        locked: policy.locked.iter().cloned().collect(),
    })
}

fn validate_elements(
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
        if matches!(kind, Some(ElementKind::Path | ElementKind::Polygon)) && element.path.is_empty()
        {
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
        if let Some(text) = &element.text {
            ResourceLimits::check("text bytes", text.len(), limits.max_text_bytes)?;
        }
        element.image.validate()?;
        validate_transform(&element.transform)?;
        validate_text_options(&element.text_options)?;
        validate_layout_source(element)?;
        validate_style_rules(element)?;
        if element.text_options != TextSourceOptions::default() {
            match kind {
                Some(ElementKind::Text) => {}
                Some(ElementKind::Table)
                    if element.text_options.overflow == TextOverflow::Wrap
                        && element.text_options.max_lines.is_none() => {}
                Some(ElementKind::Table) => {
                    return Err(FileMakerError::new(
                        ErrorCode::SchemaField,
                        "table text_options support min_font_size, line_height, and writing_mode",
                    )
                    .at(element.id.clone()))
                }
                _ => {
                    return Err(FileMakerError::new(
                        ErrorCode::SchemaField,
                        "text_options are only valid on text and table elements",
                    )
                    .at(element.id.clone()))
                }
            }
        }
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
        validate_elements(&element.children, limits, count, path_commands)?;
        for slot in element.slots.values() {
            validate_elements(slot, limits, count, path_commands)?;
        }
    }
    Ok(())
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

fn element_to_ir(
    source: &ElementSource,
    logical_source: &str,
    limits: &ResourceLimits,
) -> Result<ElementIr> {
    if source.component.is_some() {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "direct IR conversion requires components to be expanded",
        )
        .at(source.id.clone()));
    }
    let mut children = Vec::with_capacity(source.children.len());
    for child in &source.children {
        children.push(element_to_ir(child, logical_source, limits)?);
    }
    if let Some(text) = &source.text {
        ResourceLimits::check("text bytes", text.len(), limits.max_text_bytes)?;
    }
    Ok(ElementIr {
        id: ElementId::new(source.id.clone())?,
        kind: ElementKind::parse(&source.element_type)?,
        geometry: GeometryIr {
            x: source.x,
            y: source.y,
            width: source.width,
            height: source.height,
            constraints: source.constraints,
            align_x: source.align_x,
            align_y: source.align_y,
            region: source.region.clone(),
            anchors: source.anchors.clone(),
        },
        transform: convert_transform(source.transform)?,
        text: source.text.clone(),
        text_options: convert_text_options(source.text_options),
        table: source
            .table
            .as_ref()
            .map(|table| convert_table(table, limits))
            .transpose()?,
        asset: source.asset.clone(),
        path: source.path.iter().map(convert_path_command).collect(),
        image: source.image,
        style: convert_style(&source.style)?,
        style_rules: source
            .style_rules
            .iter()
            .map(|rule| {
                Ok(crate::ElementStyleRule {
                    when: rule.when.clone(),
                    style: convert_style(&rule.style)?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        layout: source.layout,
        distribute: source.distribute,
        gap: source.gap,
        collision: source
            .collision
            .as_ref()
            .map(convert_collision)
            .transpose()?,
        children,
        locked: source.locked,
        hidden: source.hidden,
        layer: source.layer.clone(),
        z_index: source.z_index,
        binding: source.binding.clone(),
        when: source.when.clone(),
        repeat: source.repeat.clone(),
        provenance: Provenance {
            source: source
                .provenance_source
                .clone()
                .unwrap_or_else(|| logical_source.to_owned()),
            components: source.provenance_components.clone(),
            styles: source.styles.clone(),
            patches: Vec::new(),
        },
        page_placement: None,
    })
}

fn convert_path_command(source: &PathCommandSource) -> PathCommandIr {
    match *source {
        PathCommandSource::Move { x, y } => PathCommandIr::Move { x, y },
        PathCommandSource::Line { x, y } => PathCommandIr::Line { x, y },
        PathCommandSource::Curve {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } => PathCommandIr::Curve {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        },
        PathCommandSource::Close => PathCommandIr::Close,
    }
}

fn convert_regions(source: &BTreeMap<String, RegionSource>) -> Result<BTreeMap<String, RegionIr>> {
    source
        .iter()
        .map(|(name, region)| {
            Ok((
                name.clone(),
                RegionIr {
                    x: region.x,
                    y: region.y,
                    width: region.width,
                    height: region.height,
                    collision: region
                        .collision
                        .as_ref()
                        .map(convert_collision)
                        .transpose()?,
                },
            ))
        })
        .collect()
}

fn convert_data_schema(source: &BTreeMap<String, DataFieldSource>) -> DataSchema {
    source
        .iter()
        .map(|(name, field)| {
            let data_type = match field.data_type {
                DataTypeSource::String => DataType::String,
                DataTypeSource::Integer => DataType::Integer,
                DataTypeSource::Decimal => DataType::Decimal,
                DataTypeSource::Boolean => DataType::Boolean,
                DataTypeSource::Date => DataType::Date,
                DataTypeSource::DateTime => DataType::DateTime,
                DataTypeSource::Duration => DataType::Duration,
                DataTypeSource::Currency => DataType::Currency,
                DataTypeSource::Array => DataType::Array,
                DataTypeSource::Object => DataType::Object,
                DataTypeSource::Null => DataType::Null,
            };
            (
                name.clone(),
                DataField {
                    data_type,
                    nullable: field.nullable,
                    computed: field.computed.clone(),
                },
            )
        })
        .collect()
}

fn convert_collision(source: &CollisionSource) -> Result<CollisionPolicy> {
    let advanced = match source {
        CollisionSource::Enabled(enabled) => {
            return Ok(CollisionPolicy {
                enabled: *enabled,
                ..CollisionPolicy::default()
            });
        }
        CollisionSource::Advanced(advanced) => advanced,
    };
    let resolution = match advanced.policy.as_str() {
        "push" => CollisionResolution::Push,
        "error" => CollisionResolution::Error,
        "overlay" => CollisionResolution::Overlay,
        "next_page" => CollisionResolution::NextPage,
        "shrink" => CollisionResolution::Shrink,
        _ => {
            return Err(FileMakerError::new(
                ErrorCode::SchemaField,
                format!("unknown collision policy `{}`", advanced.policy),
            ))
        }
    };
    Ok(CollisionPolicy {
        enabled: advanced.enabled,
        group: advanced.group.clone(),
        collides_with: advanced.collides_with.iter().cloned().collect(),
        ignore: advanced.ignore.iter().cloned().collect(),
        priority: advanced.priority,
        movable: advanced.movable,
        bounds: advanced.bounds,
        resolution,
    })
}

fn validate_safe_name(label: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            format!("{label} is invalid"),
        ));
    }
    Ok(())
}
