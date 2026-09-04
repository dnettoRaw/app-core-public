// =============================================================================
//        #######
//     ###       ###     F: page.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded page contracts and behavior for this crate.

use serde::{Deserialize, Serialize};

use crate::{Insets, Size};

/// Semantic page role used by paginated templates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageRole {
    /// First page.
    First,
    /// Middle/continuation page.
    Continuation,
    /// Last page.
    Last,
    /// Master background/header/footer.
    Master,
}

/// Semantic band within a page layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageBand {
    /// Paint behind body content.
    Background,
    /// Repeating or role-specific header content.
    Header,
    /// Repeating or role-specific footer content.
    Footer,
}

/// Placement assigned to a root element owned by a page layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PagePlacement {
    /// Master, first, continuation, or last layer.
    pub role: PageRole,
    /// Background, header, or footer band.
    pub band: PageBand,
}

/// Resolved page template metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PageTemplate {
    /// Stable template name.
    pub name: String,
    /// Semantic role.
    pub role: PageRole,
    /// Trim size.
    pub size: Size,
    /// Content margins.
    pub margin: Insets,
    /// Bleed extents.
    pub bleed: Insets,
    /// Safe-area inset.
    pub safe: Insets,
    /// Whether crop marks are requested at compatible export.
    pub crop_marks: bool,
}

impl PageTemplate {
    /// Returns the content rectangle after applying margins to the trim box.
    pub fn content_bounds(&self) -> crate::Result<crate::Rect> {
        let width = self
            .size
            .width
            .checked_sub(self.margin.left)?
            .checked_sub(self.margin.right)?;
        let height = self
            .size
            .height
            .checked_sub(self.margin.top)?
            .checked_sub(self.margin.bottom)?;
        crate::Rect::new(self.margin.left, self.margin.top, width, height)
    }

    /// Returns the safe-area rectangle inside the trim box.
    pub fn safe_bounds(&self) -> crate::Result<crate::Rect> {
        let width = self
            .size
            .width
            .checked_sub(self.safe.left)?
            .checked_sub(self.safe.right)?;
        let height = self
            .size
            .height
            .checked_sub(self.safe.top)?
            .checked_sub(self.safe.bottom)?;
        crate::Rect::new(self.safe.left, self.safe.top, width, height)
    }
}

/// Optional first/continuation/last/master page roles.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PageTemplateSet {
    /// First-page override.
    pub first: Option<PageTemplate>,
    /// Continuation page.
    pub continuation: Option<PageTemplate>,
    /// Last-page override.
    pub last: Option<PageTemplate>,
    /// Master composited beneath each page.
    pub master: Option<PageTemplate>,
}

impl PageTemplateSet {
    /// Builds all semantic page templates from one validated geometry contract.
    #[must_use]
    pub fn from_base(base: &PageTemplate) -> Self {
        let with_role = |role| {
            let mut template = base.clone();
            template.role = role;
            template
        };
        Self {
            first: Some(with_role(PageRole::First)),
            continuation: Some(with_role(PageRole::Continuation)),
            last: Some(with_role(PageRole::Last)),
            master: Some(with_role(PageRole::Master)),
        }
    }

    /// Selects the deterministic template for a zero-based page.
    #[must_use]
    pub fn select(&self, index: usize, total: usize) -> Option<&PageTemplate> {
        if index == 0 {
            self.first.as_ref().or(self.continuation.as_ref())
        } else if index + 1 == total {
            self.last.as_ref().or(self.continuation.as_ref())
        } else {
            self.continuation.as_ref()
        }
    }

    /// Replaces page-number placeholders without locale-dependent formatting.
    #[must_use]
    pub fn number_text(source: &str, index: usize, total: usize) -> String {
        source
            .replace("{page}", &(index + 1).to_string())
            .replace("{pages}", &total.to_string())
    }

    /// Returns whether a layer role is active on this physical page.
    #[must_use]
    pub fn role_is_active(role: PageRole, index: usize, total: usize) -> bool {
        match role {
            PageRole::Master => true,
            PageRole::First => index == 0,
            PageRole::Continuation => index > 0 && index + 1 < total,
            PageRole::Last => total > 1 && index + 1 == total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_numbers_and_roles_are_deterministic() {
        assert_eq!(
            PageTemplateSet::number_text("Page {page}/{pages}", 1, 3),
            "Page 2/3"
        );
        assert!(PageTemplateSet::role_is_active(PageRole::Master, 2, 3));
        assert!(PageTemplateSet::role_is_active(
            PageRole::Continuation,
            1,
            3
        ));
        assert!(!PageTemplateSet::role_is_active(PageRole::Last, 0, 1));
    }
}
