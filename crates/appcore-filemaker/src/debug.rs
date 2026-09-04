// =============================================================================
//        #######
//     ###       ###     F: debug.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded debug contracts and behavior for this crate.

use serde::{Deserialize, Serialize};

use crate::debug_geometry::{mask_free_regions, selected_bounds};
use crate::{
    Color, ElementId, ErrorCode, FileMakerError, Point, Rect, ResolvedElement, ResolvedPage,
    ResolvedScene, ResourceLimits, Result, SceneInspector, Size, Unit,
};

/// Geometry view used by a derived mask.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskView {
    /// Collision bounds.
    #[default]
    CollisionMask,
    /// Layout bounds.
    LayoutBounds,
    /// Visual bounds.
    VisualBounds,
    /// All distinct bounds.
    Combined,
}

/// Debug overlay switches. These never mutate a scene.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DebugOverlayOptions {
    /// Grid spacing; common values are 1/5/10/20 logical units.
    pub grid: Option<Unit>,
    /// Draw coordinate rulers.
    pub ruler: bool,
    /// Draw IDs.
    pub ids: bool,
    /// Draw resolved origin coordinates.
    pub coordinates: bool,
    /// Draw bounds.
    pub bounds: bool,
    /// Label retained anchor expressions.
    pub anchors: bool,
    /// Draw named region rectangles.
    pub regions: bool,
    /// Draw the page safe-area rectangle.
    pub safe_area: bool,
    /// Draw collidable geometry and named exclusions.
    pub collision: bool,
    /// Draw a crosshair at each element origin.
    pub crosshair: bool,
    /// Bounds class.
    pub view: MaskView,
}

/// Format-neutral debug primitive.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DebugPrimitive {
    /// Line segment.
    Line {
        /// Start.
        from: Point,
        /// End.
        to: Point,
        /// Stroke.
        color: Color,
    },
    /// Rectangle outline.
    Rect {
        /// Bounds.
        bounds: Rect,
        /// Stroke.
        color: Color,
    },
    /// Label.
    Label {
        /// Origin.
        origin: Point,
        /// UTF-8 label.
        text: String,
        /// Text color.
        color: Color,
    },
}

/// Derived overlay, separate from resolved scene elements.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DebugOverlay {
    /// Page index.
    pub page: usize,
    /// Debug-only primitives.
    pub primitives: Vec<DebugPrimitive>,
}

impl DebugOverlay {
    /// Builds an overlay without modifying scene geometry or paint order.
    pub fn build(
        scene: &ResolvedScene,
        page: usize,
        options: &DebugOverlayOptions,
    ) -> Result<Self> {
        Self::build_bounded(scene, page, options, &ResourceLimits::default())
    }

    /// Builds an overlay under the caller's scene and diagnostic budgets.
    pub fn build_bounded(
        scene: &ResolvedScene,
        page: usize,
        options: &DebugOverlayOptions,
        limits: &ResourceLimits,
    ) -> Result<Self> {
        crate::resolved::validate_scene_contract(scene, limits)?;
        let page_ref = scene
            .pages
            .get(page)
            .ok_or_else(|| debug_error("debug page was not found"))?;
        if let Some(grid) = options.grid {
            validate_grid(grid)?;
        }
        crate::debug_plan::validate_overlay(page_ref, options, limits)?;
        let mut primitives = Vec::new();
        if let Some(grid) = options.grid {
            add_grid(&mut primitives, page_ref.size, grid)?;
        }
        add_page_geometry(&mut primitives, page_ref, options)?;
        for element in &page_ref.elements {
            add_element_geometry(&mut primitives, element, options)?;
        }
        if options.ruler {
            add_ruler(
                &mut primitives,
                page_ref.size,
                options.grid.unwrap_or(Unit::points(10)?),
            )?;
        }
        Ok(Self { page, primitives })
    }
}

fn add_page_geometry(
    primitives: &mut Vec<DebugPrimitive>,
    page: &ResolvedPage,
    options: &DebugOverlayOptions,
) -> Result<()> {
    if options.safe_area {
        if let Some(bounds) = page
            .page_template
            .as_ref()
            .map(crate::PageTemplate::safe_bounds)
            .transpose()?
        {
            add_named_rect(
                primitives,
                "safe",
                bounds,
                Color::Rgb { r: 0, g: 128, b: 0 },
            );
        }
    }
    if options.regions {
        for region in &page.regions {
            add_named_rect(
                primitives,
                &format!("region:{}", region.name),
                region.bounds,
                Color::Rgb {
                    r: 0,
                    g: 96,
                    b: 192,
                },
            );
        }
    }
    if options.collision {
        for element in page.elements.iter().filter(|element| element.collidable) {
            add_named_rect(
                primitives,
                &format!("collision:{}", element.id.as_str()),
                element.bounds.collision,
                Color::Rgb {
                    r: 220,
                    g: 0,
                    b: 160,
                },
            );
        }
        for exclusion in &page.exclusions {
            add_named_rect(
                primitives,
                &format!("exclusion:{}", exclusion.name),
                exclusion.bounds,
                Color::Rgb {
                    r: 160,
                    g: 0,
                    b: 220,
                },
            );
        }
    }
    Ok(())
}

