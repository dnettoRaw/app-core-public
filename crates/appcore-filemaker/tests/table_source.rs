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
        - { field: amount, header: Amount, width: { mode: auto } }
      repeat_header: true
      group_by: group
      total_fields: [amount]
      conditional_styles:
        - { when: "data.amount == 2", style: { fill: red } }
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
        assert_eq!(table.spec.max_rows, 4);

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
    fn rejects_missing_misplaced_and_non_tabular_table_data() {
        let compiler = Compiler::builder().build().unwrap();
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
    fn layout_engine_emits_physical_table_fragments_with_explicit_font() {
        let bytes = deterministic_test_font();
        let yaml = TABLE.replace("height: 160pt", "height: 25pt");
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

        assert!(scene.pages.len() > 1);
        let fragments: Vec<_> = scene
            .pages
            .iter()
            .flat_map(|page| &page.elements)
            .map(|element| element.table.as_ref().unwrap())
            .collect();
        assert_eq!(fragments[0].index, 0);
        assert_eq!(fragments[0].rows[0].source_index, 0);
        assert_eq!(fragments.last().unwrap().totals[2].text, "10");
        assert!(fragments
            .iter()
            .flat_map(|fragment| &fragment.header)
            .all(|cell| !cell.text_layout.lines.is_empty()));
        let inspection = SceneInspector::new(&scene)
            .inspect_element(&ElementId::new("results").unwrap())
            .unwrap();
        assert_eq!(inspection.table_fragment, Some(0));
        assert_eq!(inspection.table_rows, Some(1));
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
            if format == ExportFormat::Pdf {
                let source = String::from_utf8_lossy(&bytes);
                assert!(source.contains("/BaseFont /FMSubset1"));
                assert!(source.contains("/FontFile2"));
                assert!(source.contains("/ToUnicode"));
            }
        }
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

    fn deterministic_test_font() -> Vec<u8> {
        include_bytes!("../examples/assets/NotoSans-Regular.ttf").to_vec()
    }
}
