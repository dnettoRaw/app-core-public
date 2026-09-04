// =============================================================================
//        #######
//     ###       ###     F: cli.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::path::PathBuf;
use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_appcore-filemaker"))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/simple.yaml")
}

fn temporary_directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "appcore-filemaker-cli-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir(&path).unwrap();
    path
}

#[test]
fn schema_and_check_have_stable_json() {
    let schema = binary().args(["schema", "--json"]).output().unwrap();
    assert!(schema.status.success());
    assert!(schema.stdout.starts_with(b"{\n"));
    assert!(schema.stdout.ends_with(b"\n"));
    let value: serde_json::Value = serde_json::from_slice(&schema.stdout).unwrap();
    assert_eq!(value["version"], "1.0");
    assert_eq!(value["text_overflow"][2], "ellipsis");
    assert_eq!(value["writing_modes"][1], "vertical_rl");
    assert_eq!(value["prepared_text_capabilities"][0], "color_emoji");
    assert_eq!(value["color_sources"][6], "typed");
    assert_eq!(value["resolvers"][1], "font_sandbox");
    assert_eq!(value["style_cascade"][6], "export_override");
    assert_eq!(value["export_style_override"][4], "paint_only");
    assert_eq!(value["paint_order"][3], "collision_independent");
    assert_eq!(value["patch_operations"][2], "set_style");
    assert_eq!(value["image_fit"][4], "scale_down");
    assert_eq!(value["image_contract"][3], "optional_exif");
    assert_eq!(value["coordinate_units"][7], "normalized_0_to_1");
    assert_eq!(value["canvas_primitives"][6], "polygon");
    assert_eq!(value["path_commands"][3], "close");
    assert_eq!(value["validation_stages"][3], "preflight");
    assert_eq!(value["fingerprint_inputs"][4], "patches");
    assert_eq!(value["cache_contract"][1], "resolve_on_miss");
    assert_eq!(value["export_contract"][6], "pdf_metadata");
    assert_eq!(value["debug_overlay"][0], "grid_1_5_10_20");
    assert_eq!(value["mask_views"][3], "combined");
    assert_eq!(value["mask_json"][3], "overflow");
    assert_eq!(value["inspect_trace"][6], "provenance");
    assert_eq!(value["layout_constraints"][3], "aspect_ratio_millionths");
    assert_eq!(value["guide_anchor"], "guide:name[+offset]");
    assert_eq!(value["flow_distribution"][3], "space_between");
    assert_eq!(value["exclusions"][1], "non_painted");
    assert_eq!(value["page_layers"][7], "page_n_of_m");
    assert_eq!(value["table_planning"][8], "streaming");
    assert_eq!(value["table_planning"][9], "resolved_fragments");
    assert_eq!(value["table_source"][2], "typed_rows");
    assert_eq!(value["table_rendering"][0], "pdf_editable");
    assert_eq!(value["table_rendering"][2], "pdf_hybrid");

    let capabilities = binary().args(["capabilities", "--json"]).output().unwrap();
    assert!(capabilities.status.success());
    let capabilities: serde_json::Value = serde_json::from_slice(&capabilities.stdout).unwrap();
    assert_eq!(capabilities["formats"]["pdf"][7], "metadata");
    assert_eq!(capabilities["pdf_modes"][2], "hybrid");
    assert_eq!(capabilities["prepared_pdf"][1], "bookmarks");
    assert_eq!(capabilities["commands"].as_array().unwrap().len(), 12);
    assert_eq!(capabilities["output_contract"][1], "direct_stdout_writer");
    assert_eq!(capabilities["output_limit_bytes"], 536_870_912_u64);
    assert_eq!(capabilities["exit_codes"]["cancelled"], 130);
    assert_eq!(
        capabilities["mutation_contract"][1],
        "input_template_never_replaced"
    );

    let checked = binary()
        .arg("check")
        .arg(fixture())
        .arg("--json")
        .output()
        .unwrap();
    assert!(checked.status.success());
    let value: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["template"], "cli-test");
    assert_eq!(value["truncated"], false);

    let validated = binary()
        .arg("validate")
        .arg(fixture())
        .arg("--json")
        .output()
        .unwrap();
    assert!(validated.status.success());
    let value: serde_json::Value = serde_json::from_slice(&validated.stdout).unwrap();
    assert_eq!(value["truncated"], false);
}