fn add_element_geometry(
    primitives: &mut Vec<DebugPrimitive>,
    element: &ResolvedElement,
    options: &DebugOverlayOptions,
) -> Result<()> {
    if options.bounds {
        for bounds in selected_bounds(element.bounds, options.view) {
            primitives.push(DebugPrimitive::Rect {
                bounds,
                color: Color::Rgba {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 160,
                },
            });
        }
    }
    if options.ids {
        primitives.push(DebugPrimitive::Label {
            origin: element.bounds.layout.origin,
            text: element.id.as_str().to_owned(),
            color: Color::Rgb { r: 180, g: 0, b: 0 },
        });
    }
    if options.coordinates {
        primitives.push(DebugPrimitive::Label {
            origin: element.bounds.layout.origin,
            text: format!(
                "({:.3}, {:.3}) pt",
                element.bounds.layout.origin.x.as_points_f64(),
                element.bounds.layout.origin.y.as_points_f64()
            ),
            color: Color::Rgb { r: 0, g: 0, b: 0 },
        });
    }
    if options.anchors {
        for (edge, expression) in &element.layout_trace.geometry.anchors {
            primitives.push(DebugPrimitive::Label {
                origin: element.bounds.layout.origin,
                text: format!("anchor:{edge}={expression}"),
                color: Color::Rgb {
                    r: 128,
                    g: 64,
                    b: 0,
                },
            });
        }
    }
    if options.crosshair {
        add_crosshair(primitives, element.bounds.layout.origin)?;
    }
    Ok(())
}

fn add_grid(primitives: &mut Vec<DebugPrimitive>, size: Size, spacing: Unit) -> Result<()> {
    let color = Color::Rgba {
        r: 0,
        g: 128,
        b: 255,
        a: 64,
    };
    let mut x = Unit::ZERO;
    while x <= size.width {
        primitives.push(DebugPrimitive::Line {
            from: Point { x, y: Unit::ZERO },
            to: Point { x, y: size.height },
            color,
        });
        x = x.checked_add(spacing)?;
    }
    let mut y = Unit::ZERO;
    while y <= size.height {
        primitives.push(DebugPrimitive::Line {
            from: Point { x: Unit::ZERO, y },
            to: Point { x: size.width, y },
            color,
        });
        y = y.checked_add(spacing)?;
    }
    Ok(())
}

fn validate_grid(grid: Unit) -> Result<()> {
    for spacing in [1, 5, 10, 20] {
        if grid == Unit::points(spacing)? {
            return Ok(());
        }
    }
    Err(debug_error("debug grid must be 1, 5, 10, or 20 points"))
}

fn add_ruler(primitives: &mut Vec<DebugPrimitive>, size: Size, spacing: Unit) -> Result<()> {
    let mut x = Unit::ZERO;
    while x <= size.width {
        primitives.push(DebugPrimitive::Label {
            origin: Point { x, y: Unit::ZERO },
            text: format!("{:.3}", x.as_points_f64()),
            color: Color::Rgb { r: 0, g: 0, b: 0 },
        });
        x = x.checked_add(spacing)?;
    }
    let mut y = Unit::ZERO;
    while y <= size.height {
        primitives.push(DebugPrimitive::Label {
            origin: Point { x: Unit::ZERO, y },
            text: format!("{:.3}", y.as_points_f64()),
            color: Color::Rgb { r: 0, g: 0, b: 0 },
        });
        y = y.checked_add(spacing)?;
    }
    Ok(())
}

fn add_named_rect(primitives: &mut Vec<DebugPrimitive>, name: &str, bounds: Rect, color: Color) {
    primitives.push(DebugPrimitive::Rect { bounds, color });
    primitives.push(DebugPrimitive::Label {
        origin: bounds.origin,
        text: name.to_owned(),
        color,
    });
}

/// JSON-serializable geometry-derived mask.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CollisionMask {
    /// Page index.
    pub page: usize,
    /// Page size.
    pub size: Size,
    /// Selected occupied rectangles.
    pub occupied: Vec<(ElementId, Rect)>,
    /// Disjoint free rectangles.
    pub free: Vec<Rect>,
    /// Pairwise collisions.
    pub collisions: Vec<(ElementId, ElementId, Rect)>,
    /// Visual overflow IDs.
    pub overflow: Vec<ElementId>,
}

