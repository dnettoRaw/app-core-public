// =============================================================================
//        #######
//     ###       ###     F: table.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use appcore_filemaker::{
    resolve_table_columns, Color, ColumnWidth, DataValue, ErrorCode, InMemoryDataset, Result,
    StreamingDataset, Style, TableColumn, TablePage, TablePageSink, TablePaginator, TableSpec,
    TableStyleRule, Unit,
};

#[derive(Default)]
struct Pages(Vec<TablePage>);

impl TablePageSink for Pages {
    fn page(&mut self, page: TablePage) -> Result<()> {
        self.0.push(page);
        Ok(())
    }
}

fn spec(max_rows: u64) -> TableSpec {
    TableSpec {
        columns: vec![
            TableColumn {
                field: "group".to_owned(),
                header: "Group".to_owned(),
                width: ColumnWidth::Flex(1),
                align_x: appcore_filemaker::Alignment::Start,
                padding: appcore_filemaker::CellPadding::default(),
            },
            TableColumn {
                field: "amount".to_owned(),
                header: "Amount".to_owned(),
                width: ColumnWidth::Auto,
                align_x: appcore_filemaker::Alignment::Start,
                padding: appcore_filemaker::CellPadding::default(),
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
        auto_sample_rows: 16,
        max_rows,
        max_row_fields: 16,
        max_cell_bytes: 1_024,
    }
}

fn rows() -> InMemoryDataset {
    InMemoryDataset {
        rows: [("A", 1), ("A", 2), ("B", 3), ("B", 4)]
            .into_iter()
            .map(|(group, amount)| {
                BTreeMap::from([
                    ("group".to_owned(), DataValue::String(group.to_owned())),
                    ("amount".to_owned(), DataValue::Integer(amount)),
                ])
            })
            .collect(),
    }
}

#[test]
fn streaming_dataset_stops_at_explicit_limit() {
    let dataset = StreamingDataset::new(
        || {
            (0..3).map(|index| {
                Ok(BTreeMap::from([
                    ("group".to_owned(), DataValue::String("A".to_owned())),
                    ("amount".to_owned(), DataValue::Integer(index)),
                ]))
            })
        },
        Some(3),
    );
    let error = spec(2)
        .visit_bounded(&dataset, &mut |_, _| Ok(()))
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LimitExceeded);
}

#[test]
fn row_anchor_names_must_be_unique_bounded_strings() {
    let mut table_spec = spec(2);
    table_spec.row_anchor_field = Some("anchor".to_owned());
    let dataset = InMemoryDataset {
        rows: [1, 2]
            .into_iter()
            .map(|amount| {
                BTreeMap::from([
                    ("group".to_owned(), DataValue::String("A".to_owned())),
                    ("amount".to_owned(), DataValue::Integer(amount)),
                    ("anchor".to_owned(), DataValue::String("totals".to_owned())),
                ])
            })
            .collect(),
    };

    let error = table_spec
        .visit_bounded(&dataset, &mut |_, _| Ok(()))
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::DataType);
}

#[test]
fn measured_rows_groups_styles_and_totals_stream_to_pages() {
    let paginator = TablePaginator {
        available_height: Unit::points(25).unwrap(),
        header_height: Unit::points(5).unwrap(),
        row_height: Unit::ZERO,
        max_pages: 3,
    };
    let heights = [8_i64, 12, 8, 12];
    let mut cursor = 0_usize;
    let mut pages = Pages::default();
    paginator
        .paginate_measured(
            &spec(4),
            &rows(),
            &mut |_| {
                let height = Unit::points(heights[cursor])?;
                cursor += 1;
                Ok(height)
            },
            &mut pages,
        )
        .unwrap();
    assert_eq!(pages.0.len(), 2);
    assert!(pages.0.iter().all(|page| page.header));
    assert_eq!(pages.0[0].starting_group.as_deref(), Some("A"));
    assert_eq!(pages.0[0].group_starts, [Some("A".to_owned()), None]);
    assert_eq!(pages.0[1].group_starts, [Some("B".to_owned()), None]);
    assert_eq!(
        pages.0[0].row_heights,
        [Unit::points(8).unwrap(), Unit::points(12).unwrap()]
    );
    assert_eq!(
        pages.0[0].row_styles[1].fill,
        Some(Color::parse("red").unwrap())
    );
    assert!(pages.0[0].totals.is_empty());
    assert_eq!(pages.0[1].totals["amount"], DataValue::Integer(10));
}

