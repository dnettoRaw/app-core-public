// =============================================================================
//        #######
//     ###       ###     F: source.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

#[cfg(test)]
mod tests {
    use appcore_filemaker::*;

    const BASIC: &str = r#"
filemaker: "1.0"
model: document
id: example
page:
  preset: A4
elements:
  - id: title
    type: text
    x: 10mm
    y: 12mm
    width: 80%
    height: auto
    text: "Olá مرحبا 世界"
"#;

    #[test]
    fn yaml_and_rust_enter_the_same_ir() {
        let limits = ResourceLimits::default();
        let source = TemplateSourceV1::parse_yaml(BASIC.as_bytes(), &limits).unwrap();
        let from_yaml = source
            .to_ir(&PresetRegistry::v1().unwrap(), &limits)
            .unwrap();
        let encoded = serde_yaml::to_string(&source).unwrap();
        let reparsed = TemplateSourceV1::parse_yaml(encoded.as_bytes(), &limits).unwrap();
        assert_eq!(
            from_yaml,
            reparsed
                .to_ir(&PresetRegistry::v1().unwrap(), &limits)
                .unwrap()
        );
    }

    #[test]
    fn rejects_unknown_fields_and_versions() {
        let limits = ResourceLimits::default();
        let unknown = BASIC.replace("id: example", "id: example\nunknown: true");
        assert_eq!(
            TemplateSourceV1::parse_yaml(unknown.as_bytes(), &limits)
                .unwrap_err()
                .code(),
            ErrorCode::SchemaSyntax
        );
        let future = BASIC.replace("\"1.0\"", "\"2.0\"");
        assert_eq!(
            TemplateSourceV1::parse_yaml(future.as_bytes(), &limits)
                .unwrap_err()
                .code(),
            ErrorCode::SchemaVersion
        );
    }

    #[test]
    fn text_options_compile_to_the_format_neutral_ir() {
        let yaml = BASIC.replace(
            "    text: \"Olá مرحبا 世界\"",
            "    text: \"Olá مرحبا 世界\"\n    text_options:\n      overflow: shrink\n      max_lines: 2\n      min_font_size: 8pt\n      line_height: 1400000\n      writing_mode: vertical\n      align_x: end\n      padding_inline: 4pt\n      padding: { top: 1pt, right: 2pt, bottom: 3pt, left: 4pt }",
        );
        let limits = ResourceLimits::default();
        let template = TemplateSourceV1::parse_yaml(yaml.as_bytes(), &limits)
            .unwrap()
            .to_ir(&PresetRegistry::v1().unwrap(), &limits)
            .unwrap();
        let options = &template.elements[0].text_options;
        assert_eq!(options.overflow, TextOverflow::Shrink);
        assert_eq!(options.max_lines, Some(2));
        assert_eq!(
            options.min_font_size,
            Some(Length::Absolute(Unit::points(8).unwrap()))
        );
        assert_eq!(options.line_height, 1_400_000);
        assert_eq!(options.writing_mode, WritingMode::Vertical);
        assert_eq!(options.align_x, Alignment::End);
        assert_eq!(
            options.padding_inline,
            Length::Absolute(Unit::points(4).unwrap())
        );
        assert_eq!(
            options.padding.top,
            Length::Absolute(Unit::points(1).unwrap())
        );
        assert_eq!(
            options.padding.right,
            Length::Absolute(Unit::points(2).unwrap())
        );
        assert_eq!(
            options.padding.bottom,
            Length::Absolute(Unit::points(3).unwrap())
        );
        assert_eq!(
            options.padding.left,
            Length::Absolute(Unit::points(4).unwrap())
        );
    }

    #[test]
    fn underline_and_line_height_styles_are_preserved_by_the_yaml_frontend() {
        let yaml = BASIC.replace(
            "    text: \"Olá مرحبا 世界\"",
            "    text: \"Olá مرحبا 世界\"\n    style: { underline: true, line_height: 1250000 }",
        );
        let limits = ResourceLimits::default();
        let template = TemplateSourceV1::parse_yaml(yaml.as_bytes(), &limits)
            .unwrap()
            .to_ir(&PresetRegistry::v1().unwrap(), &limits)
            .unwrap();
        assert_eq!(template.elements[0].style.underline, Some(true));
        assert_eq!(template.elements[0].style.line_height, Some(1_250_000));
    }