impl CollisionMask {
    /// Derives a mask from resolved geometry; bitmap pixels are never queried.
    pub fn derive(scene: &ResolvedScene, page: usize, view: MaskView) -> Result<Self> {
        Self::derive_bounded(scene, page, view, &ResourceLimits::default())
    }

    /// Derives a mask under the caller's scene and diagnostic geometry budgets.
    pub fn derive_bounded(
        scene: &ResolvedScene,
        page: usize,
        view: MaskView,
        limits: &ResourceLimits,
    ) -> Result<Self> {
        crate::resolved::validate_scene_contract(scene, limits)?;
        let page_ref = scene
            .pages
            .get(page)
            .ok_or_else(|| debug_error("mask page was not found"))?;
        let mut budget = crate::diagnostic_budget::DiagnosticBudget::new(limits)?;
        let mut occupied = Vec::new();
        for element in page_ref
            .elements
            .iter()
            .filter(|element| view != MaskView::CollisionMask || element.collidable)
        {
            let mut element_bounds = Vec::new();
            for bounds in selected_bounds(element.bounds, view) {
                if !element_bounds.contains(&bounds) {
                    budget.retained(occupied.len().saturating_add(1))?;
                    occupied.push((element.id.clone(), bounds));
                    element_bounds.push(bounds);
                }
            }
        }
        if matches!(view, MaskView::CollisionMask | MaskView::Combined) {
            for exclusion in &page_ref.exclusions {
                budget.retained(occupied.len().saturating_add(1))?;
                occupied.push((
                    ElementId::new(format!("exclusion.{}", exclusion.name))?,
                    exclusion.bounds,
                ));
            }
        }
        let mut collisions = Vec::new();
        for (index, (left_id, left)) in occupied.iter().enumerate() {
            for (right_id, right) in &occupied[index + 1..] {
                if left_id == right_id {
                    continue;
                }
                budget.operation()?;
                if let Some(overlap) = left.intersection(*right)? {
                    budget.retained(collisions.len().saturating_add(1))?;
                    collisions.push((left_id.clone(), right_id.clone(), overlap));
                }
            }
        }
        let free = mask_free_regions(page_ref.size, &occupied, &mut budget)?;
        let overflow = SceneInspector::new(scene).inspect_page(page)?.overflow;
        Ok(Self {
            page,
            size: page_ref.size,
            occupied,
            free,
            collisions,
            overflow,
        })
    }

    /// Serializes stable geometry JSON.
    pub fn to_json(&self) -> Result<Vec<u8>> {
        self.to_json_bounded(&ResourceLimits::default())
    }

    /// Serializes stable geometry JSON under caller-supplied budgets.
    pub fn to_json_bounded(&self, limits: &ResourceLimits) -> Result<Vec<u8>> {
        self.validate_limits(limits)?;
        let size = crate::memory::serialized_size_pretty(self)?;
        if size > limits.max_output_bytes {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "debug mask JSON exceeds the output budget",
            ));
        }
        let mut bytes = Vec::with_capacity(size);
        serde_json::to_writer_pretty(&mut bytes, self)
            .map_err(|error| debug_error(format!("cannot encode mask JSON: {error}")))?;
        debug_assert_eq!(bytes.len(), size);
        Ok(bytes)
    }

    pub(crate) fn validate_limits(&self, limits: &ResourceLimits) -> Result<()> {
        limits.validate()?;
        let retained = self
            .occupied
            .len()
            .checked_add(self.free.len())
            .and_then(|count| count.checked_add(self.collisions.len()))
            .and_then(|count| count.checked_add(self.overflow.len()))
            .ok_or_else(|| {
                FileMakerError::new(ErrorCode::LimitExceeded, "debug mask count overflow")
            })?;
        crate::diagnostic_budget::DiagnosticBudget::new(limits)?.retained(retained)
    }
}

fn add_crosshair(primitives: &mut Vec<DebugPrimitive>, origin: Point) -> Result<()> {
    let arm = Unit::points(3)?;
    primitives.push(DebugPrimitive::Line {
        from: Point {
            x: origin.x.checked_sub(arm)?,
            y: origin.y,
        },
        to: Point {
            x: origin.x.checked_add(arm)?,
            y: origin.y,
        },
        color: Color::Rgb {
            r: 255,
            g: 0,
            b: 255,
        },
    });
    primitives.push(DebugPrimitive::Line {
        from: Point {
            x: origin.x,
            y: origin.y.checked_sub(arm)?,
        },
        to: Point {
            x: origin.x,
            y: origin.y.checked_add(arm)?,
        },
        color: Color::Rgb {
            r: 255,
            g: 0,
            b: 255,
        },
    });
    Ok(())
}

fn debug_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}
