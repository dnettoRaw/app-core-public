// =============================================================================
//        #######
//     ###       ###     F: native_page.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Explicit standalone native page preview; deployment owns the event pump and exit.
#[cfg(all(feature = "native-macos", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_ui::*;
    let page = UiNativePage::new(UiNativePageV1 {
        version: 1,
        title: "AppCore Native Page".into(),
        width: 480.0,
        height: 240.0,
        nodes: vec![
            UiNativeNodeV1::Text {
                id: "heading".into(),
                rect: UiRectangle {
                    x: 24.0,
                    y: 24.0,
                    width: 432.0,
                    height: 36.0,
                },
                text: "Página nativa renderizada pelo AppCore".into(),
            },
            UiNativeNodeV1::Text {
                id: "status".into(),
                rect: UiRectangle {
                    x: 24.0,
                    y: 70.0,
                    width: 432.0,
                    height: 30.0,
                },
                text: "Rust · controlos do sistema · ação limitada".into(),
            },
            UiNativeNodeV1::Button {
                id: "inspect".into(),
                rect: UiRectangle {
                    x: 24.0,
                    y: 124.0,
                    width: 200.0,
                    height: 36.0,
                },
                label: "Verificar ação".into(),
                action: "state.inspect".into(),
            },
        ],
    })?;
    let application = UiNativeApplication::new(1)?;
    let closed = std::rc::Rc::new(std::cell::Cell::new(false));
    let callback_closed = closed.clone();
    let window = UiNativeWindow::open(&application, &page, 8, move || callback_closed.set(true))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    while std::time::Instant::now() < deadline
        && window.pump(std::time::Duration::from_millis(50))?
    {
        if let Some(action) = window.next_activation() {
            println!(
                "activation={} action={}",
                action.element_id, action.action_id
            );
        }
    }
    window.close();
    let failures = window.callback_failures();
    drop(window);
    println!(
        "closed={} retained_windows={} callback_failures={}",
        closed.get(),
        application.retained_windows(),
        failures
    );
    Ok(())
}
#[cfg(not(all(feature = "native-macos", target_os = "macos")))]
fn main() {
    eprintln!("This preview requires the native-macos feature on macOS");
}
