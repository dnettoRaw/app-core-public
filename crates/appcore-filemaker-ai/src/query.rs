// =============================================================================
//        #######
//     ###       ###     F: query.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded query contracts and behavior for this crate.

use base64::Engine as _;
use serde_json::{json, Value};

use appcore_filemaker::{
    export, export_dataset_csv, preflight as core_preflight, validate_layout, BorrowedDataset,
    CollisionMask, DocumentIr, ElementId, ElementIr, ExportContext, ExportFormat, ExportRequest,
    Fidelity, HtmlMode, Length, MaskView, OperationControl, PdfMode, PreflightOptions,
    SceneInspector, Size, Unit,
};

use crate::error::json_error;
use crate::{BridgeError, BridgeResult, FileMakerAiSession};

pub(crate) fn capabilities(session: &FileMakerAiSession) -> BridgeResult<Value> {
    let document_context = session.document.as_ref().map(|document| {
        json!({
            "template": document.template_id,
            "model": document.model,
            "purpose": document.ai_policy.purpose,
            "rules": document.ai_policy.rules,
            "editable": document.ai_policy.editable,
            "locked": document.ai_policy.locked,
            "root_elements": document.elements.len(),
        })
    });
    Ok(json!({
        "schema_version": appcore_filemaker::FILEMAKER_SCHEMA_V1,
        "engine_version": appcore_filemaker::ENGINE_VERSION,
        "formats": ["pdf", "svg", "png", "jpeg", "html", "csv"],
        "pdf_modes": ["editable", "flattened", "hybrid"],
        "prepared_formats": ["webp", "xlsx", "zpl", "esc_pos", "pdf_a"],
        "prepared_pdf": ["links", "bookmarks", "tagged_accessibility", "pdf_a"],
        "mask_formats": ["json", "svg", "png", "pdf"],
        "limits": session.limits,
        "bridge_policy": session.policy,
        "calls_used": session.calls_used(),
        "calls_remaining": session.policy.max_tool_calls.saturating_sub(session.calls_used()),
        "revision": session.revision,
        "document_context": document_context,
        "loop": crate::recommended_tool_loop(),
    }))
}

pub(crate) fn schema() -> BridgeResult<Value> {
    let mut schema = json!({
        "frontend": "strict_yaml_1_0",
        "renderer_input": "resolved_scene",
        "models": ["document", "canvas", "dataset"],
        "coordinate_units": ["pt", "px", "mm", "cm", "in", "percent", "logical", "normalized_0_to_1"],
        "canvas_primitives": ["text", "image", "line", "rect", "circle", "ellipse", "polygon", "path", "group"],
        "path_commands": ["move", "line", "curve", "close"],
        "simple_add": ["element.type", "source_lengths", "style", "path", "transform", "layer", "collision"],
        "prepared_graphics": ["chart", "qr", "barcode", "clip", "mask", "gradient", "shadow", "blend"],
        "validation_stages": ["schema", "data", "layout", "preflight"],
        "validation_contract": ["warnings_first_class", "strict_rejects_warnings", "bounded_report", "truncation_fails_closed"],
        "fingerprint_inputs": ["schema_version", "engine_version", "template", "data", "patches", "assets", "fonts"],
        "cache_contract": ["immutable_scene", "resolve_on_miss", "insertion_bounded", "engine_version_checked"],
        "export_contract": ["writer", "bounded_bytes", "strict", "best_effort", "loss_report", "raster_dpi_only", "pdf_metadata", "pdf_font_subsets"],
        "debug_overlay": ["grid_1_5_10_20", "ruler", "coordinates", "ids", "bounds", "anchors", "regions", "safe_area", "collision", "crosshair", "non_mutating"],
        "mask_views": ["collision", "layout", "visual", "combined"],
        "mask_json": ["occupied", "free", "collisions", "overflow"],
        "inspect_trace": ["source_xywh", "anchors", "region", "measurement", "collision", "page_reflow", "provenance"],
        "collision_inheritance": ["document", "page", "region", "group", "element"],
        "transforms": ["translate", "rotate_integer_degrees", "scale_millionths", "flip", "mirror", "origin"],
        "text_overflow": ["wrap", "shrink", "ellipsis", "clip", "expand", "error"],
        "prepared_text_capabilities": ["color_emoji"],
        "color_sources": ["hex", "named", "rgb", "rgba", "gray", "cmyk", "typed"],
        "resolvers": ["asset_sandbox", "font_sandbox", "template_sandbox", "bounded_bytes"],
        "style_cascade": ["defaults", "theme", "template", "component", "data_rules", "runtime_override", "export_override"],
        "export_style_override": ["fill", "stroke", "opacity", "color", "paint_only"],
        "paint_order": ["layer", "z_index", "source_sequence", "collision_independent"],
        "image_fit": ["contain", "cover", "fill", "none", "scale_down"],
        "image_contract": ["crop", "focal_point", "aspect_preserved", "optional_exif", "svg", "raster", "effective_dpi"],
        "layout_constraints": ["min", "preferred", "max", "aspect_ratio_millionths", "align_x", "align_y"],
        "guide_anchor": "guide:name[+offset]",
        "flow_distribution": ["start", "center", "end", "space_between", "space_around", "space_evenly"],
        "exclusions": ["named", "non_painted", "page_relative", "collision_groups", "repeated_per_page", "bounded"],
        "page_layers": ["master", "first", "continuation", "last", "background", "header", "footer", "page_n_of_m"],
        "table_planning": ["fixed_columns", "bounded_auto_columns", "weighted_flex_columns", "measured_rows", "repeating_headers", "groups", "conditional_styles", "exact_totals", "streaming", "resolved_fragments"],
        "table_source": ["type_table", "array_binding", "typed_rows", "local_limits_only_tighten"],
        "table_rendering": ["pdf_editable", "pdf_flattened", "pdf_hybrid", "svg", "png", "jpeg", "html_semantic", "html_fixed"],
        "patch_operations": ["set_text", "set_hidden", "set_style", "move", "resize", "remove", "add", "clone", "replace"],
        "bridge_contract": ["exact_arguments", "small_atomic_patches", "document_ai_policy", "locked_subtrees", "bounded_context", "no_filesystem_output"],
        "unknown_fields": "rejected",
    });
    schema["writing_modes"] = json!(["horizontal", "vertical_rl"]);
    Ok(schema)
}

