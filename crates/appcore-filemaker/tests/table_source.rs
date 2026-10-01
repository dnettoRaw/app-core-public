// =============================================================================
//        #######
//     ###       ###     F: table_source.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use appcore_filemaker::*;

    const TABLE: &str = r#"
filemaker: "1.0"
model: document
id: report
page: { width: 200pt, height: 200pt }
elements:
  - id: results
    type: table
    binding: data.rows
    width: 180pt
    height: 160pt
    style: { font: Body, font_size: 8pt }
    table:
      columns:
        - { field: group, header: Group, width: { mode: fixed, value: 20pt } }
        - { field: name, header: Name, width: { mode: flex, value: 1 } }
        - { field: amount, header: Amount, width: { mode: auto }, align_x: end }
      repeat_header: true
      group_by: group
      keep_together_by: group
      total_fields: [amount]
      conditional_styles:
        - { when: "data.amount == 2", style: { fill: red, underline: true, stroke: black, stroke_width: 0.5pt, stroke_sides: { top: false, right: true, bottom: false, left: true } } }
      auto_sample_rows: 8
      max_rows: 4
      max_row_fields: 4
      max_cell_bytes: 64
      header_height: 10pt
      row_height: auto
"#;

    #[test]
    fn compiles_table_intent_and_binds_typed_rows() {
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(TABLE.as_bytes()).unwrap();
        let table = template.elements[0].table.as_ref().unwrap();
        assert!(table.rows.is_empty());
        assert_eq!(table.spec.columns.len(), 3);
        assert_eq!(table.spec.columns[2].align_x, Alignment::End);
        assert_eq!(table.spec.max_rows, 4);
        assert_eq!(table.spec.keep_together_by.as_deref(), Some("group"));

        let mut first = BTreeMap::new();
        first.insert("group".to_owned(), DataValue::String("A".to_owned()));
        first.insert("name".to_owned(), DataValue::String("Alpha".to_owned()));
        first.insert("amount".to_owned(), DataValue::Integer(2));
        let mut root = BTreeMap::new();
        root.insert(
            "rows".to_owned(),
            DataValue::Array(vec![DataValue::Object(first)]),
        );
        let document = compiler
            .bind(&template, &DataValue::Object(root), &[])
            .unwrap();
        let element = &document.elements[0];
        assert_eq!(element.table.as_ref().unwrap().rows.len(), 1);
        assert!(element.text.is_none());
        let error = LayoutEngine::new(
            &ResourceLimits::default(),
            &FontManager::default(),
            LayoutOptions::default(),
        )
        .unwrap()
        .resolve(&document)
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::FontMissing);
    }

    #[test]
    fn conditional_row_padding_is_applied_to_cells_and_layout() {
        let yaml = TABLE.replace(
            "style: { fill: red, underline: true, stroke: black, stroke_width: 0.5pt, stroke_sides:",
            "padding: { left: 5pt }, style: { fill: red, underline: true, stroke: black, stroke_width: 0.5pt, stroke_sides:",
        );
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let rows = [1, 2]
            .into_iter()
            .map(|amount| {
                DataValue::Object(BTreeMap::from([
                    ("group".to_owned(), DataValue::String("A".to_owned())),
                    ("name".to_owned(), DataValue::String("Row".to_owned())),
                    ("amount".to_owned(), DataValue::Integer(amount)),
                ]))
            })
            .collect();
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "rows".to_owned(),
                    DataValue::Array(rows),
                )])),
                &[],
            )
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(FontAsset::new("Body", deterministic_test_font(), 0).unwrap())
            .unwrap();
        let scene = LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        let table = scene.pages[0].elements[0].table.as_ref().unwrap();

        assert_eq!(table.rows[0].cells[1].padding.left, Unit::ZERO);
        assert_eq!(
            table.rows[1].cells[1].padding.left,
            Unit::points(5).unwrap()
        );
        assert_eq!(
            table.rows[1].cells[1].text_layout.measured.width,
            table.rows[1].cells[1]
                .bounds
                .size
                .width
                .checked_sub(Unit::points(5).unwrap())
                .unwrap()
        );
    }

    #[test]
    fn conditional_text_offset_changes_paint_without_changing_row_geometry() {
        let yaml = TABLE.replace(
            "- { when: \"data.amount == 2\", style:",
            "- { when: \"data.amount == 2\", text_offset_y: -0.4pt, style:",
        );
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let rows = [1, 2]
            .into_iter()
            .map(|amount| {
                DataValue::Object(BTreeMap::from([
                    ("group".to_owned(), DataValue::String("A".to_owned())),
                    ("name".to_owned(), DataValue::String("Row".to_owned())),
                    ("amount".to_owned(), DataValue::Integer(amount)),
                ]))
            })
            .collect();
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "rows".to_owned(),
                    DataValue::Array(rows),
                )])),
                &[],
            )
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(FontAsset::new("Body", deterministic_test_font(), 0).unwrap())
            .unwrap();
        let scene = LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        let table = scene.pages[0].elements[0].table.as_ref().unwrap();

        assert_eq!(
            table.rows[0].bounds.size.height,
            table.rows[1].bounds.size.height
        );
        assert_eq!(
            table.rows[0].cells[0].text_layout.measured,
            table.rows[1].cells[0].text_layout.measured
        );
        assert_eq!(
            table.rows[0].cells[0].text_layout.paint_offset_y,
            Unit::ZERO
        );
        assert_eq!(
            table.rows[1].cells[0].text_layout.paint_offset_y,
            Unit::from_raw(-400_000)
        );
    }

    #[test]
    fn rejects_percentage_text_offsets() {
        let yaml = TABLE.replace(
            "- { when: \"data.amount == 2\", style:",
            "- { when: \"data.amount == 2\", text_offset_y: 5%, style:",
        );
        let compiler = Compiler::builder().build().unwrap();
        assert!(compiler.compile_template_yaml(yaml.as_bytes()).is_err());
    }

    #[test]
    fn conditional_row_padding_can_differ_between_first_and_continuation_pages() {
        let yaml = TABLE
            .replace("height: 160pt", "height: 30pt")
            .replace(
                "- { when: \"data.amount == 2\", style:",
                "- { when: \"data.amount == 2\", padding_first_page: { top: 3pt }, padding_continuation: { top: 8pt }, text_offset_y_first_page: -1.35pt, text_offset_y_continuation: 0.25pt, style:",
            );
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let rows = [2, 2, 2]
            .into_iter()
            .map(|amount| {
                DataValue::Object(BTreeMap::from([
                    ("group".to_owned(), DataValue::String("A".to_owned())),
                    ("name".to_owned(), DataValue::String("Row".to_owned())),
                    ("amount".to_owned(), DataValue::Integer(amount)),
                ]))
            })
            .collect();
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "rows".to_owned(),
                    DataValue::Array(rows),
                )])),
                &[],
            )
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(FontAsset::new("Body", deterministic_test_font(), 0).unwrap())
            .unwrap();
        let scene = LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();

        assert_eq!(scene.pages.len(), 4);
        let row_padding: Vec<_> = scene
            .pages
            .iter()
            .flat_map(|page| &page.elements)
            .filter_map(|element| element.table.as_ref())
            .flat_map(|fragment| &fragment.rows)
            .map(|row| row.cells[1].padding.top)
            .collect();
        assert_eq!(
            row_padding,
            [
                Unit::points(3).unwrap(),
                Unit::points(8).unwrap(),
                Unit::points(8).unwrap()
            ]
        );
        let row_offsets: Vec<_> = scene
            .pages
            .iter()
            .flat_map(|page| &page.elements)
            .filter_map(|element| element.table.as_ref())
            .flat_map(|fragment| &fragment.rows)
            .map(|row| row.cells[1].text_layout.paint_offset_y)
            .collect();
        assert_eq!(
            row_offsets,
            [
                Unit::from_raw(-1_350_000),
                Unit::from_raw(250_000),
                Unit::from_raw(250_000)
            ]
        );
    }

    #[test]
    fn conditional_row_line_height_changes_wrapped_cell_line_metrics() {
        let yaml = TABLE.replace(
            "style: { fill: red, underline: true, stroke: black, stroke_width: 0.5pt, stroke_sides:",
            "style: { fill: red, underline: true, line_height: 2000000, stroke: black, stroke_width: 0.5pt, stroke_sides:",
        );
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let rows = [1, 2]
            .into_iter()
            .map(|amount| {
                DataValue::Object(BTreeMap::from([
                    ("group".to_owned(), DataValue::String("A".to_owned())),
                    (
                        "name".to_owned(),
                        DataValue::String("Line one\nLine two".to_owned()),
                    ),
                    ("amount".to_owned(), DataValue::Integer(amount)),
                ]))
            })
            .collect();
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "rows".to_owned(),
                    DataValue::Array(rows),
                )])),
                &[],
            )
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(FontAsset::new("Body", deterministic_test_font(), 0).unwrap())
            .unwrap();
        let scene = LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        let table = scene.pages[0].elements[0].table.as_ref().unwrap();
        let default_line = table.rows[0].cells[1].text_layout.lines[0].height;
        let styled_line = table.rows[1].cells[1].text_layout.lines[0].height;
        assert_eq!(table.rows[0].cells[1].text_layout.lines.len(), 2);
        assert_eq!(table.rows[1].cells[1].text_layout.lines.len(), 2);
        assert!(styled_line > default_line);
    }

    #[test]
    fn rejects_missing_misplaced_and_non_tabular_table_data() {
        let compiler = Compiler::builder().build().unwrap();
        let unknown_column = TABLE.replace("align_x: end", "align_x: end, unsupported: true");
        assert_eq!(
            compiler
                .compile_template_yaml(unknown_column.as_bytes())
                .unwrap_err()
                .code(),
            ErrorCode::SchemaSyntax
        );
        let automatic_padding =
            TABLE.replace("align_x: end", "align_x: end, padding: { top: auto }");
        assert_eq!(
            compiler
                .compile_template_yaml(automatic_padding.as_bytes())
                .unwrap_err()
                .code(),
            ErrorCode::DataType
        );
        let missing = TABLE.replace("    table:\n", "    absent_table:\n");
        assert_eq!(
            compiler
                .compile_template_yaml(missing.as_bytes())
                .unwrap_err()
                .code(),
            ErrorCode::SchemaSyntax
        );
        let misplaced = TABLE.replace("type: table", "type: rect");
        assert_eq!(
            compiler
                .compile_template_yaml(misplaced.as_bytes())
                .unwrap_err()
                .code(),
            ErrorCode::SchemaField
        );
        let template = compiler.compile_template_yaml(TABLE.as_bytes()).unwrap();
        let mut root = BTreeMap::new();
        root.insert("rows".to_owned(), DataValue::String("invalid".to_owned()));
        assert_eq!(
            compiler
                .bind(&template, &DataValue::Object(root), &[])
                .unwrap_err()
                .code(),
            ErrorCode::DataType
        );
    }

    #[test]
    fn table_level_text_alignment_is_rejected_instead_of_ignored() {
        let yaml = TABLE.replace(
            "    table:\n",
            "    text_options: { align_x: end }\n    table:\n",
        );
        assert_eq!(
            Compiler::builder()
                .build()
                .unwrap()
                .compile_template_yaml(yaml.as_bytes())
                .unwrap_err()
                .code(),
            ErrorCode::SchemaField
        );
    }

    #[test]
    fn table_source_cannot_raise_global_limits() {
        let limits = ResourceLimits {
            max_rows: 3,
            ..ResourceLimits::default()
        };
        let compiler = Compiler::builder().limits(limits).build().unwrap();
        assert_eq!(
            compiler
                .compile_template_yaml(TABLE.as_bytes())
                .unwrap_err()
                .code(),
            ErrorCode::LimitExceeded
        );
    }

    #[test]
    fn table_fragments_preserve_pagination_styles_and_anchors() {
        let (scene, _, _) = resolved_table_fixture();
        let fragments = result_fragments(&scene);

        assert!(scene.pages.len() > 1);
        assert_eq!(fragments[0].index, 0);
        assert_eq!(fragments[0].rows[0].source_index, 0);
        assert_eq!(fragments.last().unwrap().totals[2].text, "10");
        let conditional_amount = fragments
            .iter()
            .flat_map(|fragment| &fragment.rows)
            .flat_map(|row| &row.cells)
            .find(|cell| cell.field == "amount" && cell.text == "2")
            .unwrap();
        assert_eq!(
            conditional_amount.style.fill,
            Some(Color::parse("red").unwrap())
        );
        assert!(conditional_amount.style.underline);
        assert_eq!(
            conditional_amount.style.stroke_sides,
            StrokeSides {
                top: false,
                right: true,
                bottom: false,
                left: true
            }
        );
        assert!(fragments
            .iter()
            .flat_map(|fragment| &fragment.header)
            .all(|cell| !cell.text_layout.lines.is_empty()));
        assert!(fragments
            .iter()
            .flat_map(|fragment| &fragment.rows)
            .flat_map(|row| &row.cells)
            .filter(|cell| cell.field == "amount")
            .all(|cell| cell.text_layout.align_x == Alignment::End));

        let last_page = scene
            .pages
            .iter()
            .position(|page| {
                page.elements.iter().any(|element| {
                    element.id.as_str() == "results"
                        && element
                            .table
                            .as_ref()
                            .is_some_and(|table| table.index == fragments.len() - 1)
                })
            })
            .unwrap();
        let anchored = scene.pages[last_page]
            .elements
            .iter()
            .find(|element| element.id.as_str() == "after-results")
            .unwrap();
        assert_eq!(anchored.bounds.layout.origin.y, Unit::points(25).unwrap());
    }

    #[test]
    fn table_cell_padding_and_inspection_match_resolved_geometry() {
        let (scene, _, _) = resolved_table_fixture();
        let fragments = result_fragments(&scene);
        let amount_cells = fragments
            .iter()
            .flat_map(|fragment| {
                fragment
                    .header
                    .iter()
                    .chain(fragment.rows.iter().flat_map(|row| &row.cells))
                    .chain(&fragment.totals)
            })
            .filter(|cell| cell.field == "amount")
            .collect::<Vec<_>>();
        assert!(amount_cells.len() >= 4);
        assert!(amount_cells.iter().all(|cell| {
            let content = cell.content_bounds().unwrap();
            cell.padding
                == Insets {
                    top: Unit::points(1).unwrap(),
                    right: Unit::points(2).unwrap(),
                    bottom: Unit::points(1).unwrap(),
                    left: Unit::points(3).unwrap(),
                }
                && content.origin.x
                    == cell
                        .bounds
                        .origin
                        .x
                        .checked_add(Unit::points(3).unwrap())
                        .unwrap()
                && content.origin.y
                    == cell
                        .bounds
                        .origin
                        .y
                        .checked_add(Unit::points(1).unwrap())
                        .unwrap()
                && content.size.width
                    == cell
                        .bounds
                        .size
                        .width
                        .checked_sub(Unit::points(5).unwrap())
                        .unwrap()
                && content.size.height
                    == cell
                        .bounds
                        .size
                        .height
                        .checked_sub(Unit::points(2).unwrap())
                        .unwrap()
        }));
        assert!(fragments
            .iter()
            .flat_map(|fragment| &fragment.rows)
            .flat_map(|row| &row.cells)
            .all(|cell| cell.text_layout.padding_inline == Unit::points(2).unwrap()));
        let inspection = SceneInspector::new(&scene)
            .inspect_element(&ElementId::new("results").unwrap())
            .unwrap();
        assert_eq!(inspection.table_fragment, Some(0));
        assert_eq!(inspection.table_rows, Some(1));
    }

    #[test]
    fn table_fragments_export_with_explicit_font_without_losses() {
        let (scene, fonts, limits) = resolved_table_fixture();
        let context = ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        };
        for format in [
            ExportFormat::Pdf,
            ExportFormat::Svg,
            ExportFormat::Png,
            ExportFormat::Html,
        ] {
            let mut bytes = Vec::new();
            let outcome = export(
                &scene,
                &ExportRequest {
                    format,
                    ..ExportRequest::default()
                },
                &context,
                &mut bytes,
            )
            .unwrap();
            assert!(outcome.loss_report.losses.is_empty());
            assert!(!bytes.is_empty());
            if format == ExportFormat::Html {
                assert!(String::from_utf8_lossy(&bytes).contains("text-align:end"));
                assert!(String::from_utf8_lossy(&bytes)
                    .contains("padding:1.000000pt 4.000000pt 1.000000pt 5.000000pt"));
            }
            if format == ExportFormat::Pdf {
                let source = String::from_utf8_lossy(&bytes);
                assert!(source.contains("/BaseFont /FMSubset1"));
                assert!(source.contains("/FontFile2"));
                assert!(source.contains("/ToUnicode"));
            }
        }
    }

    fn resolved_table_fixture() -> (ResolvedScene, FontManager, ResourceLimits) {
        let bytes = deterministic_test_font();
        let yaml = TABLE
            .replace("height: 160pt", "height: 25pt")
            .replace(
                "align_x: end }",
                "align_x: end, padding: { top: 1pt, right: 2pt, bottom: 1pt, left: 3pt } }",
            )
            .replace(
                "      row_height: auto",
                "      row_height: auto\n  - id: after-results\n    type: text\n    text: Total\n    width: 50pt\n    height: 10pt\n    anchors: { top: 'results.bottom' }\n    style: { font: Body, font_size: 8pt }",
            )
            .replace(
                "    style: { font: Body, font_size: 8pt }\n    table:",
                "    style: { font: Body, font_size: 8pt }\n    text_options: { padding_inline: 2pt }\n    table:",
            );
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let rows = [
            ("A", "One", 1),
            ("A", "Two", 2),
            ("B", "Three", 3),
            ("B", "Four", 4),
        ]
        .into_iter()
        .map(|(group, name, amount)| {
            DataValue::Object(BTreeMap::from([
                ("group".to_owned(), DataValue::String(group.to_owned())),
                ("name".to_owned(), DataValue::String(name.to_owned())),
                ("amount".to_owned(), DataValue::Integer(amount)),
            ]))
        })
        .collect();
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "rows".to_owned(),
                    DataValue::Array(rows),
                )])),
                &[],
            )
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(FontAsset::new("Body", bytes, 0).unwrap())
            .unwrap();
        let limits = ResourceLimits::default();
        let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();

        (scene, fonts, limits)
    }

    fn result_fragments(scene: &ResolvedScene) -> Vec<&ResolvedTableFragment> {
        scene
            .pages
            .iter()
            .flat_map(|page| &page.elements)
            .filter_map(|element| element.table.as_ref())
            .collect()
    }

    #[test]
    fn vertical_writing_is_resolved_and_exported_inside_table_cells() {
        let yaml = r#"
filemaker: "1.0"
model: document
id: vertical-table
page: { width: 80pt, height: 80pt }
elements:
  - id: values
    type: table
    binding: data.rows
    x: 10pt
    y: 10pt
    width: 40pt
    height: 50pt
    style: { font: Japanese, font_size: 10pt }
    text_options: { writing_mode: vertical }
    table:
      columns:
        - { field: value, header: 日本, width: { mode: fixed, value: 40pt } }
      header_height: 20pt
      row_height: 20pt
      max_rows: 1
      max_row_fields: 1
      max_cell_bytes: 16
"#;
        let compiler = Compiler::builder().build().unwrap();
        let invalid = yaml.replace(
            "{ writing_mode: vertical }",
            "{ writing_mode: vertical, overflow: clip }",
        );
        assert_eq!(
            compiler
                .compile_template_yaml(invalid.as_bytes())
                .unwrap_err()
                .code(),
            ErrorCode::SchemaField
        );
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let data = DataValue::Object(BTreeMap::from([(
            "rows".to_owned(),
            DataValue::Array(vec![DataValue::Object(BTreeMap::from([(
                "value".to_owned(),
                DataValue::String("日本".to_owned()),
            )]))]),
        )]));
        let document = compiler.bind(&template, &data, &[]).unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(
                FontAsset::new(
                    "Japanese",
                    include_bytes!("assets/NotoSansJP-Test.ttf").to_vec(),
                    0,
                )
                .unwrap(),
            )
            .unwrap();
        let limits = ResourceLimits::default();
        let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        let table = scene.pages[0].elements[0].table.as_ref().unwrap();
        for cell in table.header.iter().chain(&table.rows[0].cells) {
            assert_eq!(cell.text_layout.writing_mode, WritingMode::Vertical);
            assert!(cell
                .text_layout
                .lines
                .iter()
                .flat_map(|column| &column.runs)
                .flat_map(|run| &run.glyphs)
                .all(|glyph| glyph.advance_y != Unit::ZERO));
        }
        let context = ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        };
        for format in [ExportFormat::Pdf, ExportFormat::Svg] {
            let (bytes, outcome) = export_bytes(
                &scene,
                &ExportRequest {
                    format,
                    fidelity: Fidelity::Strict,
                    ..ExportRequest::default()
                },
                &context,
            )
            .unwrap();
            assert!(outcome.loss_report.losses.is_empty());
            if format == ExportFormat::Svg {
                assert!(String::from_utf8(bytes)
                    .unwrap()
                    .contains("writing-mode=\"vertical-rl\""));
            }
        }
    }

    #[test]
    fn a_deferred_first_group_does_not_leave_a_blank_physical_page() {
        let yaml = TABLE
            .replace("height: 160pt", "height: 40pt")
            .replace("repeat_header: true", "repeat_header: false")
            .replace("row_height: auto", "row_height: 16pt");
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        assert!(
            !template.elements[0]
                .table
                .as_ref()
                .unwrap()
                .spec
                .repeat_header
        );
        let rows = ["One", "Two"]
            .into_iter()
            .map(|name| {
                DataValue::Object(BTreeMap::from([
                    ("group".to_owned(), DataValue::String("A".to_owned())),
                    ("name".to_owned(), DataValue::String(name.to_owned())),
                    ("amount".to_owned(), DataValue::Integer(1)),
                ]))
            })
            .collect();
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "rows".to_owned(),
                    DataValue::Array(rows),
                )])),
                &[],
            )
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(FontAsset::new("Body", deterministic_test_font(), 0).unwrap())
            .unwrap();
        let scene = LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();

        assert_eq!(scene.pages.len(), 2);
        assert_eq!(scene.pages[0].index, 0);
        let table = scene.pages[0].elements[0].table.as_ref().unwrap();
        assert_eq!(table.index, 1);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(scene.pages[1].index, 1);
        assert!(scene.pages[1].elements[0]
            .table
            .as_ref()
            .unwrap()
            .rows
            .is_empty());
    }

    #[test]
    fn named_table_row_anchor_positions_following_elements_on_that_row_page() {
        let yaml = TABLE
            .replace("keep_together_by: group", "keep_together_by: group\n      row_anchor_field: anchor")
            .replace(
                "      row_height: auto",
                "      row_height: auto\n  - id: after-row\n    type: text\n    text: Total\n    x: 50pt\n    width: 50pt\n    height: 10pt\n    collision: false\n    anchors: { top: 'results::totals.bottom+2pt' }\n    style: { font: Body, font_size: 8pt }",
            );
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let row = DataValue::Object(BTreeMap::from([
            ("group".to_owned(), DataValue::String("A".to_owned())),
            ("name".to_owned(), DataValue::String("Subtotal".to_owned())),
            ("amount".to_owned(), DataValue::Integer(2)),
            ("anchor".to_owned(), DataValue::String("totals".to_owned())),
        ]));
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "rows".to_owned(),
                    DataValue::Array(vec![row]),
                )])),
                &[],
            )
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(FontAsset::new("Body", deterministic_test_font(), 0).unwrap())
            .unwrap();
        let scene = LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        let page = &scene.pages[0];
        let row_bottom = page.elements[0].table.as_ref().unwrap().rows[0]
            .bounds
            .bottom()
            .unwrap();
        let anchored = page
            .elements
            .iter()
            .find(|element| element.id.as_str() == "after-row")
            .unwrap();
        assert_eq!(
            anchored.bounds.layout.origin.y,
            row_bottom.checked_add(Unit::points(2).unwrap()).unwrap()
        );
    }

    #[test]
    fn named_row_anchor_uses_the_physical_page_of_a_paginated_row() {
        let yaml = TABLE
            .replace("height: 160pt", "height: 50pt")
            .replace("keep_together_by: group", "keep_together_by: group\n      row_anchor_field: anchor")
            .replace("total_fields: [amount]", "total_fields: []")
            .replace(
                "      auto_sample_rows: 8",
                "        - { when: 'data.anchor == \"middle\"', reserve_after: 20pt }\n      auto_sample_rows: 8",
            )
            .replace(
                "      row_height: auto",
                "      row_height: 12pt\n  - id: after-row\n    type: text\n    text: Total\n    x: 50pt\n    width: 50pt\n    height: 10pt\n    collision: false\n    anchors: { top: 'results::middle.bottom+2pt' }\n    style: { font: Body, font_size: 8pt }",
            );
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let rows = ["first", "middle", "last"]
            .into_iter()
            .map(|anchor| {
                DataValue::Object(BTreeMap::from([
                    ("group".to_owned(), DataValue::String(anchor.to_owned())),
                    ("name".to_owned(), DataValue::String(anchor.to_owned())),
                    ("amount".to_owned(), DataValue::Integer(1)),
                    ("anchor".to_owned(), DataValue::String(anchor.to_owned())),
                ]))
            })
            .collect();
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "rows".to_owned(),
                    DataValue::Array(rows),
                )])),
                &[],
            )
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(FontAsset::new("Body", deterministic_test_font(), 0).unwrap())
            .unwrap();
        let scene = LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();
        let anchored_row_page = scene
            .pages
            .iter()
            .position(|page| {
                page.elements.iter().any(|element| {
                    element.table.as_ref().is_some_and(|fragment| {
                        fragment.rows.iter().any(|row| row.source_index == 1)
                    })
                })
            })
            .unwrap();
        let anchor_page = scene
            .pages
            .iter()
            .position(|page| {
                page.elements
                    .iter()
                    .any(|element| element.id.as_str() == "after-row")
            })
            .unwrap();
        assert_eq!(scene.pages.len(), 3);
        assert_eq!(anchor_page, anchored_row_page);
        let anchored_row = scene.pages[anchored_row_page]
            .elements
            .iter()
            .find_map(|element| element.table.as_ref())
            .unwrap()
            .rows
            .iter()
            .find(|row| row.source_index == 1)
            .unwrap();
        assert_eq!(anchored_row.bounds.size.height, Unit::points(12).unwrap());
    }

    #[test]
    fn row_anchor_reservation_requires_a_positive_absolute_length_and_anchor_field() {
        let compiler = Compiler::builder().build().unwrap();
        for yaml in [
            TABLE.replace(
                "      conditional_styles:\n        - { when: \"data.amount == 2\", style:",
                "      conditional_styles:\n        - { when: \"data.amount == 2\", reserve_after: 12pt, style:",
            ),
            TABLE.replace(
                "      conditional_styles:\n        - { when: \"data.amount == 2\", style:",
                "      row_anchor_field: anchor\n      conditional_styles:\n        - { when: \"data.amount == 2\", reserve_after: 0pt, style:",
            ),
            TABLE.replace(
                "      conditional_styles:\n        - { when: \"data.amount == 2\", style:",
                "      row_anchor_field: anchor\n      conditional_styles:\n        - { when: \"data.amount == 2\", reserve_after: 10%, style:",
            ),
        ] {
            assert!(compiler.compile_template_yaml(yaml.as_bytes()).is_err());
        }
    }

    #[test]
    fn oversized_contiguous_group_splits_between_rows_without_losing_or_repeating_data() {
        // Three 70pt rows exceed both the 160pt table body and 200pt page.
        let yaml = TABLE.replace("      row_height: auto", "      row_height: 70pt");
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let rows = [
            ("Oversized document section", DataValue::Integer(1)),
            ("Description segment one", DataValue::Null),
            ("Description segment two", DataValue::Null),
        ]
        .into_iter()
        .map(|(name, amount)| {
            DataValue::Object(BTreeMap::from([
                (
                    "group".to_owned(),
                    DataValue::String("same-block".to_owned()),
                ),
                ("name".to_owned(), DataValue::String(name.to_owned())),
                ("amount".to_owned(), amount),
            ]))
        })
        .collect();
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "rows".to_owned(),
                    DataValue::Array(rows),
                )])),
                &[],
            )
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(FontAsset::new("Body", deterministic_test_font(), 0).unwrap())
            .unwrap();
        let scene = LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();

        let rows: Vec<_> = scene
            .pages
            .iter()
            .flat_map(|page| &page.elements)
            .filter_map(|element| element.table.as_ref())
            .flat_map(|fragment| &fragment.rows)
            .collect();
        assert_eq!(scene.pages.len(), 2);
        assert_eq!(
            rows.iter().map(|row| row.source_index).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.cells[1].text.as_str())
                .collect::<Vec<_>>(),
            [
                "Oversized document section",
                "Description segment one",
                "Description segment two"
            ]
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.cells[2].text.as_str())
                .collect::<Vec<_>>(),
            ["1", "", ""]
        );
        let row_counts: Vec<_> = scene
            .pages
            .iter()
            .map(|page| {
                page.elements
                    .iter()
                    .filter_map(|element| element.table.as_ref())
                    .flat_map(|fragment| &fragment.rows)
                    .count()
            })
            .collect();
        assert_eq!(row_counts, [2, 1]);
        assert!(scene.pages[1].elements.iter().any(|element| {
            element
                .table
                .as_ref()
                .is_some_and(|fragment| fragment.totals.iter().any(|cell| cell.text == "1"))
        }));
    }

    #[test]
    fn table_page_bodies_use_separate_first_and_continuation_rectangles() {
        let yaml = TABLE
            .replace("height: 160pt", "height: 120pt")
            .replace("repeat_header: true", "repeat_header: false")
            .replace("total_fields: [amount]", "total_fields: []")
            .replace(
                "- { when: \"data.amount == 2\", style: { fill: red, underline: true, stroke: black, stroke_width: 0.5pt, stroke_sides: { top: false, right: true, bottom: false, left: true } } }",
                "- { when: 'data.name == \"Second\"', style: { fill: red }, min_height: 45pt }",
            )
            .replace(
                "      page_bodies:",
                "        - { when: 'data.name == \"Third\"', min_height_first_page: 35pt, min_height_continuation: 18pt }\n      page_bodies:",
            )
            .replace(
                "      header_height: 10pt",
                "      page_bodies:\n        first: { offset_y: 20pt, height: 40pt }\n        continuation: { offset_y: 70pt, height: 50pt }\n      header_height: 10pt",
            )
            .replace("row_height: auto", "row_height: 24pt");
        let compiler = Compiler::builder().build().unwrap();
        let template = compiler.compile_template_yaml(yaml.as_bytes()).unwrap();
        let rows = ["First", "Second", "Third"]
            .into_iter()
            .map(|name| {
                DataValue::Object(BTreeMap::from([
                    ("group".to_owned(), DataValue::String("bundle-A".to_owned())),
                    ("name".to_owned(), DataValue::String(name.to_owned())),
                    ("amount".to_owned(), DataValue::Integer(1)),
                ]))
            })
            .collect();
        let document = compiler
            .bind(
                &template,
                &DataValue::Object(BTreeMap::from([(
                    "rows".to_owned(),
                    DataValue::Array(rows),
                )])),
                &[],
            )
            .unwrap();
        let mut fonts = FontManager::default();
        fonts
            .register(FontAsset::new("Body", deterministic_test_font(), 0).unwrap())
            .unwrap();
        let scene = LayoutEngine::new(&ResourceLimits::default(), &fonts, LayoutOptions::default())
            .unwrap()
            .resolve(&document)
            .unwrap();

        assert_eq!(scene.pages.len(), 3);
        let table_rows = scene
            .pages
            .iter()
            .map(|page| page.elements[0].table.as_ref().unwrap().rows.as_slice())
            .collect::<Vec<_>>();
        assert_eq!(table_rows[0][0].cells[1].text, "First");
        assert_eq!(table_rows[1][0].cells[1].text, "Second");
        assert_eq!(table_rows[2][0].cells[1].text, "Third");
        let first = table_rows[0][0].bounds;
        let continuation = table_rows[1][0].bounds;
        assert_eq!(first.origin.y, Unit::points(30).unwrap());
        assert_eq!(continuation.origin.y, Unit::points(70).unwrap());
        assert_eq!(continuation.size.height, Unit::points(45).unwrap());
        assert_eq!(
            table_rows[2][0].bounds.size.height,
            Unit::points(24).unwrap()
        );
    }

    fn deterministic_test_font() -> Vec<u8> {
        include_bytes!("../examples/assets/NotoSans-Regular.ttf").to_vec()
    }
}
