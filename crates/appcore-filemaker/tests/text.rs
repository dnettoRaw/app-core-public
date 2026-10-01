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
    Alignment, Compiler, DataValue, ErrorCode, FontAsset, FontManager, LayoutEngine, LayoutOptions,
    PdfStandardFont, ResourceLimits, Size, TextDiagnostic, TextEngine, TextOptions, TextOverflow,
    Unit, WritingMode,
};
use std::collections::BTreeMap;

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
        align_x: Alignment::Start,
        padding_inline: Unit::ZERO,
    }
}

#[test]
fn inline_alignment_uses_each_measured_line_width() {
    use appcore_filemaker::{Alignment, TextLayout, TextLine};

    let line = TextLine {
        source_text: "source line".to_owned(),
        runs: Vec::new(),
        width: Unit::points(20).unwrap(),
        height: Unit::points(10).unwrap(),
    };
    for (align_x, expected) in [
        (Alignment::Start, 0),
        (Alignment::Center, 40),
        (Alignment::End, 80),
    ] {
        let layout = TextLayout {
            writing_mode: WritingMode::Horizontal,
            lines: vec![line.clone()],
            measured: Size::new(Unit::points(100).unwrap(), Unit::points(10).unwrap()).unwrap(),
            font_size: Unit::points(10).unwrap(),
            paint_offset_y: Unit::ZERO,
            diagnostics: Vec::new(),
            align_x,
            padding_inline: Unit::ZERO,
            padding: appcore_filemaker::Insets::default(),
        };
        assert_eq!(
            layout
                .inline_offset(Unit::points(100).unwrap(), &line)
                .unwrap(),
            Unit::points(expected).unwrap()
        );
    }
}

#[test]
fn pdf_standard_font_uses_afm_widths_and_fails_closed_outside_winansi() {
    let mut manager = FontManager::default();
    manager
        .register_pdf_standard("Helvetica", PdfStandardFont::Helvetica)
        .unwrap();
    let engine = TextEngine::new(&manager);
    let mut text_options = options();
    text_options.font = "Helvetica".to_owned();
    text_options.bounds.width = Unit::points(200).unwrap();

    let layout = engine.layout("AV 1 234,56 €", &text_options).unwrap();
    assert_eq!(layout.lines.len(), 1, "layout={layout:?}");
    assert!(layout.lines[0].width < Unit::points(100).unwrap());
    assert!(layout.lines[0]
        .runs
        .iter()
        .flat_map(|run| &run.glyphs)
        .all(|glyph| glyph.id <= u16::from(u8::MAX)));
    let pair_width = engine.layout("AV", &text_options).unwrap().lines[0].width;
    let separate_width = engine.layout("A", &text_options).unwrap().lines[0]
        .width
        .checked_add(engine.layout("V", &text_options).unwrap().lines[0].width)
        .unwrap();
    assert!(
        pair_width < separate_width,
        "AFM kerning must affect shaping"
    );

    let error = engine
        .layout("Narrow\u{202f}space", &text_options)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::FontMissing);
}

#[test]
fn bound_inline_text_segments_measure_gap_and_align_as_one_line() {
    let yaml = br"filemaker: '1.0'
model: document
id: inline-segments
page: { width: 240pt, height: 50pt }
data_schema:
  amount: { type: string }
elements:
  - id: total
    type: text
    x: 10pt
    y: 10pt
    width: 200pt
    height: 20pt
    text_segments:
      - { text: 'TOTAL TTC:', gap_after: 7pt }
      - { binding: data.amount }
    text_options: { overflow: error, max_lines: 1, align_x: end }
    style: { font: HelveticaBold, font_size: 16pt }
";
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build().unwrap();
    let template = compiler.compile_template_yaml(yaml).unwrap();
    let data = DataValue::Object(BTreeMap::from([(
        "amount".to_owned(),
        DataValue::String("246,00 €".to_owned()),
    )]));
    let document = compiler.bind(&template, &data, &[]).unwrap();
    let mut fonts = FontManager::default();
    fonts
        .register_pdf_standard("HelveticaBold", PdfStandardFont::HelveticaBold)
        .unwrap();
    let scene = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())
        .unwrap()
        .resolve(&document)
        .unwrap();
    let resolved = scene.pages[0]
        .elements
        .iter()
        .find(|element| element.id.as_str() == "total")
        .unwrap();
    let layout = resolved.text_layout.as_ref().unwrap();
    let line = &layout.lines[0];
    let options = TextOptions {
        font: "HelveticaBold".to_owned(),
        font_size: Unit::points(16).unwrap(),
        min_font_size: Unit::points(8).unwrap(),
        bounds: Size::new(Unit::points(200).unwrap(), Unit::points(20).unwrap()).unwrap(),
        max_lines: Some(1),
        overflow: TextOverflow::Error,
        line_height: 1_200_000,
        writing_mode: WritingMode::Horizontal,
        align_x: Alignment::Start,
        padding_inline: Unit::ZERO,
    };
    let engine = TextEngine::new(&fonts);
    let expected = engine.layout("TOTAL TTC:", &options).unwrap().lines[0]
        .width
        .checked_add(Unit::points(7).unwrap())
        .unwrap()
        .checked_add(engine.layout("246,00 €", &options).unwrap().lines[0].width)
        .unwrap();
    assert_eq!(line.width, expected);
    assert_eq!(line.source_text, "TOTAL TTC:246,00 €");
    assert_eq!(line.runs.len(), 3);
    assert!(line.runs[1].text.is_empty());
    assert_eq!(line.runs[1].width, Unit::points(7).unwrap());
    assert_eq!(
        layout
            .inline_offset(Unit::points(200).unwrap(), line)
            .unwrap(),
        Unit::points(200).unwrap().checked_sub(expected).unwrap()
    );
}

