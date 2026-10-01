// =============================================================================
//        #######
//     ###       ###     F: layout_table.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded layout table contracts and behavior for this crate.

use crate::layout_table_stream::FragmentCollector;
use crate::{
    resolve_table_columns, BorrowedDataset, ComputedStyle, DataRow, DataValue, ElementIr,
    ErrorCode, FileMakerError, FontManager, Length, Rect, ResolvedTableCell, ResolvedTableColumn,
    ResolvedTableFragment, ResourceLimits, Result, Size, Style, StyleCascade, TableIr, TablePage,
    TablePageBody, TablePageSink, TablePaginator, TextEngine, TextLayout, TextOptions,
    TextOverflow, Unit, WritingMode,
};
use std::cell::RefCell;

pub(crate) fn resolve_table_fragments(
    element: &ElementIr,
    bounds: Rect,
    fonts: &FontManager,
    limits: &ResourceLimits,
    logical_unit: Unit,
) -> Result<Vec<ResolvedTableFragment>> {
    let table = element
        .table
        .as_ref()
        .ok_or_else(|| table_layout_error("table element has no table intent"))?;
    let base_style = StyleCascade {
        template: element.style.clone(),
        ..StyleCascade::default()
    }
    .compute()?;
    let mut measurer = FontTableMeasurer::new(fonts, element, logical_unit)?;
    resolve_with_measurer(
        table,
        bounds,
        &base_style,
        limits,
        logical_unit,
        &mut measurer,
    )
}

pub(crate) trait TableTextMeasurer {
    fn inline_width(&mut self, text: &str, style: &ComputedStyle, bounds: Size) -> Result<Unit>;
    fn natural_height(&mut self, text: &str, style: &ComputedStyle, bounds: Size) -> Result<Unit>;
    fn cell_layout(
        &mut self,
        text: &str,
        style: &ComputedStyle,
        bounds: Size,
        align_x: crate::Alignment,
    ) -> Result<TextLayout>;
}

struct FontTableMeasurer<'a> {
    engine: TextEngine<'a>,
    min_font_size: Unit,
    line_height: u32,
    writing_mode: WritingMode,
    padding_inline: Length,
    logical_unit: Unit,
}

impl<'a> FontTableMeasurer<'a> {
    fn new(fonts: &'a FontManager, element: &ElementIr, logical_unit: Unit) -> Result<Self> {
        let min_font_size =
            element
                .text_options
                .min_font_size
                .map_or(Ok(Unit::from_raw(6_000_000)), |length| {
                    length
                        .resolve(Unit::ZERO, Unit::ZERO)?
                        .ok_or_else(|| table_layout_error("table minimum font size cannot be auto"))
                })?;
        Ok(Self {
            engine: TextEngine::new(fonts),
            min_font_size,
            line_height: element.text_options.line_height,
            writing_mode: element.text_options.writing_mode,
            padding_inline: element.text_options.padding_inline,
            logical_unit,
        })
    }

    fn options(
        &self,
        style: &ComputedStyle,
        bounds: Size,
        overflow: TextOverflow,
        align_x: crate::Alignment,
    ) -> Result<TextOptions> {
        Ok(TextOptions {
            font: style.font.clone().ok_or_else(|| {
                FileMakerError::new(
                    ErrorCode::FontMissing,
                    "table text requires an explicit font",
                )
            })?,
            font_size: style.font_size,
            min_font_size: self.min_font_size.min(style.font_size),
            bounds,
            max_lines: None,
            overflow,
            line_height: style.line_height.unwrap_or(self.line_height),
            writing_mode: self.writing_mode,
            align_x,
            padding_inline: self
                .padding_inline
                .resolve(bounds.width, self.logical_unit)?
                .ok_or_else(|| table_layout_error("table inline padding cannot be auto"))?,
        })
    }
}

impl TableTextMeasurer for FontTableMeasurer<'_> {
    fn inline_width(&mut self, text: &str, style: &ComputedStyle, bounds: Size) -> Result<Unit> {
        Ok(self
            .engine
            .layout(
                text,
                &self.options(style, bounds, TextOverflow::Expand, crate::Alignment::Start)?,
            )?
            .measured
            .width)
    }

    fn natural_height(&mut self, text: &str, style: &ComputedStyle, bounds: Size) -> Result<Unit> {
        Ok(self
            .engine
            .layout(
                text,
                &self.options(style, bounds, TextOverflow::Expand, crate::Alignment::Start)?,
            )?
            .measured
            .height)
    }

    fn cell_layout(
        &mut self,
        text: &str,
        style: &ComputedStyle,
        bounds: Size,
        align_x: crate::Alignment,
    ) -> Result<TextLayout> {
        self.engine.layout(
            text,
            &self.options(style, bounds, TextOverflow::Wrap, align_x)?,
        )
    }
}