#[test]
fn render_and_mask_write_atomic_artifacts() {
    let directory = temporary_directory();
    let svg = directory.join("scene.svg");
    let rendered = binary()
        .arg("render")
        .arg(fixture())
        .args(["--format", "svg", "--output"])
        .arg(&svg)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    assert!(std::fs::read(&svg).unwrap().starts_with(b"<svg"));

    let hybrid = directory.join("scene-hybrid.pdf");
    let rendered = binary()
        .arg("render")
        .arg(fixture())
        .args(["--format", "pdf", "--pdf-mode", "hybrid", "--output"])
        .arg(&hybrid)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    assert!(std::fs::read(&hybrid).unwrap().starts_with(b"%PDF-"));

    let mask = directory.join("mask.png");
    let masked = binary()
        .arg("mask")
        .arg(fixture())
        .args(["--format", "png", "--output"])
        .arg(&mask)
        .output()
        .unwrap();
    assert!(
        masked.status.success(),
        "{}",
        String::from_utf8_lossy(&masked.stderr)
    );
    assert!(std::fs::read(&mask).unwrap().starts_with(b"\x89PNG"));

    let debugged = binary()
        .arg("debug")
        .arg(fixture())
        .args(["--grid", "5", "--view", "layout", "--json"])
        .output()
        .unwrap();
    assert!(
        debugged.status.success(),
        "{}",
        String::from_utf8_lossy(&debugged.stderr)
    );
    let overlay: serde_json::Value = serde_json::from_slice(&debugged.stdout).unwrap();
    assert_eq!(overlay["page"], 0);
    assert!(overlay["primitives"].as_array().unwrap().len() > 4);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn runtime_patch_and_dataset_csv_execute_through_the_cli() {
    let directory = temporary_directory();
    let patch = directory.join("move.json");
    std::fs::write(
        &patch,
        br#"{"sequence":1,"operations":[{"op":"move","id":"box","x":"8pt","y":"9pt"}]}"#,
    )
    .unwrap();
    let inspected = binary()
        .arg("inspect")
        .arg(fixture())
        .args(["--id", "box", "--patch"])
        .arg(&patch)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let inspected: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(inspected["bounds"]["layout"]["origin"]["x"], 8_000_000);
    assert_eq!(inspected["bounds"]["layout"]["origin"]["y"], 9_000_000);
    assert_eq!(inspected["provenance"]["patches"][0], 1);

    let template = directory.join("table.yaml");
    std::fs::write(
        &template,
        b"filemaker: '1.0'\nmodel: dataset\nid: cli-csv\nelements:\n  - id: rows\n    type: table\n    binding: data.rows\n    table:\n      columns:\n        - { field: name, header: Name, width: { mode: fixed, value: 40pt } }\n      max_rows: 2\n      max_row_fields: 1\n      max_cell_bytes: 32\n",
    )
    .unwrap();
    let data = directory.join("data.json");
    std::fs::write(
        &data,
        br#"{"type":"object","value":{"rows":{"type":"array","value":[{"type":"object","value":{"name":{"type":"string","value":"Alpha"}}},{"type":"object","value":{"name":{"type":"string","value":"Beta"}}}]}}}"#,
    )
    .unwrap();
    let csv = directory.join("rows.csv");
    let rendered = binary()
        .arg("render")
        .arg(&template)
        .args(["--format", "csv", "--table", "rows", "--data"])
        .arg(&data)
        .arg("--output")
        .arg(&csv)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(csv).unwrap(),
        "Name\r\nAlpha\r\nBeta\r\n"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn diagnostics_offer_distinct_human_and_json_output() {
    for (command, options, human_prefix, json_key) in [
        ("debug", vec!["--grid", "10"], "debug page 0:", "primitives"),
        (
            "inspect",
            vec!["--id", "box"],
            "element box on page 0",
            "id",
        ),
        (
            "explain",
            vec!["--id", "box"],
            "element box on page 0:",
            "decisions",
        ),
    ] {
        let human = binary()
            .arg(command)
            .arg(fixture())
            .args(&options)
            .output()
            .unwrap();
        assert!(human.status.success());
        assert!(String::from_utf8_lossy(&human.stdout).starts_with(human_prefix));

        let json_output = binary()
            .arg(command)
            .arg(fixture())
            .args(&options)
            .arg("--json")
            .output()
            .unwrap();
        assert!(json_output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
        assert!(!value[json_key].is_null());
    }

    let free = binary()
        .arg("free-regions")
        .arg(fixture())
        .args([
            "--minimum-width",
            "1pt",
            "--minimum-height",
            "1pt",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(free.status.success());
    let free: serde_json::Value = serde_json::from_slice(&free.stdout).unwrap();
    assert!(!free.as_array().unwrap().is_empty());

    let preflight = binary()
        .arg("preflight")
        .arg(fixture())
        .args(["--format", "svg"])
        .output()
        .unwrap();
    assert!(preflight.status.success());
    assert!(String::from_utf8_lossy(&preflight.stdout).starts_with("preflight passed"));
    let preflight_json = binary()
        .arg("preflight")
        .arg(fixture())
        .args(["--format", "svg", "--json"])
        .output()
        .unwrap();
    assert!(preflight_json.status.success());
    let value: serde_json::Value = serde_json::from_slice(&preflight_json.stdout).unwrap();
    assert_eq!(value["ok"], true);

    let schema = binary().arg("schema").output().unwrap();
    assert!(String::from_utf8_lossy(&schema.stdout).starts_with("appcore-filemaker schema"));
    let capabilities = binary().arg("capabilities").output().unwrap();
    assert!(String::from_utf8_lossy(&capabilities.stdout).contains("pdf svg png"));
}

#[test]
fn artifact_output_cannot_replace_the_template() {
    let before = std::fs::read(fixture()).unwrap();
    for command in ["render", "mask"] {
        let format = if command == "render" { "svg" } else { "json" };
        let output = binary()
            .arg(command)
            .arg(fixture())
            .args(["--format", format, "--output"])
            .arg(fixture())
            .arg("--json")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(64));
        let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], "FM-CLI-USAGE");
        assert_eq!(std::fs::read(fixture()).unwrap(), before);
    }
}

#[test]
fn usage_data_and_missing_input_exit_codes_are_stable() {
    let usage = binary().arg("--json").output().unwrap();
    assert_eq!(usage.status.code(), Some(64));
    assert!(usage.stderr.ends_with(b"\n"));
    let usage: serde_json::Value = serde_json::from_slice(&usage.stderr).unwrap();
    assert_eq!(usage["code"], "FM-CLI-USAGE");

    let missing = binary()
        .args([
            "check",
            "definitely-missing-filemaker-template.yaml",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(66));
    let missing: serde_json::Value = serde_json::from_slice(&missing.stderr).unwrap();
    assert_eq!(missing["code"], "FM-CLI-NOINPUT");

    let directory = temporary_directory();
    let malformed = directory.join("malformed.yaml");
    std::fs::write(&malformed, b"filemaker: [").unwrap();
    let invalid = binary()
        .arg("check")
        .arg(&malformed)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(65));
    let invalid: serde_json::Value = serde_json::from_slice(&invalid.stderr).unwrap();
    assert_eq!(invalid["code"], "FM-SCHEMA-SYNTAX");
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn migrate_is_non_mutating_and_unavailable() {
    let before = std::fs::read(fixture()).unwrap();
    let output = binary()
        .arg("migrate")
        .arg(fixture())
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(69));
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "FM-CLI-UNAVAILABLE");
    assert_eq!(std::fs::read(fixture()).unwrap(), before);
}
