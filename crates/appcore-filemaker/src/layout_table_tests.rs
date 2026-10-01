// =============================================================================
//        #######
//     ###       ###     F: layout_table_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeMap;

use crate::layout_table::{resolve_with_measurer, TableTextMeasurer};
use crate::{
    Color, ColumnWidth, ComputedStyle, DataValue, Length, Rect, ResourceLimits, Result, Size,
    Style, TableColumn, TableIr, TableSpec, TableStyleRule, TextLayout, Unit,
};

struct MockText;

impl TableTextMeasurer for MockText {
    fn inline_width(&mut self, text: &str, _: &ComputedStyle, _: Size) -> Result<Unit> {
        Unit::points(i64::try_from(text.chars().count()).unwrap_or(i64::MAX))
    }

    fn natural_height(&mut self, text: &str, _: &ComputedStyle, bounds: Size) -> Result<Unit> {
        let text_width = i64::try_from(text.chars().count())
            .unwrap_or(i64::MAX)
            .saturating_mul(5);
        let column_points = (bounds.width.raw() / Unit::PER_POINT).max(1);
        let numerator = text_width.max(1).saturating_add(column_points - 1);
        let lines = numerator / column_points;
        Unit::points(lines.saturating_mul(10))
    }

    fn cell_layout(
        &mut self,
        _: &str,
        style: &ComputedStyle,
        bounds: Size,
        align_x: crate::Alignment,
    ) -> Result<TextLayout> {
        Ok(TextLayout {
            writing_mode: crate::WritingMode::Horizontal,
            lines: Vec::new(),
            measured: bounds,
            font_size: style.font_size,
            paint_offset_y: Unit::ZERO,
            diagnostics: Vec::new(),
            align_x,
            padding_inline: Unit::ZERO,
            padding: crate::Insets::default(),
        })
    }
}

#[test]
fn table_ir_resolves_paginated_cells_styles_groups_and_totals() {
    let (table, bounds, style) = table_fixture();
    let fragments = resolve_with_measurer(
        &table,
        bounds,
        &style,
        &ResourceLimits::default(),
        Unit::points(1).unwrap(),
        &mut MockText,
    )
    .unwrap();

    assert_eq!(fragments.len(), 2);
    assert!(fragments.iter().all(|page| page.header.len() == 3));
    assert_eq!(fragments[0].rows.len(), 3);
    assert_eq!(fragments[1].rows[0].source_index, 3);
    assert_eq!(fragments[1].starting_group.as_deref(), Some("B"));
    assert_eq!(fragments[1].rows[0].group_start, None);
    assert_eq!(
        fragments[0].rows[1].style.fill,
        Some(Color::parse("red").unwrap())
    );
    assert_eq!(fragments[1].totals[2].text, "10");
    let width = fragments[0]
        .columns
        .iter()
        .try_fold(Unit::ZERO, |sum, column| sum.checked_add(column.width))
        .unwrap();
    assert_eq!(width, bounds.size.width);
    assert_eq!(
        fragments[0].rows[0].bounds.origin.y,
        Unit::points(8).unwrap()
    );
}

fn table_fixture() -> (TableIr, Rect, ComputedStyle) {
    let rows = [
        ("A", "One", 1),
        ("A", "Two", 2),
        ("B", "Three", 3),
        ("B", "Four", 4),
    ]
    .into_iter()
    .map(|(group, name, amount)| {
        BTreeMap::from([
            ("group".to_owned(), DataValue::String(group.to_owned())),
            ("name".to_owned(), DataValue::String(name.to_owned())),
            ("amount".to_owned(), DataValue::Integer(amount)),
        ])
    })
    .collect();
    let table = TableIr {
        spec: TableSpec {
            columns: vec![
                TableColumn {
                    field: "group".to_owned(),
                    header: "Group".to_owned(),
                    width: ColumnWidth::Fixed(Length::Absolute(Unit::points(20).unwrap())),
                    align_x: crate::Alignment::Start,
                    padding: crate::CellPadding::default(),
                },
                TableColumn {
                    field: "name".to_owned(),
                    header: "Name".to_owned(),
                    width: ColumnWidth::Flex(1),
                    align_x: crate::Alignment::Start,
                    padding: crate::CellPadding::default(),
                },
                TableColumn {
                    field: "amount".to_owned(),
                    header: "Amount".to_owned(),
                    width: ColumnWidth::Auto,
                    align_x: crate::Alignment::Start,
                    padding: crate::CellPadding::default(),
                },
            ],
            repeat_header: true,
            group_by: Some("group".to_owned()),
            keep_together_by: None,
            row_anchor_field: None,
            total_fields: vec!["amount".to_owned()],
            conditional_styles: vec![TableStyleRule {
                when: "data.amount == 2".to_owned(),
                style: Style {
                    fill: Some(Color::parse("red").unwrap()),
                    ..Style::default()
                },
                padding: None,
                padding_first_page: None,
                padding_continuation: None,
                text_offset_y: None,
                text_offset_y_first_page: None,
                text_offset_y_continuation: None,
                min_height: None,
                min_height_first_page: None,
                min_height_continuation: None,
                reserve_after: None,
            }],
            style_expression_steps: 64,
            auto_sample_rows: 2,
            max_rows: 4,
            max_row_fields: 4,
            max_cell_bytes: 64,
        },
        header_height: Length::Absolute(Unit::points(5).unwrap()),
        row_height: Some(Length::Auto),
        page_bodies: None,
        rows,
    };
    let bounds = Rect::new(
        Unit::points(2).unwrap(),
        Unit::points(3).unwrap(),
        Unit::points(100).unwrap(),
        Unit::points(35).unwrap(),
    )
    .unwrap();
    let style = ComputedStyle {
        fill: None,
        stroke: None,
        stroke_width: Unit::ZERO,
        stroke_sides: crate::StrokeSides::default(),
        opacity: 1_000_000,
        font: Some("mock".to_owned()),
        font_size: Unit::points(10).unwrap(),
        line_height: None,
        color: Color::parse("black").unwrap(),
        underline: false,
    };
    (table, bounds, style)
}
