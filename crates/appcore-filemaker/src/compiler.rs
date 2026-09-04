// =============================================================================
//        #######
//     ###       ###     F: compiler.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded compiler contracts and behavior for this crate.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::compiler_style::apply_named_styles;
use crate::source::{ComponentSource, ElementSource, TemplateSourceV1};
use crate::{
    DataValue, DocumentIr, ErrorCode, FileMakerError, OperationControl, Patch, PatchTransaction,
    PresetRegistry, ProgressPhase, ResourceLimits, Result, TemplateIr, TemplateResolver,
};

/// Builder for a compiler with explicit resolvers and limits.
#[derive(Default)]
pub struct CompilerBuilder {
    limits: ResourceLimits,
    presets: PresetRegistry,
    template_resolver: Option<Arc<dyn TemplateResolver>>,
    control: OperationControl,
}

impl CompilerBuilder {
    /// Replaces resource limits.
    #[must_use]
    pub fn limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Replaces the preset registry.
    #[must_use]
    pub fn presets(mut self, presets: PresetRegistry) -> Self {
        self.presets = presets;
        self
    }

    /// Supplies the only resolver allowed to load includes.
    #[must_use]
    pub fn template_resolver(mut self, resolver: Arc<dyn TemplateResolver>) -> Self {
        self.template_resolver = Some(resolver);
        self
    }

    /// Supplies cooperative cancellation and progress controls.
    #[must_use]
    pub fn control(mut self, control: OperationControl) -> Self {
        self.control = control;
        self
    }

    /// Validates configuration and builds a compiler.
    pub fn build(self) -> Result<Compiler> {
        self.limits.validate()?;
        Ok(Compiler {
            limits: self.limits,
            presets: self.presets,
            template_resolver: self.template_resolver,
            control: self.control,
        })
    }
}

/// Deterministic frontend compiler reusable across data instances.
pub struct Compiler {
    limits: ResourceLimits,
    presets: PresetRegistry,
    template_resolver: Option<Arc<dyn TemplateResolver>>,
    control: OperationControl,
}

impl Compiler {
    /// Starts configuration with conservative defaults.
    #[must_use]
    pub fn builder() -> CompilerBuilder {
        CompilerBuilder::default()
    }

    /// Parses, expands includes/components/styles, and returns reusable IR.
    pub fn compile_template_yaml(&self, bytes: &[u8]) -> Result<TemplateIr> {
        self.control
            .checkpoint(ProgressPhase::Compile, 0, Some(4))?;
        let mut source = TemplateSourceV1::parse_yaml(bytes, &self.limits)?;
        self.control
            .checkpoint(ProgressPhase::Compile, 1, Some(4))?;
        let mut include_stack = BTreeSet::new();
        let mut include_bytes = 0_usize;
        self.expand_includes(&mut source, &mut include_stack, &mut include_bytes, 0)?;
        self.control
            .checkpoint(ProgressPhase::Compile, 2, Some(4))?;
        expand_components(&mut source, &self.limits)?;
        apply_named_styles(&mut source)?;
        self.control
            .checkpoint(ProgressPhase::Compile, 3, Some(4))?;
        let ir = source.to_ir(&self.presets, &self.limits)?;
        crate::validate_template(&ir, &self.limits).enforce(false)?;
        self.control
            .checkpoint(ProgressPhase::Compile, 4, Some(4))?;
        Ok(ir)
    }

    /// Binds one data instance and applies ordered patch transactions.
    pub fn bind(
        &self,
        template: &TemplateIr,
        data: &DataValue,
        patches: &[Patch],
    ) -> Result<DocumentIr> {
        self.control.checkpoint(ProgressPhase::Bind, 0, Some(3))?;
        data.validate(64, self.limits.max_elements, self.limits.max_text_bytes)?;
        let resolved_data = crate::data::resolve_computed_fields(
            &template.data_schema,
            data,
            self.limits.max_expression_steps,
        )?;
        self.control.checkpoint(ProgressPhase::Bind, 1, Some(3))?;
        let mut document = DocumentIr {
            template_id: template.id.clone(),
            model: template.model,
            page_size: template.page_size,
            page_template: template.page_template.clone(),
            collision: template.collision.clone(),
            page_collision: template.page_collision.clone(),
            guides: template.guides.clone(),
            regions: template.regions.clone(),
            exclusions: template.exclusions.clone(),
            ai_policy: template.ai_policy.clone(),
            elements: crate::compiler_bind::bind_elements(
                &template.elements,
                &resolved_data,
                &self.limits,
                &self.control,
            )?,
        };
        self.control.checkpoint(ProgressPhase::Bind, 2, Some(3))?;
        let total_patch_operations = patches.iter().try_fold(0_usize, |total, patch| {
            total
                .checked_add(patch.operations.len())
                .ok_or_else(|| limit_error("runtime patch operation accounting overflow"))
        })?;
        if total_patch_operations > self.limits.max_patch_operations {
            return Err(limit_error(
                "runtime patches exceed the configured total operation limit",
            ));
        }
        for (index, patch) in patches.iter().enumerate() {
            self.control.checkpoint(
                ProgressPhase::Bind,
                u64::try_from(index).unwrap_or(u64::MAX),
                u64::try_from(patches.len()).ok(),
            )?;
            PatchTransaction::new(&mut document, self.limits.max_patch_operations).apply(patch)?;
        }
        self.control.checkpoint(ProgressPhase::Bind, 3, Some(3))?;
        Ok(document)
    }