#[test]
fn keep_together_by_moves_a_fitting_contiguous_group_before_the_page_break() {
    let paginator = TablePaginator {
        available_height: Unit::points(25).unwrap(),
        header_height: Unit::points(5).unwrap(),
        row_height: Unit::ZERO,
        max_pages: 2,
    };
    let dataset = InMemoryDataset {
        rows: [("intro", 0), ("bundle-A", 1), ("bundle-A", 2)]
            .into_iter()
            .map(|(group, amount)| {
                BTreeMap::from([
                    ("group".to_owned(), DataValue::String(group.to_owned())),
                    ("amount".to_owned(), DataValue::Integer(amount)),
                ])
            })
            .collect(),
    };
    let heights = [12_i64, 8, 8];
    let mut cursor = 0;
    let mut table_spec = spec(3);
    table_spec.keep_together_by = Some("group".to_owned());
    let mut pages = Pages::default();

    paginator
        .paginate_measured(
            &table_spec,
            &dataset,
            &mut |_| {
                let height = Unit::points(heights[cursor])?;
                cursor += 1;
                Ok(height)
            },
            &mut pages,
        )
        .unwrap();

    assert_eq!(pages.0.len(), 2);
    assert_eq!(pages.0[0].rows.len(), 1);
    assert_eq!(pages.0[1].rows.len(), 2);
    assert!(pages.0[1]
        .rows
        .iter()
        .all(|row| row["group"] == DataValue::String("bundle-A".to_owned())));
}

#[test]
fn keep_together_by_splits_a_group_larger_than_a_page_without_losing_rows() {
    let paginator = TablePaginator {
        available_height: Unit::points(25).unwrap(),
        header_height: Unit::points(5).unwrap(),
        row_height: Unit::ZERO,
        max_pages: 2,
    };
    let dataset = InMemoryDataset {
        rows: (0..4)
            .map(|amount| {
                BTreeMap::from([
                    ("group".to_owned(), DataValue::String("bundle-A".to_owned())),
                    ("amount".to_owned(), DataValue::Integer(amount)),
                ])
            })
            .collect(),
    };
    let mut table_spec = spec(4);
    table_spec.keep_together_by = Some("group".to_owned());
    let mut pages = Pages::default();

    paginator
        .paginate_measured(&table_spec, &dataset, &mut |_| Unit::points(8), &mut pages)
        .unwrap();

    assert_eq!(pages.0.iter().map(|page| page.rows.len()).sum::<usize>(), 4);
    assert_eq!(pages.0.len(), 2);
    assert_eq!(pages.0[0].rows.len(), 2);
    assert_eq!(pages.0[1].rows.len(), 2);
}

#[test]
fn keep_together_by_rejects_rows_without_the_declared_group_key() {
    let paginator = TablePaginator {
        available_height: Unit::points(25).unwrap(),
        header_height: Unit::points(5).unwrap(),
        row_height: Unit::points(8).unwrap(),
        max_pages: 1,
    };
    let dataset = InMemoryDataset {
        rows: vec![BTreeMap::from([(
            "amount".to_owned(),
            DataValue::Integer(1),
        )])],
    };
    let mut table_spec = spec(1);
    table_spec.keep_together_by = Some("group".to_owned());

    let error = paginator
        .paginate(&table_spec, &dataset, &mut Pages::default())
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::DataType);
}

#[test]
fn auto_columns_stop_at_sample_limit_and_flex_consumes_remainder() {
    let visited = Arc::new(AtomicUsize::new(0));
    let dataset = StreamingDataset::new(
        {
            let visited = Arc::clone(&visited);
            move || {
                let visited = Arc::clone(&visited);
                (0..100).map(move |amount| {
                    visited.fetch_add(1, Ordering::Relaxed);
                    Ok(BTreeMap::from([
                        ("group".to_owned(), DataValue::String("A".to_owned())),
                        ("amount".to_owned(), DataValue::Integer(amount)),
                    ]))
                })
            }
        },
        Some(100),
    );
    let mut table_spec = spec(100);
    table_spec.auto_sample_rows = 2;
    table_spec.columns[1].padding = appcore_filemaker::CellPadding {
        left: appcore_filemaker::Length::Absolute(Unit::points(2).unwrap()),
        right: appcore_filemaker::Length::Absolute(Unit::points(3).unwrap()),
        ..appcore_filemaker::CellPadding::default()
    };
    let columns = resolve_table_columns(
        &table_spec,
        &dataset,
        Unit::points(100).unwrap(),
        Unit::points(1).unwrap(),
        &mut |value| Unit::points(i64::try_from(value.chars().count()).unwrap()),
    )
    .unwrap();
    assert_eq!(visited.load(Ordering::Relaxed), 2);
    assert_eq!(columns[1].width, Unit::points(11).unwrap());
    assert_eq!(columns[0].width, Unit::points(89).unwrap());
    assert_eq!(columns[1].padding.left, Unit::points(2).unwrap());
    assert_eq!(columns[1].padding.right, Unit::points(3).unwrap());
}

