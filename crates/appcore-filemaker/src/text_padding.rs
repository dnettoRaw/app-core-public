// =============================================================================
//        #######
//     ###       ###     F: text_padding.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Resolves standalone text-block padding and exporter content geometry.

use serde::{Deserialize, Serialize};

use crate::{ErrorCode, FileMakerError, Insets, Length, Rect, Result, Size, TextLayout, Unit};

/// Declarative per-side padding around a text element's measured content.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextBlockPadding {
    /// Top inset, relative percentages use the element height.
    pub top: Length,
    /// Right inset, relative percentages use the element width.
    pub right: Length,
    /// Bottom inset, relative percentages use the element height.
    pub bottom: Length,
    /// Left inset, relative percentages use the element width.
    pub left: Length,
}

impl Default for TextBlockPadding {
    fn default() -> Self {
        let zero = Length::Absolute(Unit::ZERO);
        Self {
            top: zero,
            right: zero,
            bottom: zero,
            left: zero,
        }
    }
}

impl TextBlockPadding {
    pub(crate) fn resolve(self, width: Unit, height: Unit, logical_unit: Unit) -> Result<Insets> {
        let resolve = |value: Length, percent_base: Unit| {
            value
                .resolve(percent_base, logical_unit)?
                .ok_or_else(|| padding_error("text block padding cannot be auto"))
        };
        Ok(Insets {
            top: resolve(self.top, height)?,
            right: resolve(self.right, width)?,
            bottom: resolve(self.bottom, height)?,
            left: resolve(self.left, width)?,
        })
    }
}

impl TextLayout {
    /// Returns the measured outer size including block padding.
    pub fn outer_measured(&self) -> Result<Size> {
        Size::new(
            self.measured
                .width
                .checked_add(self.padding.left.checked_add(self.padding.right)?)?,
            self.measured
                .height
                .checked_add(self.padding.top.checked_add(self.padding.bottom)?)?,
        )
    }

    /// Returns the inner text rectangle after applying block padding.
    pub fn content_bounds(&self, bounds: Rect) -> Result<Rect> {
        let x = bounds.origin.x.checked_add(self.padding.left)?;
        let y = bounds.origin.y.checked_add(self.padding.top)?;
        let width = bounds
            .size
            .width
            .checked_sub(self.padding.left.checked_add(self.padding.right)?)?;
        let height = bounds
            .size
            .height
            .checked_sub(self.padding.top.checked_add(self.padding.bottom)?)?;
        if width <= Unit::ZERO || height <= Unit::ZERO {
            return Err(padding_error("text block padding leaves no content area"));
        }
        Rect::new(x, y, width, height)
    }
}

fn padding_error(message: &'static str) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}