    fn expand_includes(
        &self,
        source: &mut TemplateSourceV1,
        stack: &mut BTreeSet<String>,
        total_bytes: &mut usize,
        depth: usize,
    ) -> Result<()> {
        self.control.checkpoint(
            ProgressPhase::Compile,
            u64::try_from(depth).unwrap_or(u64::MAX),
            u64::try_from(self.limits.max_include_depth).ok(),
        )?;
        if depth > self.limits.max_include_depth {
            return Err(limit_error("include depth exceeds configured limit"));
        }
        let includes = std::mem::take(&mut source.includes);
        for include in includes {
            if !stack.insert(include.path.clone()) {
                return Err(
                    FileMakerError::new(ErrorCode::DataCycle, "include cycle detected")
                        .at(include.path),
                );
            }
            let resolver = self.template_resolver.as_ref().ok_or_else(|| {
                FileMakerError::new(
                    ErrorCode::AssetInvalid,
                    "template contains includes but no resolver is configured",
                )
            })?;
            let bytes = resolver.resolve_template(&include.path, self.limits.max_include_bytes)?;
            *total_bytes = total_bytes
                .checked_add(bytes.len())
                .ok_or_else(|| limit_error("include byte accounting overflow"))?;
            if *total_bytes > self.limits.max_include_bytes {
                return Err(limit_error("total include bytes exceed configured limit"));
            }
            let mut child = TemplateSourceV1::parse_yaml(&bytes, &self.limits)?;
            if child.model != source.model {
                return Err(schema_error("included template model does not match root"));
            }
            self.expand_includes(&mut child, stack, total_bytes, depth + 1)?;
            merge_include(source, child, include.namespace.as_deref(), &include.path)?;
            stack.remove(&include.path);
        }
        Ok(())
    }
}

fn merge_include(
    root: &mut TemplateSourceV1,
    mut child: TemplateSourceV1,
    namespace: Option<&str>,
    source_path: &str,
) -> Result<()> {
    if child
        .page
        .as_ref()
        .is_some_and(|page| page.has_layer_elements())
    {
        return Err(schema_error(
            "included templates cannot declare page layers; the root owns physical pages",
        ));
    }
    let prefix = namespace
        .map(|value| format!("{value}/"))
        .unwrap_or_default();
    for element in &mut child.elements {
        namespace_element(element, &prefix, source_path);
    }
    namespace_components(&mut child.components, namespace);
    merge_map(&mut root.components, child.components, "component")?;
    merge_map(&mut root.styles, child.styles, "style")?;
    merge_map(&mut root.themes, child.themes, "theme")?;
    merge_map(&mut root.guides, child.guides, "guide")?;
    merge_map(&mut root.regions, child.regions, "region")?;
    merge_map(&mut root.exclusions, child.exclusions, "exclusion")?;
    merge_map(&mut root.data_schema, child.data_schema, "data field")?;
    root.elements.extend(child.elements);
    Ok(())
}

fn merge_map<T>(
    target: &mut BTreeMap<String, T>,
    source: BTreeMap<String, T>,
    label: &str,
) -> Result<()> {
    for (name, value) in source {
        if target.insert(name.clone(), value).is_some() {
            return Err(schema_error(format!(
                "duplicate {label} `{name}` after include expansion"
            )));
        }
    }
    Ok(())
}

