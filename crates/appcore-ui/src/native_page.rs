// =============================================================================
//        #######
//     ###       ###     F: native_page.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Validated native page declarations. Coordinates use logical pixels from the top left.
use crate::{UiError, UiResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
/// Native page schema, independently versioned from manifests and bridge frames.
pub const UI_NATIVE_PAGE_V1: u16 = 1;
/// Fixed logical rectangle within the initial page viewport.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiRectangle {
    /// Distance from the left edge.
    pub x: f64,
    /// Distance from the top edge.
    pub y: f64,
    /// Logical width.
    pub width: f64,
    /// Logical height.
    pub height: f64,
}
/// Initial native control vocabulary. Application behavior is bound by action identity.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiNativeNodeV1 {
    /// A noneditable system-rendered text control.
    Text {
        /// Unique page-local control identity.
        id: String,
        /// Initial logical frame.
        rect: UiRectangle,
        /// Application-owned display text, never logged by this module.
        text: String,
    },
    /// A native action button; it does not itself dispatch or grant authorization.
    Button {
        /// Unique page-local control identity.
        id: String,
        /// Initial logical frame.
        rect: UiRectangle,
        /// Application-owned visible label.
        label: String,
        /// Application binding resolved after a trusted native activation.
        action: String,
    },
    /// Editable system text field; activation is submitted on Return.
    Input {
        /// Unique control identity.
        id: String,
        /// Logical frame.
        rect: UiRectangle,
        /// Initial bounded text.
        value: String,
        /// Bounded placeholder.
        placeholder: String,
        /// Submit action, without implicit dispatch.
        action: String,
    },
    /// Native checkbox with explicit initial state.
    Toggle {
        /// Unique control identity.
        id: String,
        /// Logical frame.
        rect: UiRectangle,
        /// Bounded visible label.
        label: String,
        /// Initial checked state.
        checked: bool,
        /// Change action identity.
        action: String,
    },
    /// Native dropdown with a bounded immutable option set.
    Select {
        /// Unique control identity.
        id: String,
        /// Logical frame.
        rect: UiRectangle,
        /// Between 1 and 64 unique nonempty bounded labels; selection is an index, not authority.
        options: Vec<String>,
        /// Initial index into options.
        selected: usize,
        /// Change action identity.
        action: String,
    },
    /// Finite bounded numeric slider.
    Slider {
        /// Unique control identity.
        id: String,
        /// Logical frame.
        rect: UiRectangle,
        /// Inclusive minimum.
        min: f64,
        /// Inclusive maximum, strictly greater than minimum.
        max: f64,
        /// Initial value within the interval.
        value: f64,
        /// Change action identity.
        action: String,
    },
    /// Determinate progress presentation; it does not start or track work implicitly.
    Progress {
        /// Unique control identity.
        id: String,
        /// Logical frame.
        rect: UiRectangle,
        /// Finite fraction in 0..=1.
        value: f64,
    },
}
impl UiNativeNodeV1 {
    /// Stable control identity; no display text is returned.
    pub fn id(&self) -> &str {
        match self {
            Self::Text { id, .. }
            | Self::Button { id, .. }
            | Self::Input { id, .. }
            | Self::Toggle { id, .. }
            | Self::Select { id, .. }
            | Self::Slider { id, .. }
            | Self::Progress { id, .. } => id,
        }
    }
    /// Bound action identity for interactive controls; display-only controls return none.
    pub fn action(&self) -> Option<&str> {
        match self {
            Self::Text { .. } | Self::Progress { .. } => None,
            Self::Button { action, .. }
            | Self::Input { action, .. }
            | Self::Toggle { action, .. }
            | Self::Select { action, .. }
            | Self::Slider { action, .. } => Some(action),
        }
    }
    /// Initial logical rectangle.
    pub fn rect(&self) -> UiRectangle {
        match self {
            Self::Text { rect, .. }
            | Self::Button { rect, .. }
            | Self::Input { rect, .. }
            | Self::Toggle { rect, .. }
            | Self::Select { rect, .. }
            | Self::Slider { rect, .. }
            | Self::Progress { rect, .. } => *rect,
        }
    }
}
/// Source-native page definition; use `UiNativePage::new` before platform rendering.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiNativePageV1 {
    /// Exactly native page version 1.
    pub version: u16,
    /// Application-owned window title.
    pub title: String,
    /// Initial viewport width, in logical pixels.
    pub width: f64,
    /// Initial viewport height, in logical pixels.
    pub height: f64,
    /// Flat bounded set of controls; order determines focus and paint order.
    pub nodes: Vec<UiNativeNodeV1>,
}
/// Immutable validated page. No rendering, process startup or authority is implicit.
#[derive(Clone)]
pub struct UiNativePage(UiNativePageV1);
impl UiNativePage {
    /// Decodes at most 256 KiB; unknown versions hit the wall before schema decoding.
    pub fn decode(bytes: &[u8]) -> UiResult<Self> {
        if bytes.len() > 256 * 1024 {
            return Err(UiError::PayloadTooLarge);
        }
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| UiError::InvalidFrame)?;
        if value.get("version").and_then(serde_json::Value::as_u64)
            != Some(u64::from(UI_NATIVE_PAGE_V1))
        {
            return Err(UiError::UnsupportedVersion);
        }
        Self::new(serde_json::from_value(value).map_err(|_| UiError::InvalidFrame)?)
    }
    /// Validates dimensions, byte/count budgets, unique IDs and action names.
    pub fn new(page: UiNativePageV1) -> UiResult<Self> {
        if page.version != UI_NATIVE_PAGE_V1 {
            return Err(UiError::UnsupportedVersion);
        }
        if !extent(page.width)
            || !extent(page.height)
            || page.width * page.height > 4_194_304.0
            || page.title.is_empty()
            || page.title.len() > 256
            || page.title.contains('\0')
            || page.nodes.len() > 512
        {
            return Err(UiError::InvalidRequest);
        }
        crate::json_budget::json_bytes(&page, 256 * 1024)?;
        let mut ids = HashSet::new();
        for node in &page.nodes {
            crate::validation::validate_name(node.id())?;
            if !ids.insert(node.id()) {
                return Err(UiError::InvalidRequest);
            }
            let rect = node.rect();
            if !rect.x.is_finite()
                || !rect.y.is_finite()
                || rect.x < 0.0
                || rect.y < 0.0
                || !extent(rect.width)
                || !extent(rect.height)
                || rect.x + rect.width > page.width
                || rect.y + rect.height > page.height
            {
                return Err(UiError::InvalidRequest);
            }
            if let Some(action) = node.action() {
                crate::validation::validate_name(action)?;
            }
            match node {
                UiNativeNodeV1::Input {
                    value, placeholder, ..
                } => {
                    text_budget(value)?;
                    text_budget(placeholder)?;
                }
                UiNativeNodeV1::Toggle { label, .. } => text_budget(label)?,
                UiNativeNodeV1::Select {
                    options, selected, ..
                } => {
                    if options.is_empty() || options.len() > 64 || *selected >= options.len() {
                        return Err(UiError::InvalidRequest);
                    }
                    let mut labels = HashSet::new();
                    for label in options {
                        text_budget(label)?;
                        if label.is_empty() || !labels.insert(label) {
                            return Err(UiError::InvalidRequest);
                        }
                    }
                }
                UiNativeNodeV1::Slider {
                    min, max, value, ..
                } => {
                    if !min.is_finite()
                        || !max.is_finite()
                        || !value.is_finite()
                        || min >= max
                        || !(*min..=*max).contains(value)
                        || !(*max - *min).is_finite()
                    {
                        return Err(UiError::InvalidRequest);
                    }
                }
                UiNativeNodeV1::Progress { value, .. } => {
                    if !value.is_finite() || !(0.0..=1.0).contains(value) {
                        return Err(UiError::InvalidRequest);
                    }
                }
                UiNativeNodeV1::Text { text, .. } => text_budget(text)?,
                UiNativeNodeV1::Button { label, action, .. } => {
                    text_budget(label)?;
                    crate::validation::validate_name(action)?;
                }
            }
        }
        Ok(Self(page))
    }
    /// Produces a validated immutable update to an existing text/button label.
    /// Rejects unknown controls and rechecks the whole encoded budget before any rendering.
    pub fn with_label(&self, id: &str, label: impl Into<String>) -> UiResult<Self> {
        let mut page = self.0.clone();
        let node = page
            .nodes
            .iter_mut()
            .find(|node| node.id() == id)
            .ok_or(UiError::InvalidRequest)?;
        match node {
            UiNativeNodeV1::Text { text, .. } => *text = label.into(),
            UiNativeNodeV1::Button { label: text, .. }
            | UiNativeNodeV1::Toggle { label: text, .. } => *text = label.into(),
            _ => return Err(UiError::InvalidRequest),
        }
        Self::new(page)
    }
    /// Produces a validated immutable value update; mismatched kinds or indices fail closed.
    pub fn with_value(&self, id: &str, value: UiNativeValue) -> UiResult<Self> {
        let mut page = self.0.clone();
        let node = page
            .nodes
            .iter_mut()
            .find(|node| node.id() == id)
            .ok_or(UiError::InvalidRequest)?;
        match (node, value) {
            (UiNativeNodeV1::Input { value, .. }, UiNativeValue::Text(next)) => *value = next,
            (UiNativeNodeV1::Toggle { checked, .. }, UiNativeValue::Boolean(next)) => {
                *checked = next
            }
            (UiNativeNodeV1::Select { selected, .. }, UiNativeValue::Selection(next)) => {
                *selected = next
            }
            (
                UiNativeNodeV1::Slider { value, .. } | UiNativeNodeV1::Progress { value, .. },
                UiNativeValue::Number(next),
            ) => *value = next,
            _ => return Err(UiError::InvalidRequest),
        }
        Self::new(page)
    }
    /// Validated window title, not included in Debug/log output.
    pub fn title(&self) -> &str {
        &self.0.title
    }
    /// Initial logical viewport width.
    pub fn width(&self) -> f64 {
        self.0.width
    }
    /// Initial logical viewport height.
    pub fn height(&self) -> f64 {
        self.0.height
    }
    /// Immutable validated controls.
    pub fn nodes(&self) -> &[UiNativeNodeV1] {
        &self.0.nodes
    }
}
fn extent(value: f64) -> bool {
    value.is_finite() && (1.0..=8192.0).contains(&value)
}
fn text_budget(value: &str) -> UiResult<()> {
    if value.len() > 4096 {
        return Err(UiError::PayloadTooLarge);
    }
    if value.contains('\0') {
        return Err(UiError::InvalidRequest);
    }
    Ok(())
}

/// Typed presentation value, without implicit command dispatch or authorization.
/// No Debug implementation exposes editable content.
#[derive(Clone)]
pub enum UiNativeValue {
    /// Bounded editable text.
    Text(String),
    /// Checkbox state.
    Boolean(bool),
    /// Index into the declared option set.
    Selection(usize),
    /// Finite numeric value validated against the control interval.
    Number(f64),
}