pub(crate) fn inspect(session: &FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let scene = session.resolve()?;
    let inspector = SceneInspector::new(&scene);
    if let Some(id) = args.get("id").and_then(Value::as_str) {
        to_value(&inspector.inspect_element(&ElementId::new(id)?)?)
    } else {
        to_value(&inspector.inspect_page(usize_field(args, "page", 0)?)?)
    }
}

pub(crate) fn explain(session: &FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let scene = session.resolve()?;
    let id = ElementId::new(string_field(args, "id")?)?;
    to_value(&SceneInspector::new(&scene).explain_layout(&id)?)
}

pub(crate) fn measure(session: &FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let scene = session.resolve()?;
    let id = ElementId::new(string_field(args, "id")?)?;
    let inspection = SceneInspector::new(&scene).inspect_element(&id)?;
    Ok(json!({
        "id": inspection.id,
        "page": inspection.page,
        "bounds": inspection.bounds,
    }))
}

pub(crate) fn validate(session: &FileMakerAiSession) -> BridgeResult<Value> {
    if session.document()?.model == appcore_filemaker::ModelKind::Dataset {
        session.validate_document(session.document()?)?;
        return Ok(json!({
            "valid": true,
            "template": session.document()?.template_id,
            "pages": 0,
            "issues": [],
            "truncated": false,
        }));
    }
    let scene = session.resolve()?;
    let report = validate_layout(
        &scene,
        &session.limits,
        PreflightOptions::default().max_issues,
        &OperationControl::default(),
    )?;
    Ok(json!({
        "valid": !report.has_errors() && !report.truncated,
        "template": scene.template_id,
        "pages": scene.pages.len(),
        "issues": report.issues,
        "truncated": report.truncated,
    }))
}

pub(crate) fn preflight(session: &FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let scene = session.resolve()?;
    let request = export_request(args)?;
    let options = PreflightOptions {
        strict: bool_field(args, "strict", false)?,
        require_accessibility: bool_field(args, "require_accessibility", false)?,
        ..PreflightOptions::default()
    };
    let report = core_preflight(
        &scene,
        &request,
        &session.export_context(),
        &options,
        &OperationControl::default(),
    )?;
    to_value(&report)
}

pub(crate) fn preview(session: &FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    ensure_artifacts_allowed(session)?;
    let mut request = ExportRequest {
        format: ExportFormat::Png,
        fidelity: Fidelity::BestEffort,
        dpi: u32_field(args, "dpi", 96)?,
        ..ExportRequest::default()
    };
    request.page = Some(usize_field(args, "page", 0)?);
    encode_artifact(session, &request, "image/png")
}

