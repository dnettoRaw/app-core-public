// =============================================================================
//        #######
//     ###       ###     F: phases.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Isolates binding from measurement/layout/collision on the same A4 fixture.

use appcore_filemaker::{
    Compiler, DataValue, LayoutEngine, LayoutOptions, PatchTransaction, ResourceLimits,
};
use std::hint::black_box;

const BIND_CASE: &str = "bind_a4_precompiled";
const LAYOUT_CASE: &str = "layout_a4_prepared";
pub(super) const CASES: [&str; 2] = [BIND_CASE, LAYOUT_CASE];

pub(super) fn run(selected: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    if selected.is_none_or(|value| value == BIND_CASE) {
        benchmark_bind()?;
    }
    if selected.is_none_or(|value| value == LAYOUT_CASE) {
        benchmark_layout()?;
    }
    Ok(())
}

fn benchmark_bind() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = Compiler::builder().build()?;
    let template = compiler.compile_template_yaml(super::A4_TEMPLATE)?;
    let data: DataValue = serde_json::from_slice(super::A4_DATA)?;
    super::measure(BIND_CASE, 100, || {
        black_box(compiler.bind(&template, &data, &[])?);
        Ok(())
    })
}

fn benchmark_layout() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(super::A4_TEMPLATE)?;
    let data: DataValue = serde_json::from_slice(super::A4_DATA)?;
    let mut document = compiler.bind(&template, &data, &[])?;
    PatchTransaction::new(&mut document, limits.max_patch_operations).apply(&super::a4_patch()?)?;
    let fonts = super::fonts()?;
    let engine = LayoutEngine::new(&limits, &fonts, LayoutOptions::default())?;
    super::measure(LAYOUT_CASE, 20, || {
        black_box(engine.resolve(&document)?);
        Ok(())
    })
}
