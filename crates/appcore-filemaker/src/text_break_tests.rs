// =============================================================================
//        #######
//     ###       ###     F: text_break_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{text::break_lines, ErrorCode, Unit};

fn character_width(source: &str) -> crate::Result<Unit> {
    Unit::points(i64::try_from(source.chars().count()).unwrap())
}

#[test]
fn breaks_overlong_latin_and_cjk_words_at_grapheme_boundaries() {
    assert_eq!(
        break_lines("abcdefgh", Unit::points(3).unwrap(), character_width).unwrap(),
        ["abc", "def", "gh"]
    );
    assert_eq!(
        break_lines("超長單詞", Unit::points(2).unwrap(), character_width).unwrap(),
        ["超長", "單詞"]
    );
}

#[test]
fn preserves_explicit_lines_and_rejects_measurement_failure() {
    assert_eq!(
        break_lines("one\ntwo", Unit::points(3).unwrap(), character_width).unwrap(),
        ["one", "two"]
    );
    let error = break_lines("failure", Unit::points(3).unwrap(), |_| {
        Err(crate::FileMakerError::new(
            ErrorCode::LimitExceeded,
            "synthetic measurement budget",
        ))
    })
    .unwrap_err();
    assert_eq!(error.code(), ErrorCode::LimitExceeded);
}