pub(crate) fn resolve_with_measurer(
    table: &crate::TableIr,
    bounds: Rect,
    base_style: &ComputedStyle,
    limits: &ResourceLimits,
    logical_unit: Unit,
    measurer: &mut dyn TableTextMeasurer,
) -> Result<Vec<ResolvedTableFragment>> {
    let first_bounds = page_body_bounds(table, bounds, 0, logical_unit)?;
    let continuation_bounds = page_body_bounds(table, bounds, 1, logical_unit)?;
    let dataset = BorrowedDataset::new(&table.rows);
    let columns = resolve_table_columns(
        &table.spec,
        &dataset,
        bounds.size.width,
        logical_unit,
        &mut |text| measurer.inline_width(text, base_style, bounds.size),
    )?;
    for resolved in &columns {
        let horizontal = resolved.padding.left.checked_add(resolved.padding.right)?;
        if horizontal >= resolved.width {
            return Err(table_layout_error(
                "horizontal cell padding must leave positive content width",
            ));
        }
    }
    let header_height = resolve_fixed(table.header_height, bounds.size.height, logical_unit)?;
    let fixed_row_height = table
        .row_height
        .filter(|height| !matches!(height, Length::Auto))
        .map(|height| resolve_fixed(height, bounds.size.height, logical_unit))
        .transpose()?;
    let measurer = RefCell::new(measurer);
    let mut fragments = FragmentCollector::new(
        &columns,
        base_style,
        &table.spec,
        bounds,
        first_bounds,
        continuation_bounds,
        header_height,
        fixed_row_height,
        logical_unit,
        limits.max_pages,
        &measurer,
    );
    TablePaginator {
        available_height: continuation_bounds.size.height,
        header_height,
        row_height: fixed_row_height.unwrap_or(Unit::ZERO),
        max_pages: limits.max_pages,
    }
    .paginate_measured_with_page_index(
        &table.spec,
        &dataset,
        &mut |page_index, row| {
            let mut measurer = measurer.borrow_mut();
            let measured = fixed_row_height.map_or_else(
                || {
                    measure_row(
                        row,
                        &columns,
                        base_style,
                        &table.spec,
                        TableMeasureContext {
                            page_index,
                            bounds,
                            logical_unit,
                        },
                        &mut **measurer,
                    )
                },
                Ok,
            )?;
            let minimum = table.spec.minimum_row_height_for(
                row,
                page_index,
                bounds.size.height,
                logical_unit,
            )?;
            Ok(minimum.map_or(measured, |minimum| measured.max(minimum)))
        },
        &mut fragments,
        first_bounds.size.height,
        true,
    )?;
    if fragments.is_empty() {
        fragments.page(TablePage {
            index: 0,
            header: true,
            rows: Vec::new(),
            row_heights: Vec::new(),
            row_styles: Vec::new(),
            row_padding: Vec::new(),
            group_starts: Vec::new(),
            starting_group: None,
            totals: Default::default(),
        })?;
    }
    Ok(fragments.finish())
}

pub(crate) fn measure_row(
    row: &DataRow,
    columns: &[ResolvedTableColumn],
    base_style: &ComputedStyle,
    spec: &crate::TableSpec,
    context: TableMeasureContext,
    measurer: &mut dyn TableTextMeasurer,
) -> Result<Unit> {
    let TableMeasureContext {
        page_index,
        bounds,
        logical_unit,
    } = context;
    let style = compute_row_style(base_style, &spec.style_for(row)?)?;
    let row_padding = spec.padding_for(row, page_index)?.resolve(logical_unit)?;
    columns.iter().try_fold(Unit::ZERO, |height, column| {
        let text = row
            .get(&column.field)
            .map_or_else(String::new, DataValue::display);
        let padding = add_padding(column.padding, row_padding)?;
        let content_width = column
            .width
            .checked_sub(padding.left.checked_add(padding.right)?)?;
        let content_height = measurer
            .natural_height(&text, &style, Size::new(content_width, bounds.size.height)?)?
            .checked_add(padding.top.checked_add(padding.bottom)?)?;
        Ok(height.max(content_height))
    })
}

#[derive(Clone, Copy)]
pub(crate) struct TableMeasureContext {
    pub page_index: usize,
    pub bounds: Rect,
    pub logical_unit: Unit,
}