    #[test]
    fn rejects_invalid_or_misplaced_text_options() {
        let limits = ResourceLimits::default();
        let zero_lines = BASIC.replace(
            "    text: \"Olá مرحبا 世界\"",
            "    text: \"Olá مرحبا 世界\"\n    text_options: { max_lines: 0 }",
        );
        assert_eq!(
            TemplateSourceV1::parse_yaml(zero_lines.as_bytes(), &limits)
                .unwrap_err()
                .code(),
            ErrorCode::SchemaField
        );

        let automatic_padding = BASIC.replace(
            "    text: \"Olá مرحبا 世界\"",
            "    text: \"Olá مرحبا 世界\"\n    text_options: { padding_inline: auto }",
        );
        assert_eq!(
            TemplateSourceV1::parse_yaml(automatic_padding.as_bytes(), &limits)
                .unwrap_err()
                .code(),
            ErrorCode::SchemaField
        );

        for padding in ["{ top: auto }", "{ left: -1pt }", "{ right: 50% }"] {
            let invalid_block_padding = BASIC.replace(
                "    text: \"Olá مرحبا 世界\"",
                &format!(
                    "    text: \"Olá مرحبا 世界\"\n    text_options: {{ padding: {padding} }}"
                ),
            );
            assert_eq!(
                TemplateSourceV1::parse_yaml(invalid_block_padding.as_bytes(), &limits)
                    .unwrap_err()
                    .code(),
                ErrorCode::SchemaField,
                "invalid text block padding {padding} must be rejected"
            );
        }

        let non_text = BASIC.replace("type: text", "type: rect").replace(
            "    text: \"Olá مرحبا 世界\"",
            "    text_options: { overflow: clip }",
        );
        assert_eq!(
            TemplateSourceV1::parse_yaml(non_text.as_bytes(), &limits)
                .unwrap_err()
                .code(),
            ErrorCode::SchemaField
        );

        let oversized_line_height = BASIC.replace(
            "    text: \"Olá مرحبا 世界\"",
            "    text: \"Olá مرحبا 世界\"\n    style: { line_height: 4000001 }",
        );
        assert!(
            TemplateSourceV1::parse_yaml(oversized_line_height.as_bytes(), &limits)
                .unwrap()
                .to_ir(&PresetRegistry::v1().unwrap(), &limits)
                .is_err()
        );
    }

    #[test]
    fn constraints_and_alignment_are_preserved_in_ir() {
        let yaml = BASIC.replace("    x: 10mm\n", "").replace(
            "    text: \"Olá مرحبا 世界\"",
            "    text: \"Olá مرحبا 世界\"\n    constraints: { min_width: 20pt, preferred_width: 40pt, max_width: 60pt, aspect_ratio: 2000000 }\n    align_x: center",
        );
        let limits = ResourceLimits::default();
        let template = TemplateSourceV1::parse_yaml(yaml.as_bytes(), &limits)
            .unwrap()
            .to_ir(&PresetRegistry::v1().unwrap(), &limits)
            .unwrap();
        let geometry = &template.elements[0].geometry;
        assert_eq!(geometry.align_x, Some(Alignment::Center));
        assert_eq!(geometry.constraints.aspect_ratio, Some(2_000_000));
        assert_eq!(
            geometry.constraints.preferred_width,
            Some(Length::Absolute(Unit::points(40).unwrap()))
        );

        let contradictory = BASIC.replace(
            "    text: \"Olá مرحبا 世界\"",
            "    text: \"Olá مرحبا 世界\"\n    align_x: center",
        );
        assert_eq!(
            TemplateSourceV1::parse_yaml(contradictory.as_bytes(), &limits)
                .unwrap_err()
                .code(),
            ErrorCode::SchemaField
        );
    }

    #[test]
    fn distribution_requires_a_flow_and_is_preserved_in_ir() {
        let flow = BASIC.replace("type: text", "type: group").replace(
            "    text: \"Olá مرحبا 世界\"",
            "    layout: flow_vertical\n    distribute: space_evenly",
        );
        let limits = ResourceLimits::default();
        let template = TemplateSourceV1::parse_yaml(flow.as_bytes(), &limits)
            .unwrap()
            .to_ir(&PresetRegistry::v1().unwrap(), &limits)
            .unwrap();
        assert_eq!(template.elements[0].distribute, Distribution::SpaceEvenly);

        let invalid = BASIC.replace(
            "    text: \"Olá مرحبا 世界\"",
            "    text: \"Olá مرحبا 世界\"\n    distribute: center",
        );
        assert_eq!(
            TemplateSourceV1::parse_yaml(invalid.as_bytes(), &limits)
                .unwrap_err()
                .code(),
            ErrorCode::SchemaField
        );
    }
}
