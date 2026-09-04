// =============================================================================
//        #######
//     ###       ###     F: source_page.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded source page contracts and behavior for this crate.

use serde::{Deserialize, Serialize};

use crate::source::{CollisionSource, ElementSource};
use crate::{Length, Orientation};

/// Page or canvas geometry and role-layer source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSource {
    /// Named preset, mutually exclusive with explicit dimensions.
    #[serde(default)]
    pub preset: Option<String>,
    /// Explicit width.
    #[serde(default)]
    pub width: Option<Length>,
    /// Explicit height.
    #[serde(default)]
    pub height: Option<Length>,
    /// Orientation applied after preset lookup.
    #[serde(default)]
    pub orientation: Orientation,
    /// Page margins.
    #[serde(default)]
    pub margin: EdgeSource,
    /// Bleed outside the trim box.
    #[serde(default)]
    pub bleed: EdgeSource,
    /// Safe inset inside the page.
    #[serde(default)]
    pub safe: EdgeSource,
    /// Request crop marks from compatible exporters.
    #[serde(default)]
    pub crop_marks: bool,
    /// Page collision policy inherited after the document policy.
    #[serde(default)]
    pub collision: Option<CollisionSource>,
    /// Layer composited on every physical page.
    #[serde(default)]
    pub master: PageLayerSource,
    /// Layer composited on the first page.
    #[serde(default)]
    pub first: PageLayerSource,
    /// Layer composited only on middle pages.
    #[serde(default)]
    pub continuation: PageLayerSource,
    /// Layer composited on the last page when the document has multiple pages.
    #[serde(default)]
    pub last: PageLayerSource,
}

impl PageSource {
    pub(crate) fn has_layer_elements(&self) -> bool {
        self.element_lists()
            .iter()
            .any(|elements| !elements.is_empty())
    }

    pub(crate) fn element_lists(&self) -> [&[ElementSource]; 12] {
        [
            &self.master.background,
            &self.master.header,
            &self.master.footer,
            &self.first.background,
            &self.first.header,
            &self.first.footer,
            &self.continuation.background,
            &self.continuation.header,
            &self.continuation.footer,
            &self.last.background,
            &self.last.header,
            &self.last.footer,
        ]
    }

    pub(crate) fn element_lists_mut(&mut self) -> [&mut Vec<ElementSource>; 12] {
        [
            &mut self.master.background,
            &mut self.master.header,
            &mut self.master.footer,
            &mut self.first.background,
            &mut self.first.header,
            &mut self.first.footer,
            &mut self.continuation.background,
            &mut self.continuation.header,
            &mut self.continuation.footer,
            &mut self.last.background,
            &mut self.last.header,
            &mut self.last.footer,
        ]
    }

    pub(crate) fn placed_element_lists(&self) -> [(crate::PagePlacement, &[ElementSource]); 12] {
        use crate::{PageBand::*, PageRole::*};
        [
            (placement(Master, Background), &self.master.background),
            (placement(Master, Header), &self.master.header),
            (placement(Master, Footer), &self.master.footer),
            (placement(First, Background), &self.first.background),
            (placement(First, Header), &self.first.header),
            (placement(First, Footer), &self.first.footer),
            (
                placement(Continuation, Background),
                &self.continuation.background,
            ),
            (placement(Continuation, Header), &self.continuation.header),
            (placement(Continuation, Footer), &self.continuation.footer),
            (placement(Last, Background), &self.last.background),
            (placement(Last, Header), &self.last.header),
            (placement(Last, Footer), &self.last.footer),
        ]
    }
}

const fn placement(role: crate::PageRole, band: crate::PageBand) -> crate::PagePlacement {
    crate::PagePlacement { role, band }
}

/// Elements painted in one semantic page layer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PageLayerSource {
    /// Elements painted behind normal body content.
    pub background: Vec<ElementSource>,
    /// Header elements painted above normal body content.
    pub header: Vec<ElementSource>,
    /// Footer elements painted above normal body content.
    pub footer: Vec<ElementSource>,
}

/// Four independently specified edges.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EdgeSource {
    /// Top edge.
    pub top: Option<Length>,
    /// Right edge.
    pub right: Option<Length>,
    /// Bottom edge.
    pub bottom: Option<Length>,
    /// Left edge.
    pub left: Option<Length>,
}
