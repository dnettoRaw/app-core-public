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
            "    text: \"Olá مرحبا 世界\"\n    text_options:\n      overflow: shrink\n      max_lines: 2\n      min_font_size: 8pt\n      line_height: 1400000\n      writing_mode: vertical",
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
