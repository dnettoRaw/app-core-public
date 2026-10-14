// =============================================================================
//        #######
//     ###       ###     F: web_page.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Explicit local web preview. No server, Runtime, authentication or business behavior is started.
#[cfg(all(feature = "web-macos", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_ui::*;
    let bundle = std::sync::Arc::new(UiWebBundle::new(
        "index.html".into(),
        [
            UiWebAsset {
                path: "index.html".into(),
                media: UiWebMedia::Html,
                bytes: include_bytes!("web-preview/index.html").to_vec(),
            },
            UiWebAsset {
                path: "page.css".into(),
                media: UiWebMedia::Css,
                bytes: include_bytes!("web-preview/page.css").to_vec(),
            },
            UiWebAsset {
                path: "page.js".into(),
                media: UiWebMedia::JavaScript,
                bytes: include_bytes!("web-preview/page.js").to_vec(),
            },
        ],
    )?);
    let application = UiNativeApplication::new(1)?;
    let closed = std::rc::Rc::new(std::cell::Cell::new(false));
    let hook = closed.clone();
    let window = UiWebWindow::open(
        &application,
        bundle,
        "AppCore Web Page".into(),
        560.0,
        300.0,
        move || hook.set(true),
    )?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    while std::time::Instant::now() < deadline
        && window.pump(std::time::Duration::from_millis(50))?
    {}
    let loaded = window.loaded();
    let rejected = window.rejected_requests();
    window.close();
    drop(window);
    println!(
        "loaded={loaded} closed={} rejected_requests={rejected} retained_windows={}",
        closed.get(),
        application.retained_windows()
    );
    Ok(())
}
#[cfg(not(all(feature = "web-macos", target_os = "macos")))]
fn main() {
    eprintln!("This preview requires the web-macos feature on macOS");
}
