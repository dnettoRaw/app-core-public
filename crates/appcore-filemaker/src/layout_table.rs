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
    ResolvedTableFragment, ResourceLimits, Result, Size, Style, StyleCascade, TablePage,
    TablePageSink, TablePaginator, TextEngine, TextLayout, TextOptions, TextOverflow, Unit,
    WritingMode,
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
    let mut measurer = FontTableMeasurer::new(fonts, element)?;
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
    ) -> Result<TextLayout>;
}

struct FontTableMeasurer<'a> {
    engine: TextEngine<'a>,
    min_font_size: Unit,
    line_height: u32,
    writing_mode: WritingMode,
}

impl<'a> FontTableMeasurer<'a> {
    fn new(fonts: &'a FontManager, element: &ElementIr) -> Result<Self> {
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
        })
    }

    fn options(
        &self,
        style: &ComputedStyle,
        bounds: Size,
        overflow: TextOverflow,
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
            line_height: self.line_height,
            writing_mode: self.writing_mode,
        })
    }
}

impl TableTextMeasurer for FontTableMeasurer<'_> {
    fn inline_width(&mut self, text: &str, style: &ComputedStyle, bounds: Size) -> Result<Unit> {
        Ok(self
            .engine
            .layout(text, &self.options(style, bounds, TextOverflow::Expand)?)?
            .measured
            .width)
    }

    fn natural_height(&mut self, text: &str, style: &ComputedStyle, bounds: Size) -> Result<Unit> {
        Ok(self
            .engine
            .layout(text, &self.options(style, bounds, TextOverflow::Expand)?)?
            .measured
            .height)
    }

    fn cell_layout(
        &mut self,
        text: &str,
        style: &ComputedStyle,
        bounds: Size,
    ) -> Result<TextLayout> {
        self.engine
            .layout(text, &self.options(style, bounds, TextOverflow::Wrap)?)
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
    let dataset = BorrowedDataset::new(&table.rows);
    let columns = resolve_table_columns(
        &table.spec,
        &dataset,
        bounds.size.width,
        logical_unit,
        &mut |text| measurer.inline_width(text, base_style, bounds.size),
    )?;
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
        header_height,
        fixed_row_height,
        limits.max_pages,
        &measurer,
    );
    TablePaginator {
        available_height: bounds.size.height,
        header_height,
        row_height: fixed_row_height.unwrap_or(Unit::ZERO),
        max_pages: limits.max_pages,
    }
    .paginate_measured(
        &table.spec,
        &dataset,
        &mut |row| {
            let mut measurer = measurer.borrow_mut();
            fixed_row_height.map_or_else(
                || {
                    measure_row(
                        row,
                        &columns,
                        base_style,
                        &table.spec,
                        bounds,
                        &mut **measurer,
                    )
                },
                Ok,
            )
        },
        &mut fragments,
    )?;
    if fragments.is_empty() {
        fragments.page(TablePage {
            index: 0,
            header: true,
            rows: Vec::new(),
            row_heights: Vec::new(),
            row_styles: Vec::new(),
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
    bounds: Rect,
    measurer: &mut dyn TableTextMeasurer,
) -> Result<Unit> {
    let style = compute_row_style(base_style, &spec.style_for(row)?)?;
    columns.iter().try_fold(Unit::ZERO, |height, column| {
        let text = row
            .get(&column.field)
            .map_or_else(String::new, DataValue::display);
        Ok(height.max(measurer.natural_height(
            &text,
            &style,
            Size::new(column.width, bounds.size.height)?,
        )?))
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
            x = x.checked_add(column.width)?;
            Ok(ResolvedTableCell {
                field: column.field.clone(),
                text: text.clone(),
                bounds,
                style: style.clone(),
                text_layout: measurer.cell_layout(&text, style, bounds.size)?,
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
            opacity: Some(base.opacity),
            font: base.font.clone(),
            font_size: Some(base.font_size),
            color: Some(base.color),
        },
        data_rule: data_rule.clone(),
        ..StyleCascade::default()
    }
    .compute()
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
