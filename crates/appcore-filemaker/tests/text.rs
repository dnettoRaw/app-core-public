// =============================================================================
//        #######
//     ###       ###     F: text.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use appcore_filemaker::{
    ErrorCode, FontAsset, FontManager, Size, TextDiagnostic, TextEngine, TextOptions, TextOverflow,
    Unit, WritingMode,
};

fn options() -> TextOptions {
    TextOptions {
        font: "missing".to_owned(),
        font_size: Unit::points(12).unwrap(),
        min_font_size: Unit::points(8).unwrap(),
        bounds: Size::new(Unit::points(100).unwrap(), Unit::points(100).unwrap()).unwrap(),
        max_lines: None,
        overflow: TextOverflow::Wrap,
        line_height: 1_200_000,
        writing_mode: WritingMode::Horizontal,
    }
}

#[test]
fn missing_explicit_font_fails_for_unicode_instead_of_falling_back() {
    let manager = FontManager::default();
    let engine = TextEngine::new(&manager);
    for overflow in [
        TextOverflow::Wrap,
        TextOverflow::Shrink,
        TextOverflow::Ellipsis,
        TextOverflow::Clip,
        TextOverflow::Expand,
        TextOverflow::Error,
    ] {
        let mut options = options();
        options.overflow = overflow;
        for text in ["Latin", "العربية", "中文", "👩🏽‍💻"] {
            assert_eq!(
                engine.layout(text, &options).unwrap_err().code(),
                ErrorCode::FontMissing,
                "overflow mode {overflow:?} masked the missing font"
            );
        }
    }
}

#[test]
fn rejects_zero_line_limit_as_layout_not_font_failure() {
    let manager = FontManager::default();
    let engine = TextEngine::new(&manager);
    let mut options = options();
    options.max_lines = Some(0);
    assert_eq!(
        engine.layout("x", &options).unwrap_err().code(),
        ErrorCode::LayoutInvalid
    );
}

#[test]
fn vertical_japanese_is_shaped_into_top_to_bottom_columns() {
    let mut manager = FontManager::default();
    manager
        .register(
            FontAsset::new(
                "Japanese",
                include_bytes!("assets/NotoSansJP-Test.ttf").to_vec(),
                0,
            )
            .unwrap(),
        )
        .unwrap();
    let engine = TextEngine::new(&manager);
    let mut options = options();
    options.font = "Japanese".to_owned();
    options.bounds = Size::new(Unit::points(60).unwrap(), Unit::points(24).unwrap()).unwrap();
    options.writing_mode = WritingMode::Vertical;

    let layout = engine.layout("日本語の運用レポート", &options).unwrap();

    assert_eq!(layout.writing_mode, WritingMode::Vertical);
    assert!(layout.lines.len() > 1);
    assert!(layout
        .lines
        .iter()
        .all(|column| column.width <= options.bounds.height));
    assert!(layout
        .lines
        .iter()
        .flat_map(|column| &column.runs)
        .flat_map(|run| &run.glyphs)
        .any(|glyph| glyph.advance_y != Unit::ZERO));
    assert!(!layout
        .diagnostics
        .contains(&TextDiagnostic::VerticalWritingUnavailable));
}