fn namespace_components(
    components: &mut BTreeMap<String, ComponentSource>,
    namespace: Option<&str>,
) {
    let Some(namespace) = namespace else {
        return;
    };
    let old = std::mem::take(components);
    for (name, mut component) in old {
        let prefix = format!("{namespace}/");
        for element in &mut component.elements {
            namespace_element(element, &prefix, "component");
        }
        components.insert(format!("{namespace}.{name}"), component);
    }
}

fn namespace_element(element: &mut ElementSource, prefix: &str, source_path: &str) {
    element.id = format!("{prefix}{}", element.id);
    element.provenance_source = Some(source_path.to_owned());
    for child in &mut element.children {
        namespace_element(child, prefix, source_path);
    }
    for values in element.slots.values_mut() {
        for child in values {
            namespace_element(child, prefix, source_path);
        }
    }
}

fn expand_components(source: &mut TemplateSourceV1, limits: &ResourceLimits) -> Result<()> {
    let components = source.components.clone();
    let mut count = 0_usize;
    source.elements = expand_element_list(
        std::mem::take(&mut source.elements),
        &components,
        limits,
        &mut count,
        &mut Vec::new(),
    )?;
    if let Some(page) = &mut source.page {
        for elements in page.element_lists_mut() {
            *elements = expand_element_list(
                std::mem::take(elements),
                &components,
                limits,
                &mut count,
                &mut Vec::new(),
            )?;
        }
    }
    source.components.clear();
    Ok(())
}

fn expand_element_list(
    elements: Vec<ElementSource>,
    components: &BTreeMap<String, ComponentSource>,
    limits: &ResourceLimits,
    count: &mut usize,
    component_stack: &mut Vec<String>,
) -> Result<Vec<ElementSource>> {
    let mut expanded = Vec::new();
    for mut element in elements {
        *count = count.saturating_add(1);
        if *count > limits.max_elements {
            return Err(limit_error("expanded element limit exceeded"));
        }
        if element.element_type == "slot" {
            return Err(schema_error(
                "slot placeholder is only valid inside a component",
            ));
        }
        if let Some(name) = element.component.clone() {
            if component_stack.contains(&name) {
                return Err(FileMakerError::new(
                    ErrorCode::DataCycle,
                    "component cycle detected",
                ));
            }
            let component = components
                .get(&name)
                .ok_or_else(|| schema_error(format!("component `{name}` was not found")))?;
            component_stack.push(name.clone());
            let mut props = component.props.clone();
            props.extend(element.props.clone());
            let body =
                fill_component_slots(component.elements.clone(), &component.slots, &element.slots);
            let mut body = expand_element_list(body, components, limits, count, component_stack)?;
            for child in &mut body {
                prefix_component_child(child, &element.id, &name, &props);
            }
            component_stack.pop();
            element.component = None;
            "group".clone_into(&mut element.element_type);
            element.children = body;
            element.slots.clear();
        } else {
            element.children =
                expand_element_list(element.children, components, limits, count, component_stack)?;
        }
        expanded.push(element);
    }
    Ok(expanded)
}

fn fill_component_slots(
    elements: Vec<ElementSource>,
    defaults: &BTreeMap<String, Vec<ElementSource>>,
    supplied: &BTreeMap<String, Vec<ElementSource>>,
) -> Vec<ElementSource> {
    let mut result = Vec::new();
    for mut element in elements {
        if element.element_type == "slot" {
            let name = element.text.as_deref().unwrap_or("default");
            result.extend(
                supplied
                    .get(name)
                    .or_else(|| defaults.get(name))
                    .cloned()
                    .unwrap_or_default(),
            );
        } else {
            element.children = fill_component_slots(element.children, defaults, supplied);
            result.push(element);
        }
    }
    result
}

fn prefix_component_child(
    element: &mut ElementSource,
    instance_id: &str,
    component: &str,
    props: &BTreeMap<String, serde_json::Value>,
) {
    element.id = format!("{instance_id}/{}", element.id);
    element.provenance_components.push(component.to_owned());
    if let Some(text) = &mut element.text {
        *text = substitute_props(text, props);
    }
    for child in &mut element.children {
        prefix_component_child(child, instance_id, component, props);
    }
}

fn substitute_props(source: &str, props: &BTreeMap<String, serde_json::Value>) -> String {
    let mut result = source.to_owned();
    for (name, value) in props {
        let replacement = value
            .as_str()
            .map_or_else(|| value.to_string(), ToOwned::to_owned);
        result = result.replace(&format!("{{{{{name}}}}}"), &replacement);
    }
    result
}

fn schema_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::SchemaField, message)
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}
