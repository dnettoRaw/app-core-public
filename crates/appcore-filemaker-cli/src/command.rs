// =============================================================================
//        #######
//     ###       ###     F: command.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded command contracts and behavior for this crate.

use std::path::Path;

use appcore_args::ParsedCli;
use appcore_filemaker::{
    export, export_collision_mask, export_dataset_csv, preflight, validate_layout,
    validate_template, BorrowedDataset, CollisionMask, DebugOverlay, DebugOverlayOptions,
    DocumentIr, ElementIr, ErrorCode, ExportFormat, ExportRequest, Fidelity, FileMakerError,
    HtmlMode, MaskFormat, MaskView, OperationControl, PdfMode, PreflightOptions, Unit,
};
use serde_json::json;

use crate::failure::{unavailable, CliFailure, CliResult};
use crate::io::{atomic_write, ensure_distinct_output};
use crate::output::CliOutput;
use crate::pipeline::Pipeline;

pub(crate) fn execute(parsed: &ParsedCli, json_output: bool) -> CliResult<CliOutput> {
    let command = parsed
        .command_path()
        .first()
        .map(String::as_str)
        .ok_or_else(|| CliFailure::usage("a command is required", json_output))?;
    match command {
        "schema" => crate::introspection::schema(json_output),
        "capabilities" => crate::introspection::capabilities(json_output),
        "migrate" => Err(unavailable(
            "schema 1.0 has no migration contract; input was not modified",
            json_output,
        )),
        _ => execute_pipeline(command, parsed, json_output),
    }
}

fn execute_pipeline(command: &str, parsed: &ParsedCli, json_output: bool) -> CliResult<CliOutput> {
    let pipeline = Pipeline::from_cli(parsed, json_output)?;
    let template_path = Path::new(
        parsed
            .positionals()
            .first()
            .ok_or_else(|| CliFailure::usage("template path is required", json_output))?,
    );
    match command {
        "check" => check(&pipeline, template_path, parsed, json_output),
        "validate" => validate(&pipeline, template_path, parsed, json_output),
        "render" => render(&pipeline, template_path, parsed, json_output),
        "debug" => debug(&pipeline, template_path, parsed, json_output),
        "mask" => mask(&pipeline, template_path, parsed, json_output),
        "inspect" => crate::diagnostic::inspect(&pipeline, template_path, parsed, json_output),
        "explain" => crate::diagnostic::explain(&pipeline, template_path, parsed, json_output),
        "free-regions" => {
            crate::diagnostic::free_regions(&pipeline, template_path, parsed, json_output)
        }
        "preflight" => run_preflight(&pipeline, template_path, parsed, json_output),
        _ => Err(CliFailure::usage("unknown command", json_output)),
    }
}

fn check(
    pipeline: &Pipeline,
    template_path: &Path,
    parsed: &ParsedCli,
    json_output: bool,
) -> CliResult<CliOutput> {
    let template = pipeline.compile_template(template_path)?;
    let report = validate_template(&template, &pipeline.limits);
    report
        .enforce(parsed.has_flag("strict"))
        .map_err(|error| CliFailure::from_core(error, json_output))?;
    response(
        json!({"ok": true, "template": template.id, "issues": report.issues, "truncated": report.truncated}),
        format!("OK {} ({} issues)", template.id, report.issues.len()),
        json_output,
    )
}

fn validate(
    pipeline: &Pipeline,
    template_path: &Path,
    parsed: &ParsedCli,
    json_output: bool,
) -> CliResult<CliOutput> {
    let compiled = pipeline.compile_scene(template_path)?;
    let report = validate_layout(
        &compiled.scene,
        &pipeline.limits,
        PreflightOptions::default().max_issues,
        &OperationControl::default(),
    )
    .map_err(|error| CliFailure::from_core(error, json_output))?;
    report
        .enforce(parsed.has_flag("strict"))
        .map_err(|error| CliFailure::from_core(error, json_output))?;
    response(
        json!({"ok": true, "template": compiled.template.id, "pages": compiled.scene.pages.len(), "issues": report.issues, "truncated": report.truncated}),
        format!(
            "OK {} ({} pages, {} issues)",
            compiled.template.id,
            compiled.scene.pages.len(),
            report.issues.len()
        ),
        json_output,
    )
}

