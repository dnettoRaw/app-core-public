// =============================================================================
//        #######
//     ###       ###     F: diagnostic.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded diagnostic contracts and behavior for this crate.

use std::path::Path;

use appcore_args::ParsedCli;
use appcore_filemaker::{ElementId, ErrorCode, FileMakerError, Length, SceneInspector, Size, Unit};

use crate::failure::{CliFailure, CliResult};
use crate::output::CliOutput;
use crate::pipeline::Pipeline;

pub(crate) fn inspect(
    pipeline: &Pipeline,
    template_path: &Path,
    parsed: &ParsedCli,
    json_output: bool,
) -> CliResult<CliOutput> {
    let compiled = pipeline.compile_scene(template_path)?;
    let inspector = SceneInspector::new(&compiled.scene);
    if let Some(id) = parsed.option_value("id") {
        let id = ElementId::new(id).map_err(|error| CliFailure::from_core(error, json_output))?;
        let inspection = inspector
            .inspect_element(&id)
            .map_err(|error| CliFailure::from_core(error, json_output))?;
        let human = format!(
            "element {} on page {} (layer {}, z-index {})",
            inspection.id.as_str(),
            inspection.page,
            inspection.layer,
            inspection.z_index
        );
        return response(inspection, human, json_output);
    }
    let inspection = inspector
        .inspect_page(page_index(parsed, json_output)?)
        .map_err(|error| CliFailure::from_core(error, json_output))?;
    let human = format!(
        "page {}: {} elements, {} overflow",
        inspection.page,
        inspection.elements,
        inspection.overflow.len()
    );
    response(inspection, human, json_output)
}

pub(crate) fn explain(
    pipeline: &Pipeline,
    template_path: &Path,
    parsed: &ParsedCli,
    json_output: bool,
) -> CliResult<CliOutput> {
    let compiled = pipeline.compile_scene(template_path)?;
    let id = ElementId::new(required_option(parsed, "id", json_output)?)
        .map_err(|error| CliFailure::from_core(error, json_output))?;
    let explanation = SceneInspector::new(&compiled.scene)
        .explain_layout(&id)
        .map_err(|error| CliFailure::from_core(error, json_output))?;
    let human = format!(
        "element {} on page {}: {}",
        explanation.id.as_str(),
        explanation.page,
        explanation.decisions.join("; ")
    );
    response(explanation, human, json_output)
}

pub(crate) fn free_regions(
    pipeline: &Pipeline,
    template_path: &Path,
    parsed: &ParsedCli,
    json_output: bool,
) -> CliResult<CliOutput> {
    let compiled = pipeline.compile_scene(template_path)?;
    let page = page_index(parsed, json_output)?;
    let page_size = compiled
        .scene
        .pages
        .get(page)
        .ok_or_else(|| {
            CliFailure::from_core(
                FileMakerError::new(ErrorCode::LayoutInvalid, "page index is outside the scene"),
                json_output,
            )
        })?
        .size;
    let width = resolved_length(parsed, "minimum-width", page_size.width, json_output)?;
    let height = resolved_length(parsed, "minimum-height", page_size.height, json_output)?;
    let free = SceneInspector::new(&compiled.scene)
        .query_free_regions_bounded(
            page,
            Size::new(width, height).map_err(|error| CliFailure::from_core(error, json_output))?,
            &pipeline.limits,
        )
        .map_err(|error| CliFailure::from_core(error, json_output))?;
    let human = format!("page {page}: {} free regions", free.len());
    response(free, human, json_output)
}

fn resolved_length(
    parsed: &ParsedCli,
    name: &str,
    reference: Unit,
    json_output: bool,
) -> CliResult<Unit> {
    let value = required_option(parsed, name, json_output)?
        .parse::<Length>()
        .map_err(|error| CliFailure::from_core(error, json_output))?
        .resolve(
            reference,
            Unit::points(1).map_err(|error| CliFailure::from_core(error, json_output))?,
        )
        .map_err(|error| CliFailure::from_core(error, json_output))?;
    value.ok_or_else(|| CliFailure::usage(format!("--{name} cannot be auto"), json_output))
}

fn required_option<'a>(parsed: &'a ParsedCli, name: &str, json: bool) -> CliResult<&'a str> {
    parsed
        .option_value(name)
        .ok_or_else(|| CliFailure::usage(format!("--{name} is required"), json))
}

fn page_index(parsed: &ParsedCli, json: bool) -> CliResult<usize> {
    parsed.option_value("page").map_or(Ok(0), |value| {
        value
            .parse::<usize>()
            .map_err(|_| CliFailure::usage("--page is outside platform range", json))
    })
}

fn response<T: serde::Serialize + 'static>(
    value: T,
    human: String,
    json: bool,
) -> CliResult<CliOutput> {
    Ok(CliOutput::response(value, human, json))
}
