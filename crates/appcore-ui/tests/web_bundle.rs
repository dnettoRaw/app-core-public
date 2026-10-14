// =============================================================================
//        #######
//     ###       ###     F: web_bundle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ui::*;
fn asset(path: &str, media: UiWebMedia, bytes: Vec<u8>) -> UiWebAsset {
    UiWebAsset {
        path: path.into(),
        media,
        bytes,
    }
}
#[test]
fn web_resources_are_exact_bounded_and_explicitly_typed() {
    let bundle = UiWebBundle::new(
        "index.html".into(),
        [
            asset(
                "index.html",
                UiWebMedia::Html,
                b"<!doctype html><script src='main.js'></script>".to_vec(),
            ),
            asset(
                "main.js",
                UiWebMedia::JavaScript,
                b"document.title='Ready';".to_vec(),
            ),
        ],
    )
    .unwrap();
    assert_eq!(bundle.asset_count(), 2);
    assert_eq!(
        bundle.resource("main.js").unwrap().media.mime_type(),
        "text/javascript; charset=utf-8"
    );
    for path in [
        "/index.html",
        "../index.html",
        "a/../index.html",
        "%2e%2e/index.html",
        "https://remote/index.html",
        "index.html?principal=a",
        "missing.html",
    ] {
        assert!(
            matches!(bundle.resource(path), Err(UiError::InvalidRequest)),
            "{path}"
        );
    }
    assert!(bundle
        .content_security_policy()
        .contains("connect-src 'none'"));
}
#[test]
fn web_constructor_rejects_duplicates_bad_entry_utf8_and_oversized_content() {
    assert!(matches!(
        UiWebBundle::new(
            "index.html".into(),
            [asset("index.html", UiWebMedia::Css, vec![])]
        ),
        Err(UiError::InvalidRequest)
    ));
    assert!(matches!(
        UiWebBundle::new(
            "index.html".into(),
            [asset("index.html", UiWebMedia::Html, vec![255])]
        ),
        Err(UiError::InvalidRequest)
    ));
    assert!(matches!(
        UiWebBundle::new(
            "index.html".into(),
            [
                asset("index.html", UiWebMedia::Html, vec![]),
                asset("index.html", UiWebMedia::Html, vec![])
            ]
        ),
        Err(UiError::InvalidRequest)
    ));
    assert!(matches!(
        UiWebBundle::new(
            "index.html".into(),
            [asset(
                "index.html",
                UiWebMedia::Html,
                vec![b'a'; 2 * 1024 * 1024 + 1]
            )]
        ),
        Err(UiError::PayloadTooLarge)
    ));
}

#[test]
fn web_bundle_enforces_count_and_aggregate_memory_admission() {
    let many = std::iter::once(asset("index.html", UiWebMedia::Html, vec![]))
        .chain((0..512).map(|index| asset(&format!("image-{index}.png"), UiWebMedia::Png, vec![])));
    assert!(matches!(
        UiWebBundle::new("index.html".into(), many),
        Err(UiError::Busy)
    ));
    let large =
        std::iter::once(asset("index.html", UiWebMedia::Html, vec![])).chain((0..9).map(|index| {
            asset(
                &format!("image-{index}.png"),
                UiWebMedia::Png,
                vec![0; 2 * 1024 * 1024],
            )
        }));
    assert!(matches!(
        UiWebBundle::new("index.html".into(), large),
        Err(UiError::PayloadTooLarge)
    ));
}