#[test]
fn inline_padding_is_preserved_around_every_aligned_line() {
    use appcore_filemaker::{TextLayout, TextLine};

    let line = TextLine {
        source_text: "source line".to_owned(),
        runs: Vec::new(),
        width: Unit::points(20).unwrap(),
        height: Unit::points(10).unwrap(),
    };
    let padding = Unit::points(10).unwrap();
    for (align_x, expected) in [
        (Alignment::Start, 10),
        (Alignment::Center, 40),
        (Alignment::End, 70),
    ] {
        let layout = TextLayout {
            writing_mode: WritingMode::Horizontal,
            lines: vec![line.clone()],
            measured: Size::new(Unit::points(100).unwrap(), Unit::points(10).unwrap()).unwrap(),
            font_size: Unit::points(10).unwrap(),
            paint_offset_y: Unit::ZERO,
            diagnostics: Vec::new(),
            align_x,
            padding_inline: padding,
            padding: appcore_filemaker::Insets::default(),
        };
        assert_eq!(
            layout
                .inline_offset(Unit::points(100).unwrap(), &line)
                .unwrap(),
            Unit::points(expected).unwrap()
        );
    }
}

#[test]
fn inline_padding_reduces_wrapping_width() {
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
    options.bounds = Size::new(Unit::points(40).unwrap(), Unit::points(100).unwrap()).unwrap();
    options.padding_inline = Unit::points(4).unwrap();

    let layout = engine
        .layout("日本語の運用レポート日本語の運用レポート", &options)
        .unwrap();

    assert!(layout.lines.len() > 1);
    assert!(layout
        .lines
        .iter()
        .all(|line| line.width <= Unit::points(32).unwrap()));
    assert!(layout.lines.iter().all(|line| {
        layout
            .inline_offset(options.bounds.width, line)
            .is_ok_and(|offset| offset >= options.padding_inline)
    }));
}

#[test]
fn paragraph_indentation_is_retained_on_wrapped_lines() {
    let mut manager = FontManager::default();
    manager
        .register(
            FontAsset::new(
                "Pinned",
                include_bytes!("../examples/assets/NotoSans-Regular.ttf").to_vec(),
                0,
            )
            .unwrap(),
        )
        .unwrap();
    let engine = TextEngine::new(&manager);
    let mut text_options = options();
    text_options.font = "Pinned".to_owned();
    text_options.font_size = Unit::points(10).unwrap();
    text_options.bounds = Size::new(Unit::points(75).unwrap(), Unit::points(120).unwrap()).unwrap();
    let layout = engine
        .layout(
            "    A long description wraps across several visual lines and keeps its indentation.",
            &text_options,
        )
        .unwrap();

    assert!(layout.lines.len() > 1, "fixture did not wrap");
    assert!(layout
        .lines
        .iter()
        .all(|line| line.source_text.starts_with("    ")));
}

#[test]
fn right_alignment_uses_shaped_width_after_wrap_and_shrink() {
    let mut manager = FontManager::default();
    manager
        .register(
            FontAsset::new(
                "Pinned",
                include_bytes!("../examples/assets/NotoSans-Regular.ttf").to_vec(),
                0,
            )
            .unwrap(),
        )
        .unwrap();
    let engine = TextEngine::new(&manager);
    let available = Unit::points(52).unwrap();
    let padding = Unit::points(3).unwrap();
    for text in ["0.00", "72.00", "1,234.56", "VARIABLE"] {
        let mut options = options();
        options.font = "Pinned".to_owned();
        options.font_size = Unit::points(32).unwrap();
        options.min_font_size = Unit::points(7).unwrap();
        options.bounds = Size::new(available, Unit::points(48).unwrap()).unwrap();
        options.overflow = TextOverflow::Shrink;
        options.align_x = Alignment::End;
        options.padding_inline = padding;

        let layout = engine.layout(text, &options).unwrap();

        assert!(!layout.lines.is_empty(), "{text}");
        assert!(
            layout.lines.iter().all(|line| {
                layout
                    .inline_offset(available, line)
                    .and_then(|offset| offset.checked_add(line.width))
                    .is_ok_and(|right| right == available.checked_sub(padding).unwrap())
            }),
            "right edge differs for {text}"
        );
        assert!(layout.font_size <= options.font_size);
        if text == "1,234.56" {
            assert!(
                layout.font_size < options.font_size,
                "wide value did not shrink"
            );
        }
    }

    let mut wrapped_options = options();
    wrapped_options.font = "Pinned".to_owned();
    wrapped_options.bounds = Size::new(available, Unit::points(80).unwrap()).unwrap();
    wrapped_options.align_x = Alignment::End;
    wrapped_options.padding_inline = padding;
    let wrapped = engine
        .layout(
            "a deliberately long description that wraps over multiple lines",
            &wrapped_options,
        )
        .unwrap();
    assert!(wrapped.lines.len() > 1, "fixture did not wrap");
    assert!(wrapped.lines.iter().all(|line| {
        wrapped
            .inline_offset(available, line)
            .and_then(|offset| offset.checked_add(line.width))
            .is_ok_and(|right| right == available.checked_sub(padding).unwrap())
    }));
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