pub(crate) fn debug_mask(session: &FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let scene = session.resolve()?;
    let page = usize_field(args, "page", 0)?;
    let view = match args
        .get("view")
        .and_then(Value::as_str)
        .unwrap_or("collision")
    {
        "collision" => MaskView::CollisionMask,
        "layout" => MaskView::LayoutBounds,
        "visual" => MaskView::VisualBounds,
        "combined" => MaskView::Combined,
        _ => return Err(BridgeError::InvalidInput("unsupported mask view")),
    };
    to_value(&CollisionMask::derive_bounded(
        &scene,
        page,
        view,
        &session.limits,
    )?)
}

pub(crate) fn free_regions(session: &FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let scene = session.resolve()?;
    let page = usize_field(args, "page", 0)?;
    let page_ref = scene
        .pages
        .get(page)
        .ok_or(BridgeError::InvalidInput("page"))?;
    let width = length_field(args, "minimum_width", Unit::points(1)?)?
        .resolve(page_ref.size.width, Unit::points(1)?)?
        .ok_or(BridgeError::InvalidInput("minimum_width cannot be auto"))?;
    let height = length_field(args, "minimum_height", Unit::points(1)?)?
        .resolve(page_ref.size.height, Unit::points(1)?)?
        .ok_or(BridgeError::InvalidInput("minimum_height cannot be auto"))?;
    let free = SceneInspector::new(&scene).query_free_regions_bounded(
        page,
        Size::new(width, height)?,
        &session.limits,
    )?;
    to_value(&free)
}

pub(crate) fn export_artifact(session: &FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    ensure_artifacts_allowed(session)?;
    if args.get("format").and_then(Value::as_str) == Some("csv") {
        if args.as_object().is_some_and(|object| {
            object
                .keys()
                .any(|key| !matches!(key.as_str(), "format" | "table"))
        }) {
            return Err(BridgeError::InvalidInput(
                "CSV export accepts only format and table",
            ));
        }
        return encode_csv(session, args);
    }
    let request = export_request(args)?;
    let media_type = match request.format {
        ExportFormat::Pdf => "application/pdf",
        ExportFormat::Svg => "image/svg+xml",
        ExportFormat::Png => "image/png",
        ExportFormat::Jpeg => "image/jpeg",
        ExportFormat::Html => "text/html",
    };
    encode_artifact(session, &request, media_type)
}

fn encode_csv(session: &FileMakerAiSession, args: &Value) -> BridgeResult<Value> {
    let element = select_table(
        session.document()?,
        args.get("table").and_then(Value::as_str),
    )?;
    let table = element
        .table
        .as_ref()
        .ok_or(BridgeError::InvalidInput("selected table"))?;
    let raw_limit = session.policy.max_result_bytes.saturating_mul(3) / 4;
    let mut limits = session.limits.clone();
    limits.max_output_bytes = limits.max_output_bytes.min(raw_limit.max(1));
    let dataset = BorrowedDataset::new(&table.rows);
    let mut bytes = Vec::new();
    let outcome = export_dataset_csv(&table.spec, &dataset, &limits, &mut bytes)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(json!({
        "media_type": "text/csv; charset=utf-8",
        "table": element.id,
        "bytes": bytes.len(),
        "base64": encoded,
        "loss_report": outcome.loss_report,
    }))
}

fn select_table<'a>(
    document: &'a DocumentIr,
    requested: Option<&str>,
) -> BridgeResult<&'a ElementIr> {
    let mut tables = Vec::new();
    let mut stack: Vec<&ElementIr> = document.elements.iter().rev().collect();
    while let Some(element) = stack.pop() {
        if element.table.is_some() {
            tables.push(element);
        }
        stack.extend(element.children.iter().rev());
    }
    if let Some(requested) = requested {
        return tables
            .into_iter()
            .find(|element| element.id.as_str() == requested)
            .ok_or(BridgeError::InvalidInput("requested CSV table"));
    }
    if tables.len() != 1 {
        return Err(BridgeError::InvalidInput(
            "CSV export requires one table or an explicit table argument",
        ));
    }
    Ok(tables[0])
}

