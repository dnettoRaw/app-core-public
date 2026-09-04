// =============================================================================
//        #######
//     ###       ###     F: introspection.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded introspection contracts and behavior for this crate.

use serde_json::json;

use crate::failure::CliResult;
use crate::output::CliOutput;

pub(crate) fn schema(json_output: bool) -> CliResult<CliOutput> {
    let mut schema = json!({
        "schema": "appcore-filemaker-template",
        "version": appcore_filemaker::FILEMAKER_SCHEMA_V1,
        "models": ["document", "canvas", "dataset"],
        "coordinate_units": ["pt", "px", "mm", "cm", "in", "percent", "logical", "normalized_0_to_1"],
        "canvas_primitives": ["text", "image", "line", "rect", "circle", "ellipse", "polygon", "path", "group"],
        "path_commands": ["move", "line", "curve", "close"],
        "prepared_graphics": ["chart", "qr", "barcode", "clip", "mask", "gradient", "shadow", "blend"],
        "export_contract": ["writer", "bounded_bytes", "strict", "best_effort", "loss_report", "raster_dpi_only", "pdf_metadata", "pdf_font_subsets"],
        "debug_overlay": ["grid_1_5_10_20", "ruler", "coordinates", "ids", "bounds", "anchors", "regions", "safe_area", "collision", "crosshair", "non_mutating"],
        "mask_views": ["collision", "layout", "visual", "combined"],
        "mask_json": ["occupied", "free", "collisions", "overflow"],
        "inspect_trace": ["source_xywh", "anchors", "region", "measurement", "collision", "page_reflow", "provenance"],
        "unknown_fields": "rejected",
        "frontend": "yaml",
        "renderer_input": "resolved_scene",
        "collision_inheritance": ["document", "page", "region", "group", "element"],
        "transforms": ["translate", "rotate_integer_degrees", "scale_millionths", "flip", "mirror", "origin"],
        "text_overflow": ["wrap", "shrink", "ellipsis", "clip", "expand", "error"],
        "writing_modes": ["horizontal", "vertical_rl"],
        "prepared_text_capabilities": ["color_emoji"],
        "color_sources": ["hex", "named", "rgb", "rgba", "gray", "cmyk", "typed"],
        "resolvers": ["asset_sandbox", "font_sandbox", "template_sandbox", "bounded_bytes"],
        "style_cascade": ["defaults", "theme", "template", "component", "data_rules", "runtime_override", "export_override"],
        "export_style_override": ["fill", "stroke", "opacity", "color", "paint_only"],
        "paint_order": ["layer", "z_index", "source_sequence", "collision_independent"],
        "image_fit": ["contain", "cover", "fill", "none", "scale_down"],
        "image_contract": ["crop", "focal_point", "aspect_preserved", "optional_exif", "svg", "raster", "effective_dpi"],
        "patch_operations": ["set_text", "set_hidden", "set_style", "move", "resize", "remove", "add", "clone", "replace"],
        "layout_constraints": ["min", "preferred", "max", "aspect_ratio_millionths", "align_x", "align_y"],
        "guide_anchor": "guide:name[+offset]",
        "flow_distribution": ["start", "center", "end", "space_between", "space_around", "space_evenly"],
        "exclusions": ["named", "non_painted", "page_relative", "collision_groups", "repeated_per_page", "bounded"],
        "page_layers": ["master", "first", "continuation", "last", "background", "header", "footer", "page_n_of_m"],
        "table_planning": ["fixed_columns", "bounded_auto_columns", "weighted_flex_columns", "measured_rows", "repeating_headers", "groups", "conditional_styles", "exact_totals", "streaming", "resolved_fragments"],
        "table_source": ["type_table", "array_binding", "typed_rows", "local_limits_only_tighten"],
        "table_rendering": ["pdf_editable", "pdf_flattened", "pdf_hybrid", "svg", "png", "jpeg", "html_semantic", "html_fixed"]
    });
    schema["validation_stages"] = json!(["schema", "data", "layout", "preflight"]);
    schema["validation_contract"] = json!([
        "warnings_first_class",
        "strict_rejects_warnings",
        "bounded_report",
        "truncation_fails_closed"
    ]);
    schema["fingerprint_inputs"] = json!([
        "schema_version",
        "engine_version",
        "template",
        "data",
        "patches",
        "assets",
        "fonts"
    ]);
    schema["cache_contract"] = json!([
        "immutable_scene",
        "resolve_on_miss",
        "insertion_bounded",
        "engine_version_checked"
    ]);
    response(
        schema,
        format!(
            "appcore-filemaker schema {}",
            appcore_filemaker::FILEMAKER_SCHEMA_V1
        ),
        json_output,
    )
}

pub(crate) fn capabilities(json_output: bool) -> CliResult<CliOutput> {
    response(
        json!({
            "formats": {
                "pdf": ["multi_page", "editable_text", "embedded_fonts", "vector", "transparency", "cmyk", "images", "metadata"],
                "svg": ["multi_page", "editable_text", "embedded_fonts", "vector", "transparency", "images"],
                "png": ["multi_page", "raster", "transparency", "images"],
                "jpeg": ["multi_page", "raster", "images"],
                "html": ["multi_page", "editable_text", "semantic", "images"],
                "csv": ["streaming_dataset"]
            },
            "pdf_modes": ["editable", "flattened", "hybrid"],
            "prepared_formats": ["webp", "xlsx", "zpl", "esc_pos", "pdf_a"],
            "prepared_pdf": ["links", "bookmarks", "tagged_accessibility", "pdf_a"],
            "commands": ["check", "validate", "render", "debug", "mask", "inspect", "explain", "free-regions", "preflight", "schema", "capabilities", "migrate"],
            "output_modes": ["human", "json"],
            "output_contract": ["bounded_sizing", "direct_stdout_writer", "stable_pretty_json", "newline_terminated"],
            "output_limit_bytes": crate::output::MAX_CLI_OUTPUT_BYTES,
            "exit_codes": {
                "success": 0, "validation": 2, "usage": 64, "data": 65, "no_input": 66,
                "unavailable": 69, "software": 70, "cannot_create": 73, "io": 74,
                "temporary_failure": 75, "cancelled": 130
            },
            "mutation_contract": ["atomic_artifacts", "input_template_never_replaced", "migrate_unavailable"],
            "reserved": ["migrate"]
        }),
        "pdf svg png jpeg html csv".to_owned(),
        json_output,
    )
}

fn response(value: serde_json::Value, human: String, json_output: bool) -> CliResult<CliOutput> {
    Ok(CliOutput::response(value, human, json_output))
}