fn render(
    pipeline: &Pipeline,
    template_path: &Path,
    parsed: &ParsedCli,
    json_output: bool,
) -> CliResult<CliOutput> {
    let output = required_option(parsed, "output", json_output)?;
    ensure_distinct_output(template_path, Path::new(output), json_output)?;
    if required_option(parsed, "format", json_output)? == "csv" {
        if ["page", "dpi", "jpeg-quality", "pdf-mode", "html-mode"]
            .iter()
            .any(|name| parsed.option_value(name).is_some())
            || parsed.has_flag("best-effort")
        {
            return Err(CliFailure::usage(
                "CSV output accepts --table but no graphical exporter options",
                json_output,
            ));
        }
        return render_csv(pipeline, template_path, parsed, output, json_output);
    }
    if parsed.option_value("table").is_some() {
        return Err(CliFailure::usage(
            "--table is valid only for CSV output",
            json_output,
        ));
    }
    let compiled = pipeline.compile_scene(template_path)?;
    let request = export_request(parsed, json_output)?;
    let mut bytes = Vec::new();
    let outcome = export(
        &compiled.scene,
        &request,
        &pipeline.export_context(),
        &mut bytes,
    )
    .map_err(|error| CliFailure::from_core(error, json_output))?;
    atomic_write(Path::new(output), &bytes, json_output)?;
    response(
        json!({"ok": true, "output": output, "bytes_written": outcome.bytes_written, "loss_report": outcome.loss_report, "capabilities": outcome.capabilities}),
        format!("wrote {} bytes to {output}", outcome.bytes_written),
        json_output,
    )
}

fn render_csv(
    pipeline: &Pipeline,
    template_path: &Path,
    parsed: &ParsedCli,
    output: &str,
    json_output: bool,
) -> CliResult<CliOutput> {
    let compiled = pipeline.compile_document(template_path)?;
    let element = select_table(
        &compiled.document,
        parsed.option_value("table"),
        json_output,
    )?;
    let table = element.table.as_ref().ok_or_else(|| {
        CliFailure::from_core(
            FileMakerError::new(ErrorCode::Validation, "selected element is not a table"),
            json_output,
        )
    })?;
    let dataset = BorrowedDataset::new(&table.rows);
    let mut bytes = Vec::new();
    let outcome = export_dataset_csv(&table.spec, &dataset, &pipeline.limits, &mut bytes)
        .map_err(|error| CliFailure::from_core(error, json_output))?;
    atomic_write(Path::new(output), &bytes, json_output)?;
    response(
        json!({
            "ok": true,
            "template": compiled.template.id,
            "table": element.id,
            "output": output,
            "bytes_written": outcome.bytes_written,
            "loss_report": outcome.loss_report,
            "capabilities": outcome.capabilities,
        }),
        format!("wrote {} CSV bytes to {output}", outcome.bytes_written),
        json_output,
    )
}

fn select_table<'a>(
    document: &'a DocumentIr,
    requested: Option<&str>,
    json_output: bool,
) -> CliResult<&'a ElementIr> {
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
            .ok_or_else(|| {
                CliFailure::from_core(
                    FileMakerError::new(ErrorCode::Validation, "requested CSV table was not found"),
                    json_output,
                )
            });
    }
    if tables.len() != 1 {
        return Err(CliFailure::from_core(
            FileMakerError::new(
                ErrorCode::Validation,
                "CSV output requires exactly one table or an explicit --table",
            ),
            json_output,
        ));
    }
    Ok(tables[0])
}

fn debug(
    pipeline: &Pipeline,
    template_path: &Path,
    parsed: &ParsedCli,
    json_output: bool,
) -> CliResult<CliOutput> {
    let compiled = pipeline.compile_scene(template_path)?;
    let page = page_index(parsed, json_output)?;
    let grid = parsed
        .option_value("grid")
        .unwrap_or("10")
        .parse::<i64>()
        .map_err(|_| CliFailure::usage("invalid debug grid", json_output))?;
    let view = mask_view(
        parsed.option_value("view").unwrap_or("combined"),
        json_output,
    )?;
    let overlay = DebugOverlay::build_bounded(
        &compiled.scene,
        page,
        &DebugOverlayOptions {
            grid: Some(
                Unit::points(grid).map_err(|error| CliFailure::from_core(error, json_output))?,
            ),
            ruler: true,
            ids: true,
            coordinates: true,
            bounds: true,
            anchors: true,
            regions: true,
            safe_area: true,
            collision: true,
            crosshair: true,
            view,
        },
        &pipeline.limits,
    )
    .map_err(|error| CliFailure::from_core(error, json_output))?;
    let human = format!(
        "debug page {}: {} primitives",
        overlay.page,
        overlay.primitives.len()
    );
    diagnostic_response(overlay, human, json_output)
}

fn mask(
    pipeline: &Pipeline,
    template_path: &Path,
    parsed: &ParsedCli,
    json_output: bool,
) -> CliResult<CliOutput> {
    let output = required_option(parsed, "output", json_output)?;
    ensure_distinct_output(template_path, Path::new(output), json_output)?;
    let compiled = pipeline.compile_scene(template_path)?;
    let page = page_index(parsed, json_output)?;
    let view = mask_view(
        parsed.option_value("view").unwrap_or("collision"),
        json_output,
    )?;
    let format = match required_option(parsed, "format", json_output)? {
        "json" => MaskFormat::Json,
        "svg" => MaskFormat::Svg,
        "png" => MaskFormat::Png,
        "pdf" => MaskFormat::Pdf,
        _ => return Err(CliFailure::usage("invalid mask format", json_output)),
    };
    let mask = CollisionMask::derive_bounded(&compiled.scene, page, view, &pipeline.limits)
        .map_err(|error| CliFailure::from_core(error, json_output))?;
    let mut bytes = Vec::new();
    let written = export_collision_mask(
        &mask,
        format,
        dpi(parsed, json_output)?,
        &pipeline.limits,
        &mut bytes,
    )
    .map_err(|error| CliFailure::from_core(error, json_output))?;
    atomic_write(Path::new(output), &bytes, json_output)?;
    response(
        json!({"ok": true, "output": output, "bytes_written": written}),
        format!("wrote {written} bytes to {output}"),
        json_output,
    )
}

