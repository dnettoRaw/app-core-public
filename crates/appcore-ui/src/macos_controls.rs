// =============================================================================
//        #######
//     ###       ###     F: macos_controls.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Typed system control factories and bounded native value access; no command dispatch.
use crate::{macos_callbacks::UiCallbacks, UiError, UiNativeNodeV1, UiNativeValue, UiResult};
use objc2::{rc::Retained, runtime::AnyObject, sel, MainThreadOnly};
use objc2_app_kit::{
    NSButton, NSPopUpButton, NSProgressIndicator, NSProgressIndicatorStyle, NSSlider, NSTextField,
    NSView,
};
use objc2_foundation::{MainThreadMarker, NSRect, NSString};
pub(crate) enum NativeControl {
    Label(Retained<NSTextField>),
    Input(Retained<NSTextField>),
    Button(Retained<NSButton>),
    Toggle(Retained<NSButton>),
    Select(Retained<NSPopUpButton>),
    Slider(Retained<NSSlider>),
    Progress(Retained<NSProgressIndicator>),
}
impl NativeControl {
    pub(crate) fn create(
        node: &UiNativeNodeV1,
        mtm: MainThreadMarker,
        target: Option<&UiCallbacks>,
    ) -> Self {
        let target: Option<&AnyObject> = target.map(|target| target.as_ref());
        match node {
            UiNativeNodeV1::Text { text, .. } => {
                Self::Label(NSTextField::labelWithString(&NSString::from_str(text), mtm))
            }
            UiNativeNodeV1::Input {
                value, placeholder, ..
            } => {
                let control = NSTextField::textFieldWithString(&NSString::from_str(value), mtm);
                control.setPlaceholderString(Some(&NSString::from_str(placeholder)));
                // SAFETY: Target is retained by window; fixed selector accepts the NSControl superclass.
                unsafe {
                    control.setTarget(target);
                    control.setAction(Some(sel!(activate:)));
                }
                Self::Input(control)
            }
            UiNativeNodeV1::Button { label, .. } => {
                // SAFETY: Retained target implements the fixed NSControl selector, with no payload cast.
                Self::Button(unsafe {
                    NSButton::buttonWithTitle_target_action(
                        &NSString::from_str(label),
                        target,
                        Some(sel!(activate:)),
                        mtm,
                    )
                })
            }
            UiNativeNodeV1::Toggle { label, checked, .. } => {
                // SAFETY: Same retained target and fixed NSControl selector as action buttons.
                let control = unsafe {
                    NSButton::checkboxWithTitle_target_action(
                        &NSString::from_str(label),
                        target,
                        Some(sel!(activate:)),
                        mtm,
                    )
                };
                control.setState(isize::from(*checked));
                Self::Toggle(control)
            }
            UiNativeNodeV1::Select {
                options, selected, ..
            } => {
                let control = NSPopUpButton::initWithFrame_pullsDown(
                    NSPopUpButton::alloc(mtm),
                    NSRect::default(),
                    false,
                );
                for option in options {
                    control.addItemWithTitle(&NSString::from_str(option));
                }
                control.selectItemAtIndex(*selected as isize);
                // SAFETY: Validated fixed target/action binding retained by window.
                unsafe {
                    control.setTarget(target);
                    control.setAction(Some(sel!(activate:)));
                }
                Self::Select(control)
            }
            UiNativeNodeV1::Slider {
                min, max, value, ..
            } => {
                // SAFETY: Finite validated range and fixed retained NSControl callback.
                Self::Slider(unsafe {
                    NSSlider::sliderWithValue_minValue_maxValue_target_action(
                        *value,
                        *min,
                        *max,
                        target,
                        Some(sel!(activate:)),
                        mtm,
                    )
                })
            }
            UiNativeNodeV1::Progress { value, .. } => {
                let control = NSProgressIndicator::initWithFrame(
                    NSProgressIndicator::alloc(mtm),
                    NSRect::default(),
                );
                control.setStyle(NSProgressIndicatorStyle::Bar);
                control.setIndeterminate(false);
                control.setMinValue(0.0);
                control.setMaxValue(1.0);
                control.setDoubleValue(*value);
                Self::Progress(control)
            }
        }
    }
    pub(crate) fn view(&self) -> Retained<NSView> {
        match self {
            Self::Label(c) | Self::Input(c) => c.clone().into_super().into_super(),
            Self::Button(c) | Self::Toggle(c) => c.clone().into_super().into_super(),
            Self::Select(c) => c.clone().into_super().into_super().into_super(),
            Self::Slider(c) => c.clone().into_super().into_super(),
            Self::Progress(c) => c.clone().into_super(),
        }
    }
    pub(crate) fn set_label(&self, value: &str) -> UiResult<()> {
        let value = NSString::from_str(value);
        match self {
            Self::Label(c) => c.setStringValue(&value),
            Self::Button(c) | Self::Toggle(c) => c.setTitle(&value),
            _ => return Err(UiError::InvalidRequest),
        }
        Ok(())
    }
    pub(crate) fn set_enabled(&self, value: bool) -> UiResult<()> {
        match self {
            Self::Input(c) => c.setEnabled(value),
            Self::Button(c) | Self::Toggle(c) => c.setEnabled(value),
            Self::Select(c) => c.setEnabled(value),
            Self::Slider(c) => c.setEnabled(value),
            _ => return Err(UiError::InvalidRequest),
        }
        Ok(())
    }
    pub(crate) fn set_value(&self, value: &UiNativeValue) -> UiResult<()> {
        match (self, value) {
            (Self::Input(c), UiNativeValue::Text(v)) => c.setStringValue(&NSString::from_str(v)),
            (Self::Toggle(c), UiNativeValue::Boolean(v)) => c.setState(isize::from(*v)),
            (Self::Select(c), UiNativeValue::Selection(v)) => c.selectItemAtIndex(*v as isize),
            (Self::Slider(c), UiNativeValue::Number(v)) => c.setDoubleValue(*v),
            (Self::Progress(c), UiNativeValue::Number(v)) => c.setDoubleValue(*v),
            _ => return Err(UiError::InvalidRequest),
        }
        Ok(())
    }
    pub(crate) fn value(&self) -> UiResult<UiNativeValue> {
        Ok(match self {
            Self::Input(c) => {
                let string = c.stringValue();
                if string.length() > 4096 {
                    return Err(UiError::PayloadTooLarge);
                }
                let value = string.to_string();
                if value.len() > 4096 {
                    return Err(UiError::PayloadTooLarge);
                }
                UiNativeValue::Text(value)
            }
            Self::Toggle(c) => UiNativeValue::Boolean(c.state() != 0),
            Self::Select(c) => UiNativeValue::Selection(
                usize::try_from(c.indexOfSelectedItem()).map_err(|_| UiError::InvalidRequest)?,
            ),
            Self::Slider(c) => UiNativeValue::Number(c.doubleValue()),
            Self::Progress(c) => UiNativeValue::Number(c.doubleValue()),
            _ => return Err(UiError::InvalidRequest),
        })
    }
}
