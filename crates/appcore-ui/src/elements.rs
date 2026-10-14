// =============================================================================
//        #######
//     ###       ###     F: elements.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Function-based native elements. Construction stays separate from page validation and behavior.
use crate::{UiNativeNodeV1, UiRectangle};
/// Creates a native text declaration; validate it with `UiNativePage::new` before rendering.
pub fn text(id: impl Into<String>, rect: UiRectangle, value: impl Into<String>) -> UiNativeNodeV1 {
    UiNativeNodeV1::Text {
        id: id.into(),
        rect,
        text: value.into(),
    }
}
/// Creates a button bound to an action identity, without dispatching or granting it.
/// Validate the enclosing page before rendering; application commands own behavior.
pub fn button(
    id: impl Into<String>,
    rect: UiRectangle,
    label: impl Into<String>,
    action: impl Into<String>,
) -> UiNativeNodeV1 {
    UiNativeNodeV1::Button {
        id: id.into(),
        rect,
        label: label.into(),
        action: action.into(),
    }
}
/// Creates a bounded editable-field declaration; Return emits only the submit action identity.
pub fn input(
    id: impl Into<String>,
    rect: UiRectangle,
    value: impl Into<String>,
    placeholder: impl Into<String>,
    action: impl Into<String>,
) -> UiNativeNodeV1 {
    UiNativeNodeV1::Input {
        id: id.into(),
        rect,
        value: value.into(),
        placeholder: placeholder.into(),
        action: action.into(),
    }
}
/// Creates a native checkbox declaration; the application owns state transitions.
pub fn toggle(
    id: impl Into<String>,
    rect: UiRectangle,
    label: impl Into<String>,
    checked: bool,
    action: impl Into<String>,
) -> UiNativeNodeV1 {
    UiNativeNodeV1::Toggle {
        id: id.into(),
        rect,
        label: label.into(),
        checked,
        action: action.into(),
    }
}
/// Creates a dropdown declaration with an immutable bounded option list.
pub fn select(
    id: impl Into<String>,
    rect: UiRectangle,
    options: Vec<String>,
    selected: usize,
    action: impl Into<String>,
) -> UiNativeNodeV1 {
    UiNativeNodeV1::Select {
        id: id.into(),
        rect,
        options,
        selected,
        action: action.into(),
    }
}
/// Creates a finite slider declaration; validate the enclosing page before rendering.
pub fn slider(
    id: impl Into<String>,
    rect: UiRectangle,
    min: f64,
    max: f64,
    value: f64,
    action: impl Into<String>,
) -> UiNativeNodeV1 {
    UiNativeNodeV1::Slider {
        id: id.into(),
        rect,
        min,
        max,
        value,
        action: action.into(),
    }
}
/// Creates determinate progress presentation, without scheduling any work.
pub fn progress(id: impl Into<String>, rect: UiRectangle, value: f64) -> UiNativeNodeV1 {
    UiNativeNodeV1::Progress {
        id: id.into(),
        rect,
        value,
    }
}