fn mask_view(value: &str, json_output: bool) -> CliResult<MaskView> {
    Ok(match value {
        "collision" => MaskView::CollisionMask,
        "layout" => MaskView::LayoutBounds,
        "visual" => MaskView::VisualBounds,
        "combined" => MaskView::Combined,
        _ => return Err(CliFailure::usage("invalid mask view", json_output)),
    })
}

fn run_preflight(
    pipeline: &Pipeline,
    template_path: &Path,
    parsed: &ParsedCli,
    json_output: bool,
) -> CliResult<CliOutput> {
    let compiled = pipeline.compile_scene(template_path)?;
    let request = export_request(parsed, json_output)?;
    let report = preflight(
        &compiled.scene,
        &request,
        &pipeline.export_context(),
        &PreflightOptions {
            strict: parsed.has_flag("strict"),
            require_accessibility: parsed.has_flag("require-accessibility"),
            ..PreflightOptions::default()
        },
        &OperationControl::default(),
    )
    .map_err(|error| CliFailure::from_core(error, json_output))?;
    response(
        json!({"ok": true, "issues": report.issues, "truncated": report.truncated}),
        format!("preflight passed with {} issues", report.issues.len()),
        json_output,
    )
}

fn export_request(parsed: &ParsedCli, json_output: bool) -> CliResult<ExportRequest> {
    let format = match required_option(parsed, "format", json_output)? {
        "pdf" => ExportFormat::Pdf,
        "svg" => ExportFormat::Svg,
        "png" => ExportFormat::Png,
        "jpeg" => ExportFormat::Jpeg,
        "html" => ExportFormat::Html,
        _ => return Err(CliFailure::usage("invalid export format", json_output)),
    };
    let pdf_mode = match parsed.option_value("pdf-mode").unwrap_or("editable") {
        "editable" => PdfMode::Editable,
        "flattened" => PdfMode::Flattened,
        "hybrid" => PdfMode::Hybrid,
        _ => return Err(CliFailure::usage("invalid PDF mode", json_output)),
    };
    let html_mode = match parsed.option_value("html-mode").unwrap_or("semantic") {
        "semantic" => HtmlMode::Semantic,
        "fixed" => HtmlMode::Fixed,
        _ => return Err(CliFailure::usage("invalid HTML mode", json_output)),
    };
    Ok(ExportRequest {
        format,
        fidelity: if parsed.has_flag("best-effort") {
            Fidelity::BestEffort
        } else {
            Fidelity::Strict
        },
        dpi: dpi(parsed, json_output)?,
        page: parsed
            .option_value("page")
            .map(|_| page_index(parsed, json_output))
            .transpose()?,
        jpeg_quality: parsed
            .option_value("jpeg-quality")
            .map_or(Ok(90), |value| {
                value
                    .parse::<u8>()
                    .map_err(|_| CliFailure::usage("invalid JPEG quality", json_output))
            })?,
        pdf_mode,
        html_mode,
        ..ExportRequest::default()
    })
}

fn response(value: serde_json::Value, human: String, json_output: bool) -> CliResult<CliOutput> {
    Ok(CliOutput::response(value, human, json_output))
}

fn diagnostic_response<T>(value: T, human: String, json_output: bool) -> CliResult<CliOutput>
where
    T: serde::Serialize + 'static,
{
    Ok(CliOutput::response(value, human, json_output))
}

fn required_option<'a>(parsed: &'a ParsedCli, name: &str, json_output: bool) -> CliResult<&'a str> {
    parsed
        .option_value(name)
        .ok_or_else(|| CliFailure::usage(format!("--{name} is required"), json_output))
}

fn page_index(parsed: &ParsedCli, json_output: bool) -> CliResult<usize> {
    parsed.option_value("page").map_or(Ok(0), |value| {
        value
            .parse::<usize>()
            .map_err(|_| CliFailure::usage("--page is outside platform range", json_output))
    })
}

fn dpi(parsed: &ParsedCli, json_output: bool) -> CliResult<u32> {
    parsed.option_value("dpi").map_or(Ok(144), |value| {
        value
            .parse::<u32>()
            .map_err(|_| CliFailure::usage("--dpi is outside u32 range", json_output))
    })
}
