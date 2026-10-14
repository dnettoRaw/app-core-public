// =============================================================================
//        #######
//     ###       ###     F: native_gallery.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Native function-built gallery and deterministic state machine; no production data or AI session.
#[cfg(any(all(feature = "native-macos", target_os = "macos"), test))]
#[path = "native-gallery/state.rs"]
mod state;
#[cfg(all(feature = "native-macos", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_ui::*;
    let app = UiNativeApplication::new(1)?;
    let window = UiNativeWindow::open(&app, &page()?, 32, || {})?;
    let mut state = state::State::default();
    render(&window, &state)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    while std::time::Instant::now() < deadline
        && window.pump(std::time::Duration::from_millis(16))?
    {
        if let Some(action) = window.next_activation() {
            let command = match action.action_id.as_str() {
                "demo.review" => Some(state::Command::Review(read_draft(&window)?)),
                "demo.apply" => Some(state::Command::Apply),
                "demo.edit" => Some(state::Command::Edit),
                "demo.reset" => Some(state::Command::Reset),
                "demo.script" => {
                    run_script(&mut state);
                    render(&window, &state)?;
                    None
                }
                _ => {
                    window.set_label(
                        "feedback",
                        "Input changed — Review validates the complete draft.",
                    )?;
                    None
                }
            };
            if let Some(command) = command {
                match state.execute(state.revision, command) {
                    Ok(()) => {
                        render(&window, &state)?;
                        println!("revision={} phase={:?}", state.revision, state.phase);
                    }
                    Err(error) => window.set_label("feedback", error)?,
                }
            }
        }
    }
    window.close();
    let dropped = window.dropped_activations();
    drop(window);
    println!(
        "native_gallery revision={} retained_windows={} dropped={dropped}",
        state.revision,
        app.retained_windows()
    );
    Ok(())
}
#[cfg(all(feature = "native-macos", target_os = "macos"))]
fn page() -> appcore_ui::UiResult<appcore_ui::UiNativePage> {
    use appcore_ui::{elements::*, *};
    let rect = |x, y, width, height| UiRectangle {
        x,
        y,
        width,
        height,
    };
    UiNativePage::new(UiNativePageV1{version:1,title:"AppCore · Native element laboratory".into(),width:800.0,height:590.0,nodes:vec![
        text("heading",rect(28.0,24.0,744.0,28.0),"Native element laboratory · Rust functions / system controls"),
        text("phase",rect(28.0,68.0,744.0,24.0),"Editing · revision 0"),
        text("field-label",rect(28.0,112.0,220.0,24.0),"TextInput / SearchField primitive"),
        input("name",rect(28.0,142.0,440.0,30.0),"Local workspace","Workspace name","demo.input"),
        text("select-label",rect(28.0,192.0,350.0,24.0),"Select / Dropdown primitive"),
        select("mode",rect(28.0,224.0,260.0,32.0),vec!["Local".into(),"Isolated".into(),"Preview".into()],0,"demo.select"),
        toggle("enabled",rect(330.0,224.0,420.0,32.0),"Checkbox / Switch primitive",true,"demo.toggle"),
        text("slider-label",rect(28.0,276.0,744.0,24.0),"Numeric control · finite interval 0..100"),
        slider("strength",rect(28.0,312.0,440.0,28.0),0.0,100.0,50.0,"demo.slider"),
        text("summary",rect(500.0,310.0,272.0,28.0),"Value: 50 / 100"),
        progress("progress",rect(28.0,368.0,744.0,20.0),0.0),
        text("feedback",rect(28.0,406.0,744.0,48.0),"Editing → Review → Applied. Invalid/stale commands are rejected."),
        button("review",rect(28.0,474.0,140.0,36.0),"Review","demo.review"),
        button("apply",rect(178.0,474.0,140.0,36.0),"Apply","demo.apply"),
        button("edit",rect(328.0,474.0,110.0,36.0),"Edit","demo.edit"),
        button("reset",rect(448.0,474.0,110.0,36.0),"Reset","demo.reset"),
        button("script",rect(568.0,474.0,204.0,36.0),"Scripted commands","demo.script"),
        text("note",rect(28.0,538.0,744.0,24.0),"Same typed reducer for clicks or future tools · no AI connected · no business effect"),
    ]})
}
#[cfg(all(feature = "native-macos", target_os = "macos"))]
fn read_draft(window: &appcore_ui::UiNativeWindow) -> appcore_ui::UiResult<state::Draft> {
    use appcore_ui::*;
    let UiNativeValue::Text(name) = window.value("name")? else {
        return Err(UiError::InvalidRequest);
    };
    let UiNativeValue::Selection(mode) = window.value("mode")? else {
        return Err(UiError::InvalidRequest);
    };
    let UiNativeValue::Boolean(enabled) = window.value("enabled")? else {
        return Err(UiError::InvalidRequest);
    };
    let UiNativeValue::Number(strength) = window.value("strength")? else {
        return Err(UiError::InvalidRequest);
    };
    Ok(state::Draft {
        name,
        mode,
        enabled,
        strength,
    })
}
#[cfg(all(feature = "native-macos", target_os = "macos"))]
fn render(window: &appcore_ui::UiNativeWindow, state: &state::State) -> appcore_ui::UiResult<()> {
    use appcore_ui::*;
    use state::Phase;
    window.set_label(
        "phase",
        &format!("{:?} · revision {}", state.phase, state.revision),
    )?;
    window.set_label(
        "summary",
        &format!("Value: {:.0} / 100", state.draft.strength),
    )?;
    window.set_label(
        "feedback",
        match state.phase {
            Phase::Editing => "Edit fields, then Review. No changes are applied yet.",
            Phase::Review => "Review accepted. Apply is now available; fields are locked.",
            Phase::Applied => "Applied to this demo state only. Edit reopens the draft.",
        },
    )?;
    for id in ["name", "mode", "enabled", "strength", "review"] {
        window.set_enabled(id, state.phase == Phase::Editing)?;
    }
    window.set_enabled("apply", state.phase == Phase::Review)?;
    window.set_enabled("edit", state.phase != Phase::Editing)?;
    window.set_value(
        "progress",
        UiNativeValue::Number(match state.phase {
            Phase::Editing => 0.0,
            Phase::Review => 0.5,
            Phase::Applied => 1.0,
        }),
    )?;
    window.set_value("name", UiNativeValue::Text(state.draft.name.clone()))?;
    window.set_value("mode", UiNativeValue::Selection(state.draft.mode))?;
    window.set_value("enabled", UiNativeValue::Boolean(state.draft.enabled))?;
    window.set_value("strength", UiNativeValue::Number(state.draft.strength))?;
    Ok(())
}
#[cfg(all(feature = "native-macos", target_os = "macos"))]
fn run_script(state: &mut state::State) {
    let commands = [
        state::Command::Reset,
        state::Command::Review(state::Draft {
            name: "Script-controlled workspace".into(),
            mode: 2,
            enabled: false,
            strength: 75.0,
        }),
        state::Command::Apply,
    ];
    for command in commands {
        let revision = state.revision;
        if let Err(error) = state.execute(revision, command) {
            eprintln!("script rejected: {error}");
            break;
        }
    }
}
#[cfg(not(all(feature = "native-macos", target_os = "macos")))]
fn main() {
    eprintln!("This gallery requires native-macos on macOS");
}
