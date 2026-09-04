// =============================================================================
//        #######
//     ###       ###     F: pipeline.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded pipeline contracts and behavior for this crate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use appcore_args::ParsedCli;
use appcore_filemaker::{
    Compiler, DataValue, DocumentIr, ExportContext, FileResolver, FontAsset, FontManager,
    LayoutEngine, LayoutOptions, Patch, ResolvedScene, ResourceLimits, TemplateIr,
};

use crate::failure::{CliFailure, CliResult, EXIT_DATA};
use crate::io::read_bounded;

pub(crate) struct Pipeline {
    pub(crate) limits: ResourceLimits,
    pub(crate) fonts: FontManager,
    pub(crate) assets: Option<FileResolver>,
    compiler: Compiler,
    data_path: Option<PathBuf>,
    patches: Vec<Patch>,
    json: bool,
}

pub(crate) struct Compiled {
    pub(crate) template: TemplateIr,
    pub(crate) scene: ResolvedScene,
}

pub(crate) struct CompiledDocument {
    pub(crate) template: TemplateIr,
    pub(crate) document: DocumentIr,
}

impl Pipeline {
    pub(crate) fn from_cli(parsed: &ParsedCli, json: bool) -> CliResult<Self> {
        let limits = ResourceLimits::default();
        let assets = parsed
            .option_value("assets-root")
            .map(FileResolver::new)
            .transpose()
            .map_err(|error| CliFailure::from_core(error, json))?;
        let mut builder = Compiler::builder().limits(limits.clone());
        if let Some(resolver) = assets.clone() {
            builder = builder.template_resolver(Arc::new(resolver));
        }
        let compiler = builder
            .build()
            .map_err(|error| CliFailure::from_core(error, json))?;
        let mut fonts = FontManager::default();
        for declaration in parsed.option_values("font") {
            register_font(&mut fonts, declaration, &limits, json)?;
        }
        fonts
            .set_fallback(
                parsed
                    .option_values("font-fallback")
                    .map(ToOwned::to_owned)
                    .collect(),
            )
            .map_err(|error| CliFailure::from_core(error, json))?;
        let patches = load_patches(parsed, &limits, json)?;
        Ok(Self {
            limits,
            fonts,
            assets,
            compiler,
            data_path: parsed.option_value("data").map(PathBuf::from),
            patches,
            json,
        })
    }

    pub(crate) fn compile_template(&self, path: &Path) -> CliResult<TemplateIr> {
        let bytes = read_bounded(path, self.limits.max_template_bytes, self.json)?;
        self.compiler
            .compile_template_yaml(&bytes)
            .map_err(|error| CliFailure::from_core(error, self.json))
    }

    pub(crate) fn compile_scene(&self, path: &Path) -> CliResult<Compiled> {
        let CompiledDocument { template, document } = self.compile_document(path)?;
        let engine = LayoutEngine::new(&self.limits, &self.fonts, LayoutOptions::default())
            .map_err(|error| CliFailure::from_core(error, self.json))?;
        let scene = if let Some(assets) = &self.assets {
            engine.with_assets(assets).resolve(&document)
        } else {
            engine.resolve(&document)
        }
        .map_err(|error| CliFailure::from_core(error, self.json))?;
        Ok(Compiled { template, scene })
    }

    pub(crate) fn compile_document(&self, path: &Path) -> CliResult<CompiledDocument> {
        let template = self.compile_template(path)?;
        let data = self.load_data()?;
        let document = self
            .compiler
            .bind(&template, &data, &self.patches)
            .map_err(|error| CliFailure::from_core(error, self.json))?;
        Ok(CompiledDocument { template, document })
    }

    pub(crate) fn export_context(&self) -> ExportContext<'_> {
        ExportContext {
            limits: &self.limits,
            fonts: &self.fonts,
            assets: self
                .assets
                .as_ref()
                .map(|resolver| resolver as &dyn appcore_filemaker::AssetResolver),
        }
    }

    fn load_data(&self) -> CliResult<DataValue> {
        let Some(path) = &self.data_path else {
            return Ok(DataValue::Object(BTreeMap::new()));
        };
        let bytes = read_bounded(path, self.limits.max_template_bytes, self.json)?;
        serde_json::from_slice(&bytes).map_err(|error| {
            CliFailure::new(
                EXIT_DATA,
                "FM-CLI-DATA",
                format!("typed data JSON is invalid: {error}"),
                self.json,
            )
        })
    }
}

fn load_patches(parsed: &ParsedCli, limits: &ResourceLimits, json: bool) -> CliResult<Vec<Patch>> {
    let mut patches = Vec::new();
    let mut total_bytes = 0_usize;
    for path in parsed.option_values("patch") {
        let bytes = read_bounded(Path::new(path), limits.max_template_bytes, json)?;
        total_bytes = total_bytes.checked_add(bytes.len()).ok_or_else(|| {
            CliFailure::new(
                EXIT_DATA,
                "FM-CLI-PATCH-LIMIT",
                "runtime patch byte accounting overflow",
                json,
            )
        })?;
        if total_bytes > limits.max_template_bytes {
            return Err(CliFailure::new(
                EXIT_DATA,
                "FM-CLI-PATCH-LIMIT",
                "runtime patch files exceed the combined byte limit",
                json,
            ));
        }
        patches.push(serde_json::from_slice(&bytes).map_err(|error| {
            CliFailure::new(
                EXIT_DATA,
                "FM-CLI-PATCH",
                format!("runtime patch JSON is invalid: {error}"),
                json,
            )
        })?);
    }
    Ok(patches)
}

fn register_font(
    manager: &mut FontManager,
    declaration: &str,
    limits: &ResourceLimits,
    json: bool,
) -> CliResult<()> {
    let (name, path) = declaration
        .split_once('=')
        .ok_or_else(|| CliFailure::usage("--font requires NAME=FILE", json))?;
    if name.is_empty() || path.is_empty() {
        return Err(CliFailure::usage("--font requires NAME=FILE", json));
    }
    let bytes = read_bounded(Path::new(path), limits.max_asset_bytes, json)?;
    let font =
        FontAsset::new(name, bytes, 0).map_err(|error| CliFailure::from_core(error, json))?;
    manager
        .register(font)
        .map_err(|error| CliFailure::from_core(error, json))
}