fn encode_artifact(
    session: &FileMakerAiSession,
    request: &ExportRequest,
    media_type: &str,
) -> BridgeResult<Value> {
    let scene = session.resolve()?;
    let mut limits = session.limits.clone();
    let raw_limit = session.policy.max_result_bytes.saturating_mul(3) / 4;
    limits.max_output_bytes = limits.max_output_bytes.min(raw_limit.max(1));
    limits.max_pixels = limits
        .max_pixels
        .min(u64::try_from(raw_limit / 4).unwrap_or(1).max(1));
    let context = ExportContext {
        limits: &limits,
        fonts: &session.fonts,
        assets: session.assets.as_deref(),
    };
    let mut bytes = Vec::new();
    let outcome = export(&scene, request, &context, &mut bytes)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(json!({
        "media_type": media_type,
        "bytes": bytes.len(),
        "base64": encoded,
        "loss_report": outcome.loss_report,
    }))
}

fn export_request(args: &Value) -> BridgeResult<ExportRequest> {
    let format = match args.get("format").and_then(Value::as_str).unwrap_or("svg") {
        "pdf" => ExportFormat::Pdf,
        "svg" => ExportFormat::Svg,
        "png" => ExportFormat::Png,
        "jpeg" => ExportFormat::Jpeg,
        "html" => ExportFormat::Html,
        _ => return Err(BridgeError::InvalidInput("unsupported export format")),
    };
    let fidelity = match args
        .get("fidelity")
        .and_then(Value::as_str)
        .unwrap_or("strict")
    {
        "strict" => Fidelity::Strict,
        "best_effort" => Fidelity::BestEffort,
        _ => return Err(BridgeError::InvalidInput("unsupported fidelity")),
    };
    let pdf_mode = match args
        .get("pdf_mode")
        .and_then(Value::as_str)
        .unwrap_or("editable")
    {
        "editable" => PdfMode::Editable,
        "flattened" => PdfMode::Flattened,
        "hybrid" => PdfMode::Hybrid,
        _ => return Err(BridgeError::InvalidInput("unsupported PDF mode")),
    };
    let html_mode = match args
        .get("html_mode")
        .and_then(Value::as_str)
        .unwrap_or("semantic")
    {
        "semantic" => HtmlMode::Semantic,
        "fixed" => HtmlMode::Fixed,
        _ => return Err(BridgeError::InvalidInput("unsupported HTML mode")),
    };
    Ok(ExportRequest {
        format,
        fidelity,
        page: args
            .get("page")
            .map(|_| usize_field(args, "page", 0))
            .transpose()?,
        dpi: u32_field(args, "dpi", 144)?,
        jpeg_quality: u8_field(args, "jpeg_quality", 90)?,
        pdf_mode,
        html_mode,
        style_override: args
            .get("style_override")
            .map(|value| serde_json::from_value(value.clone()).map_err(json_error))
            .transpose()?,
    })
}

fn ensure_artifacts_allowed(session: &FileMakerAiSession) -> BridgeResult<()> {
    if session.policy.allow_artifact_bytes {
        Ok(())
    } else {
        Err(BridgeError::Policy(
            "artifact bytes are disabled for this session".to_owned(),
        ))
    }
}

fn string_field(args: &Value, name: &'static str) -> BridgeResult<String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or(BridgeError::InvalidInput(name))
}

fn usize_field(args: &Value, name: &'static str, default: usize) -> BridgeResult<usize> {
    args.get(name).map_or(Ok(default), |value| {
        value
            .as_u64()
            .and_then(|number| usize::try_from(number).ok())
            .ok_or(BridgeError::InvalidInput(name))
    })
}

fn u32_field(args: &Value, name: &'static str, default: u32) -> BridgeResult<u32> {
    args.get(name).map_or(Ok(default), |value| {
        value
            .as_u64()
            .and_then(|number| u32::try_from(number).ok())
            .ok_or(BridgeError::InvalidInput(name))
    })
}

fn u8_field(args: &Value, name: &'static str, default: u8) -> BridgeResult<u8> {
    args.get(name).map_or(Ok(default), |value| {
        value
            .as_u64()
            .and_then(|number| u8::try_from(number).ok())
            .ok_or(BridgeError::InvalidInput(name))
    })
}

fn bool_field(args: &Value, name: &'static str, default: bool) -> BridgeResult<bool> {
    args.get(name).map_or(Ok(default), |value| {
        value.as_bool().ok_or(BridgeError::InvalidInput(name))
    })
}

fn length_field(args: &Value, name: &'static str, default: Unit) -> BridgeResult<Length> {
    args.get(name)
        .map_or(Ok(Length::Absolute(default)), |value| {
            serde_json::from_value(value.clone()).map_err(json_error)
        })
}

fn to_value<T: serde::Serialize>(value: &T) -> BridgeResult<Value> {
    serde_json::to_value(value).map_err(json_error)
}
