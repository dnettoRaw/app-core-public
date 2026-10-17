// =============================================================================
//        #######
//     ###       ###     F: web_3d.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Explicit standalone local example; deployment owns startup, pump and shutdown.
#[cfg(all(feature = "web-macos", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_ui::*;
    let bundle = std::sync::Arc::new(UiWebBundle::new(
        "index.html".into(),
        [
            UiWebAsset {
                path: "index.html".into(),
                media: UiWebMedia::Html,
                bytes: include_bytes!("scene3d/index.html").to_vec(),
            },
            UiWebAsset {
                path: "page.css".into(),
                media: UiWebMedia::Css,
                bytes: include_bytes!("scene3d/page.css").to_vec(),
            },
            UiWebAsset {
                path: "page.js".into(),
                media: UiWebMedia::JavaScript,
                bytes: include_bytes!("scene3d/page.js").to_vec(),
            },
            UiWebAsset {
                path: "city.mjs".into(),
                media: UiWebMedia::JavaScript,
                bytes: include_bytes!("scene3d/city.mjs").to_vec(),
            },
            UiWebAsset {
                path: "showcase.mjs".into(),
                media: UiWebMedia::JavaScript,
                bytes: include_bytes!("scene3d/showcase.mjs").to_vec(),
            },
            UiWebAsset {
                path: "model.mjs".into(),
                media: UiWebMedia::JavaScript,
                bytes: include_bytes!("scene3d/model.mjs").to_vec(),
            },
        ],
    )?);
    let app = UiNativeApplication::new(1)?;
    let window = UiWebWindow::open(
        &app,
        bundle,
        "AppCore · 3D Showcase".into(),
        1040.0,
        740.0,
        || {},
    )?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    while std::time::Instant::now() < deadline
        && window.pump(std::time::Duration::from_millis(8))?
    {}
    let loaded = window.loaded();
    window.close();
    drop(window);
    println!(
        "example=web_3d loaded={loaded} retained_windows={}",
        app.retained_windows()
    );
    Ok(())
}
#[cfg(not(all(feature = "web-macos", target_os = "macos")))]
fn main() {
    eprintln!("This example requires web-macos on macOS");
}
