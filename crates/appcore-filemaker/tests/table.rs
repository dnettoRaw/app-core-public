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
            },
            TableColumn {
                field: "amount".to_owned(),
                header: "Amount".to_owned(),
                width: ColumnWidth::Auto,
            },
        ],
        repeat_header: true,
        group_by: Some("group".to_owned()),
        total_fields: vec!["amount".to_owned()],
        conditional_styles: vec![TableStyleRule {
            when: "data.amount == 2".to_owned(),
            style: Style {
                fill: Some(Color::parse("red").unwrap()),
                ..Style::default()
            },
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
    let columns = resolve_table_columns(
        &table_spec,
        &dataset,
        Unit::points(100).unwrap(),
        Unit::points(1).unwrap(),
        &mut |value| Unit::points(i64::try_from(value.chars().count()).unwrap()),
    )
    .unwrap();
    assert_eq!(visited.load(Ordering::Relaxed), 2);
    assert_eq!(columns[1].width, Unit::points(6).unwrap());
    assert_eq!(columns[0].width, Unit::points(94).unwrap());
}

#[test]
fn weighted_flex_assigns_rounding_remainder_to_last_column() {
    let table_spec = TableSpec {
        columns: vec![
            TableColumn {
                field: "a".to_owned(),
                header: "A".to_owned(),
                width: ColumnWidth::Flex(1),
            },
            TableColumn {
                field: "b".to_owned(),
                header: "B".to_owned(),
                width: ColumnWidth::Flex(2),
            },
        ],
        repeat_header: false,
        group_by: None,
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