pub(crate) fn add_padding(left: crate::Insets, right: crate::Insets) -> Result<crate::Insets> {
    Ok(crate::Insets {
        top: left.top.checked_add(right.top)?,
        right: left.right.checked_add(right.right)?,
        bottom: left.bottom.checked_add(right.bottom)?,
        left: left.left.checked_add(right.left)?,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_cells(
    columns: &[ResolvedTableColumn],
    row: &DataRow,
    style: &ComputedStyle,
    mut x: Unit,
    y: Unit,
    height: Unit,
    header: bool,
    row_padding: crate::Insets,
    text_offset_y: Unit,
    measurer: &mut dyn TableTextMeasurer,
) -> Result<Vec<ResolvedTableCell>> {
    columns
        .iter()
        .map(|column| {
            let text = if header {
                column.header.clone()
            } else {
                row.get(&column.field)
                    .map_or_else(String::new, DataValue::display)
            };
            let bounds = Rect::new(x, y, column.width, height)?;
            let padding = add_padding(column.padding, row_padding)?;
            let content_width = bounds
                .size
                .width
                .checked_sub(padding.left.checked_add(padding.right)?)?;
            let content_height = bounds
                .size
                .height
                .checked_sub(padding.top.checked_add(padding.bottom)?)?;
            if content_width <= Unit::ZERO || content_height <= Unit::ZERO {
                return Err(table_layout_error(
                    "cell padding must leave positive content bounds",
                ));
            }
            x = x.checked_add(column.width)?;
            let mut text_layout = measurer.cell_layout(
                &text,
                style,
                Size::new(content_width, content_height)?,
                column.align_x,
            )?;
            let offset_magnitude = text_offset_y.raw().unsigned_abs();
            if text_offset_y != Unit::ZERO
                && (offset_magnitude >= text_layout.font_size.raw().unsigned_abs()
                    || offset_magnitude >= content_height.raw().unsigned_abs())
            {
                return Err(table_layout_error(
                    "table text offset must be smaller than the cell font and content height",
                ));
            }
            text_layout.paint_offset_y = text_offset_y;
            Ok(ResolvedTableCell {
                field: column.field.clone(),
                text: text.clone(),
                bounds,
                padding,
                style: style.clone(),
                text_layout,
            })
        })
        .collect()
}

pub(crate) fn compute_row_style(base: &ComputedStyle, data_rule: &Style) -> Result<ComputedStyle> {
    StyleCascade {
        defaults: Style {
            fill: base.fill,
            stroke: base.stroke,
            stroke_width: Some(base.stroke_width),
            stroke_sides: Some(base.stroke_sides),
            opacity: Some(base.opacity),
            font: base.font.clone(),
            font_size: Some(base.font_size),
            line_height: base.line_height,
            color: Some(base.color),
            underline: Some(base.underline),
        },
        data_rule: data_rule.clone(),
        ..StyleCascade::default()
    }
    .compute()
}

pub(crate) fn page_body_bounds(
    table: &TableIr,
    bounds: Rect,
    page_index: usize,
    logical_unit: Unit,
) -> Result<Rect> {
    let Some(page_bodies) = &table.page_bodies else {
        return Ok(bounds);
    };
    let body: &TablePageBody = if page_index == 0 {
        &page_bodies.first
    } else {
        &page_bodies.continuation
    };
    let offset = body
        .offset_y
        .resolve(bounds.size.height, logical_unit)?
        .ok_or_else(|| table_layout_error("table page body offset cannot be auto"))?;
    let height = body
        .height
        .resolve(bounds.size.height, logical_unit)?
        .ok_or_else(|| table_layout_error("table page body height cannot be auto"))?;
    let end = offset.checked_add(height)?;
    if offset < Unit::ZERO || height <= Unit::ZERO || end > bounds.size.height {
        return Err(table_layout_error(
            "table page body must fit inside the table element bounds",
        ));
    }
    Rect::new(
        bounds.origin.x,
        bounds.origin.y.checked_add(offset)?,
        bounds.size.width,
        height,
    )
}

fn resolve_fixed(length: Length, reference: Unit, logical_unit: Unit) -> Result<Unit> {
    let value = length
        .resolve(reference, logical_unit)?
        .ok_or_else(|| table_layout_error("table dimension cannot be auto"))?;
    if value <= Unit::ZERO {
        return Err(table_layout_error("table dimension must be positive"));
    }
    Ok(value)
}

fn table_layout_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}

pub(crate) fn translate_table_fragment(
    fragment: &mut ResolvedTableFragment,
    from: crate::Point,
    to: crate::Point,
) -> Result<()> {
    let dx = to.x.checked_sub(from.x)?;
    let dy = to.y.checked_sub(from.y)?;
    for cell in fragment.header.iter_mut().chain(fragment.totals.iter_mut()) {
        translate_rect(&mut cell.bounds, dx, dy)?;
    }
    for row in &mut fragment.rows {
        translate_rect(&mut row.bounds, dx, dy)?;
        for cell in &mut row.cells {
            translate_rect(&mut cell.bounds, dx, dy)?;
        }
    }
    Ok(())
}

fn translate_rect(rect: &mut Rect, dx: Unit, dy: Unit) -> Result<()> {
    rect.origin.x = rect.origin.x.checked_add(dx)?;
    rect.origin.y = rect.origin.y.checked_add(dy)?;
    Ok(())
}
