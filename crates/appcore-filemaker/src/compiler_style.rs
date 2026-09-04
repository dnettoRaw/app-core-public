// =============================================================================
//        #######
//     ###       ###     F: compiler_style.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded compiler style contracts and behavior for this crate.

use std::collections::{BTreeMap, BTreeSet};

use crate::source::{ElementSource, StyleSource, TemplateSourceV1};
use crate::{ErrorCode, FileMakerError, Result};

pub(crate) fn apply_named_styles(source: &mut TemplateSourceV1) -> Result<()> {
    let theme = resolve_active_theme(source)?;
    let styles = source.styles.clone();
    for element in &mut source.elements {
        apply_element_styles(element, &styles, &theme, &source.style)?;
    }
    if let Some(page) = &mut source.page {
        for elements in page.element_lists_mut() {
            for element in elements {
                apply_element_styles(element, &styles, &theme, &source.style)?;
            }
        }
    }
    source.styles.clear();
    Ok(())
}

fn apply_element_styles(
    element: &mut ElementSource,
    styles: &BTreeMap<String, StyleSource>,
    theme: &StyleSource,
    template: &StyleSource,
) -> Result<()> {
    let mut merged = StyleSource::default();
    overlay_source_style(&mut merged, theme);
    overlay_source_style(&mut merged, template);
    for name in &element.styles {
        let style = styles
            .get(name)
            .ok_or_else(|| schema_error(format!("style `{name}` was not found")))?;
        overlay_source_style(&mut merged, style);
    }
    overlay_source_style(&mut merged, &element.style);
    element.style = merged;
    for child in &mut element.children {
        apply_element_styles(child, styles, theme, template)?;
    }
    Ok(())
}

fn resolve_active_theme(source: &mut TemplateSourceV1) -> Result<StyleSource> {
    let Some(active) = source.theme.as_deref() else {
        if source.themes.is_empty() {
            return Ok(StyleSource::default());
        }
        return Err(schema_error(
            "templates declaring themes must select an explicit `theme`",
        ));
    };
    if !source.themes.contains_key(active) {
        return Err(schema_error(format!("theme `{active}` was not found")));
    }
    let token_maps = theme_token_maps(&source.themes)?;
    let mut chain = Vec::new();
    let mut visiting = BTreeSet::new();
    collect_theme_chain(active, &source.themes, &mut visiting, &mut chain)?;
    let mut merged = StyleSource::default();
    for name in chain {
        overlay_source_style(&mut merged, &source.themes[&name].style);
    }
    resolve_style_tokens(&mut merged, &token_maps, active)?;
    resolve_style_tokens(&mut source.style, &token_maps, active)?;
    for style in source.styles.values_mut() {
        resolve_style_tokens(style, &token_maps, active)?;
    }
    resolve_element_tokens(&mut source.elements, &token_maps, active)?;
    if let Some(page) = &mut source.page {
        for elements in page.element_lists_mut() {
            resolve_element_tokens(elements, &token_maps, active)?;
        }
    }
    Ok(merged)
}

fn theme_token_maps(
    themes: &BTreeMap<String, crate::source::ThemeSource>,
) -> Result<BTreeMap<String, BTreeMap<String, String>>> {
    themes
        .iter()
        .map(|(name, theme)| {
            let mut tokens = BTreeMap::new();
            if let Some(parent) = &theme.extends {
                tokens.insert("$extends".to_owned(), parent.clone());
            }
            for (token, value) in &theme.tokens {
                if token.is_empty() || token.starts_with('$') || token.len() > 128 {
                    return Err(schema_error("theme token name is invalid"));
                }
                let value = match value {
                    serde_json::Value::String(value) => value.clone(),
                    serde_json::Value::Number(value) => value.to_string(),
                    serde_json::Value::Bool(value) => value.to_string(),
                    serde_json::Value::Null
                    | serde_json::Value::Array(_)
                    | serde_json::Value::Object(_) => {
                        return Err(schema_error("theme tokens must be scalar values"));
                    }
                };
                if value.len() > 1_024 {
                    return Err(limit_error("theme token value exceeds byte limit"));
                }
                tokens.insert(token.clone(), value);
            }
            Ok((name.clone(), tokens))
        })
        .collect()
}

fn collect_theme_chain(
    name: &str,
    themes: &BTreeMap<String, crate::source::ThemeSource>,
    visiting: &mut BTreeSet<String>,
    chain: &mut Vec<String>,
) -> Result<()> {
    if chain.iter().any(|item| item == name) {
        return Ok(());
    }
    if !visiting.insert(name.to_owned()) {
        return Err(FileMakerError::new(
            ErrorCode::DataCycle,
            format!("theme inheritance cycle includes `{name}`"),
        ));
    }
    let theme = themes
        .get(name)
        .ok_or_else(|| schema_error(format!("theme `{name}` was not found")))?;
    if let Some(parent) = &theme.extends {
        collect_theme_chain(parent, themes, visiting, chain)?;
    }
    visiting.remove(name);
    chain.push(name.to_owned());
    Ok(())
}

fn resolve_element_tokens(
    elements: &mut [ElementSource],
    themes: &BTreeMap<String, BTreeMap<String, String>>,
    active: &str,
) -> Result<()> {
    for element in elements {
        resolve_style_tokens(&mut element.style, themes, active)?;
        for rule in &mut element.style_rules {
            resolve_style_tokens(&mut rule.style, themes, active)?;
        }
        resolve_element_tokens(&mut element.children, themes, active)?;
    }
    Ok(())
}

fn resolve_style_tokens(
    style: &mut StyleSource,
    themes: &BTreeMap<String, BTreeMap<String, String>>,
    active: &str,
) -> Result<()> {
    for value in [&mut style.fill, &mut style.stroke, &mut style.color] {
        if let Some(crate::source::ColorSource::Text(token)) = value {
            if token.starts_with('$') {
                *token = crate::style::resolve_token(token, themes, active, 32)?;
            }
        }
    }
    if let Some(token) = style.font.as_deref().filter(|value| value.starts_with('$')) {
        style.font = Some(crate::style::resolve_token(token, themes, active, 32)?);
    }
    Ok(())
}

fn overlay_source_style(target: &mut StyleSource, source: &StyleSource) {
    if source.fill.is_some() {
        target.fill.clone_from(&source.fill);
    }
    if source.stroke.is_some() {
        target.stroke.clone_from(&source.stroke);
    }
    if source.stroke_width.is_some() {
        target.stroke_width = source.stroke_width;
    }
    if source.opacity.is_some() {
        target.opacity = source.opacity;
    }
    if source.font.is_some() {
        target.font.clone_from(&source.font);
    }
    if source.font_size.is_some() {
        target.font_size = source.font_size;
    }
    if source.color.is_some() {
        target.color.clone_from(&source.color);
    }
}

fn schema_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::SchemaField, message)
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}