#[test]
fn weighted_flex_assigns_rounding_remainder_to_last_column() {
    let table_spec = TableSpec {
        columns: vec![
            TableColumn {
                field: "a".to_owned(),
                header: "A".to_owned(),
                width: ColumnWidth::Flex(1),
                align_x: appcore_filemaker::Alignment::Start,
                padding: appcore_filemaker::CellPadding::default(),
            },
            TableColumn {
                field: "b".to_owned(),
                header: "B".to_owned(),
                width: ColumnWidth::Flex(2),
                align_x: appcore_filemaker::Alignment::Start,
                padding: appcore_filemaker::CellPadding::default(),
            },
        ],
        repeat_header: false,
        group_by: None,
        keep_together_by: None,
        row_anchor_field: None,
        total_fields: Vec::new(),
        conditional_styles: Vec::new(),
        style_expression_steps: 64,
        auto_sample_rows: 1,
        max_rows: 1,
        max_row_fields: 16,
        max_cell_bytes: 1_024,
    };
    let columns = resolve_table_columns(
        &table_spec,
        &InMemoryDataset::default(),
        Unit::points(10).unwrap(),
        Unit::points(1).unwrap(),
        &mut |_| Ok(Unit::ZERO),
    )
    .unwrap();
    assert_eq!(columns[0].width, Unit::from_raw(3_333_333));
    assert_eq!(columns[1].width, Unit::from_raw(6_666_667));
}

#[test]
fn invalid_totals_and_oversized_or_compound_cells_fail_closed() {
    let paginator = TablePaginator {
        available_height: Unit::points(30).unwrap(),
        header_height: Unit::points(5).unwrap(),
        row_height: Unit::points(10).unwrap(),
        max_pages: 2,
    };
    let non_numeric = InMemoryDataset {
        rows: vec![BTreeMap::from([
            ("group".to_owned(), DataValue::String("A".to_owned())),
            (
                "amount".to_owned(),
                DataValue::String("not numeric".to_owned()),
            ),
        ])],
    };
    assert_eq!(
        paginator
            .paginate(&spec(1), &non_numeric, &mut Pages::default())
            .unwrap_err()
            .code(),
        ErrorCode::DataType
    );

    let mut bounded = spec(1);
    bounded.max_cell_bytes = 3;
    let oversized = InMemoryDataset {
        rows: vec![BTreeMap::from([
            ("group".to_owned(), DataValue::String("long".to_owned())),
            ("amount".to_owned(), DataValue::Integer(1)),
        ])],
    };
    assert_eq!(
        bounded
            .visit_bounded(&oversized, &mut |_, _| Ok(()))
            .unwrap_err()
            .code(),
        ErrorCode::LimitExceeded
    );
    let compound = InMemoryDataset {
        rows: vec![BTreeMap::from([
            (
                "group".to_owned(),
                DataValue::Array(vec![DataValue::String("A".to_owned())]),
            ),
            ("amount".to_owned(), DataValue::Integer(1)),
        ])],
    };
    assert_eq!(
        spec(1)
            .visit_bounded(&compound, &mut |_, _| Ok(()))
            .unwrap_err()
            .code(),
        ErrorCode::LimitExceeded
    );
}

#[test]
fn first_only_header_changes_continuation_capacity() {
    let mut table_spec = spec(5);
    table_spec.repeat_header = false;
    table_spec.group_by = None;
    table_spec.total_fields.clear();
    table_spec.conditional_styles.clear();
    let dataset = InMemoryDataset {
        rows: (0..5)
            .map(|amount| {
                BTreeMap::from([
                    ("group".to_owned(), DataValue::String("A".to_owned())),
                    ("amount".to_owned(), DataValue::Integer(amount)),
                ])
            })
            .collect(),
    };
    let paginator = TablePaginator {
        available_height: Unit::points(30).unwrap(),
        header_height: Unit::points(10).unwrap(),
        row_height: Unit::points(10).unwrap(),
        max_pages: 2,
    };
    let mut pages = Pages::default();
    paginator
        .paginate(&table_spec, &dataset, &mut pages)
        .unwrap();
    assert_eq!(
        pages
            .0
            .iter()
            .map(|page| page.rows.len())
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert!(pages.0[0].header);
    assert!(!pages.0[1].header);
}
