// =============================================================================
//        #######
//     ###       ###     F: native_counter.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Bounded native counter; trusted activations pass through application-owned limits.
#[cfg(all(feature = "native-macos", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_ui::{elements::*, *};
    let rect = |x, y, width, height| UiRectangle {
        x,
        y,
        width,
        height,
    };
    let page = UiNativePage::new(UiNativePageV1 {
        version: 1,
        title: "AppCore · Native counter".into(),
        width: 480.0,
        height: 300.0,
        nodes: vec![
            text(
                "title",
                rect(24.0, 24.0, 432.0, 28.0),
                "Counter / NumberStepper composition",
            ),
            text("value", rect(24.0, 74.0, 432.0, 28.0), "0 / 20"),
            progress("progress", rect(24.0, 120.0, 432.0, 20.0), 0.0),
            button(
                "minus",
                rect(24.0, 166.0, 130.0, 34.0),
                "− Decrease",
                "counter.minus",
            ),
            button(
                "plus",
                rect(174.0, 166.0, 130.0, 34.0),
                "+ Increase",
                "counter.plus",
            ),
            button(
                "reset",
                rect(324.0, 166.0, 130.0, 34.0),
                "Reset",
                "counter.reset",
            ),
            text(
                "note",
                rect(24.0, 238.0, 432.0, 28.0),
                "Values bounded 0..20 · native labels and progress update",
            ),
        ],
    })?;
    let app = UiNativeApplication::new(1)?;
    let window = UiNativeWindow::open(&app, &page, 16, || {})?;
    let mut value = 0u32;
    window.set_enabled("minus", false)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    while std::time::Instant::now() < deadline
        && window.pump(std::time::Duration::from_millis(16))?
    {
        if let Some(action) = window.next_activation() {
            value = match action.action_id.as_str() {
                "counter.plus" => (value + 1).min(20),
                "counter.minus" => value.saturating_sub(1),
                "counter.reset" => 0,
                _ => value,
            };
            window.set_label("value", &format!("{value} / 20"))?;
            window.set_value("progress", UiNativeValue::Number(f64::from(value) / 20.0))?;
            window.set_enabled("minus", value > 0)?;
            window.set_enabled("plus", value < 20)?;
            println!("counter={value}");
        }
    }
    window.close();
    drop(window);
    println!("native_counter retained_windows={}", app.retained_windows());
    Ok(())
}
#[cfg(not(all(feature = "native-macos", target_os = "macos")))]
fn main() {
    eprintln!("This example requires native-macos on macOS");
}
