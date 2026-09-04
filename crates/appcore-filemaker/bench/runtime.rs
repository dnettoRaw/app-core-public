// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 18:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Measures focused compilation and bounded editable/hybrid A4 report pipelines.

use appcore_filemaker::{
    export, export_collision_mask, export_dataset_csv, preflight, BorrowedDataset, CollisionMask,
    Compiler, DataValue, DocumentFingerprint, DocumentIr, ElementId, ExportContext, ExportFormat,
    ExportRequest, Fidelity, FontAsset, FontManager, HtmlMode, LayoutEngine, LayoutOptions,
    MaskFormat, OperationControl, Patch, PatchOperation, PatchTransaction, PdfMode,
    PreflightOptions, Rect, ResolvedScene, ResourceLimits, Size, Unit,
};
use std::hint::black_box;
use std::io::{sink, Write};
use std::time::Instant;

const CANVAS_TEMPLATE: &[u8] = br"filemaker: '1.0'
model: canvas
id: bench-document
page: { width: 1920px, height: 1080px }
elements:
  - { id: title, type: text, x: 80px, y: 80px, width: 800px, height: 120px, text: 'Bench' }
  - { id: panel, type: rect, x: 80px, y: 240px, width: 1760px, height: 700px }
";
const A4_TEMPLATE: &[u8] = include_bytes!("../examples/intermediate.yml");
const A4_DATA: &[u8] = include_bytes!("../examples/intermediate-data.json");
const NOTO_SANS: &[u8] = include_bytes!("../examples/assets/NotoSans-Regular.ttf");
const COMPILE_CASE: &str = "compile_canvas_yaml";
const A4_CASE: &str = "a4_report_end_to_end";
const A4_HYBRID_CASE: &str = "a4_report_pdf_hybrid";
const A4_EXPORT_MATRIX_CASE: &str = "a4_report_export_matrix";
const FINGERPRINT_CASE: &str = "fingerprint_json_4m";
const COLLISION_MASK_CASE: &str = "collision_mask_json_4m";
const COLLISION_MASK_PDF_CASE: &str = "collision_mask_pdf_100k";

#[derive(Default)]
struct ByteCounter {
    bytes: usize,
}

impl Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("benchmark byte count overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle", true);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == COMPILE_CASE)
    {
        benchmark_compile_canvas()?;
    }
    if selected.as_deref().is_none_or(|value| value == A4_CASE) {
        benchmark_a4_report(A4_CASE, PdfMode::Editable)?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == A4_HYBRID_CASE)
    {
        benchmark_a4_report(A4_HYBRID_CASE, PdfMode::Hybrid)?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == A4_EXPORT_MATRIX_CASE)
    {
        benchmark_a4_export_matrix()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == FINGERPRINT_CASE)
    {
        benchmark_fingerprint()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == COLLISION_MASK_CASE)
    {
        benchmark_collision_mask_json()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == COLLISION_MASK_PDF_CASE)
    {
        benchmark_collision_mask_pdf()?;
    }
    if let Some(value) = selected.as_deref() {
        if value != COMPILE_CASE
            && value != A4_CASE
            && value != A4_HYBRID_CASE
            && value != A4_EXPORT_MATRIX_CASE
            && value != FINGERPRINT_CASE
            && value != COLLISION_MASK_CASE
            && value != COLLISION_MASK_PDF_CASE
        {
            return Err(format!("unknown FileMaker benchmark case: {value}").into());
        }
    }
    memory_checkpoint("retained", true);
    Ok(())
}

fn benchmark_collision_mask_json() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let mask = collision_mask(20_736)?;
    let mut counter = ByteCounter::default();
    export_collision_mask(&mask, MaskFormat::Json, 72, &limits, &mut counter)?;
    println!(
        "appcore-filemaker::{COLLISION_MASK_CASE} fixture_bytes={}",
        counter.bytes
    );
    measure(COLLISION_MASK_CASE, 10, || {
        black_box(export_collision_mask(
            &mask,
            MaskFormat::Json,
            72,
            &limits,
            &mut sink(),
        )?);
        Ok(())
    })
}

fn benchmark_collision_mask_pdf() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let mask = collision_mask(100_000)?;
    let mut counter = ByteCounter::default();
    export_collision_mask(&mask, MaskFormat::Pdf, 72, &limits, &mut counter)?;
    println!(
        "appcore-filemaker::{COLLISION_MASK_PDF_CASE} fixture_bytes={}",
        counter.bytes
    );
    measure(COLLISION_MASK_PDF_CASE, 5, || {
        black_box(export_collision_mask(
            &mask,
            MaskFormat::Pdf,
            72,
            &limits,
            &mut sink(),
        )?);
        Ok(())
    })
}

fn collision_mask(entries: usize) -> Result<CollisionMask, appcore_filemaker::FileMakerError> {
    let bounds = Rect::new(Unit::ZERO, Unit::ZERO, Unit::points(10)?, Unit::points(10)?)?;
    let occupied = (0..entries)
        .map(|index| Ok((ElementId::new(format!("bench-{index:05}"))?, bounds)))
        .collect::<Result<Vec<_>, appcore_filemaker::FileMakerError>>()?;
    Ok(CollisionMask {
        page: 0,
        size: Size::new(Unit::points(1920)?, Unit::points(1080)?)?,
        occupied,
        free: Vec::new(),
        collisions: Vec::new(),
        overflow: Vec::new(),
    })
}

fn benchmark_fingerprint() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let template = compiler.compile_template_yaml(CANVAS_TEMPLATE)?;
    let data = DataValue::String("d".repeat(4 * 1024 * 1024));
    let fonts = FontManager::default();
    measure(FINGERPRINT_CASE, 10, || {
        black_box(DocumentFingerprint::compute(
            &template, &data, None, &fonts, &limits,
        )?);
        Ok(())
    })
}

fn benchmark_compile_canvas() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = Compiler::builder().build()?;
    measure(COMPILE_CASE, 2_000, || {
        black_box(compiler.compile_template_yaml(black_box(CANVAS_TEMPLATE)))?;
        Ok(())
    })
}

fn benchmark_a4_report(
    case_name: &str,
    pdf_mode: PdfMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let fonts = fonts()?;
    let request = ExportRequest {
        format: ExportFormat::Pdf,
        pdf_mode,
        ..ExportRequest::default()
    };
    let control = OperationControl::default();
    let patch = a4_patch()?;
    measure(case_name, 5, || {
        let (_, scene) = resolve_a4(&compiler, &limits, &fonts, &patch)?;
        let context = ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        };
        let report = preflight(
            &scene,
            &request,
            &context,
            &PreflightOptions {
                strict: true,
                ..PreflightOptions::default()
            },
            &control,
        )?;
        report.enforce(true)?;
        let outcome = export(&scene, &request, &context, &mut sink())?;
        black_box((scene.pages.len(), outcome.bytes_written));
        Ok(())
    })
}

fn benchmark_a4_export_matrix() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default();
    let compiler = Compiler::builder().limits(limits.clone()).build()?;
    let fonts = fonts()?;
    let requests = export_matrix_requests();
    let patch = a4_patch()?;
    let control = OperationControl::default();
    measure(A4_EXPORT_MATRIX_CASE, 1, || {
        let (document, scene) = resolve_a4(&compiler, &limits, &fonts, &patch)?;
        let context = ExportContext {
            limits: &limits,
            fonts: &fonts,
            assets: None,
        };
        let mut bytes_written = 0usize;
        for request in &requests {
            let strict = request.fidelity == Fidelity::Strict;
            let report = preflight(
                &scene,
                request,
                &context,
                &PreflightOptions {
                    strict,
                    ..PreflightOptions::default()
                },
                &control,
            )?;
            report.enforce(strict)?;
            let outcome = export(&scene, request, &context, &mut sink())?;
            bytes_written = bytes_written
                .checked_add(outcome.bytes_written)
                .ok_or_else(|| std::io::Error::other("export byte count overflow"))?;
        }
        let table = document
            .elements
            .iter()
            .find_map(|element| element.table.as_ref())
            .ok_or_else(|| std::io::Error::other("A4 fixture has no table"))?;
        let outcome = export_dataset_csv(
            &table.spec,
            &BorrowedDataset::new(&table.rows),
            &limits,
            &mut sink(),
        )?;
        bytes_written = bytes_written
            .checked_add(outcome.bytes_written)
            .ok_or_else(|| std::io::Error::other("export byte count overflow"))?;
        black_box((scene.pages.len(), bytes_written));
        Ok(())
    })
}

fn resolve_a4(
    compiler: &Compiler,
    limits: &ResourceLimits,
    fonts: &FontManager,
    patch: &Patch,
) -> Result<(DocumentIr, ResolvedScene), Box<dyn std::error::Error>> {
    let template = compiler.compile_template_yaml(black_box(A4_TEMPLATE))?;
    let data: DataValue = serde_json::from_slice(black_box(A4_DATA))?;
    let mut document = compiler.bind(&template, &data, &[])?;
    PatchTransaction::new(&mut document, limits.max_patch_operations).apply(patch)?;
    let scene = LayoutEngine::new(limits, fonts, LayoutOptions::default())?.resolve(&document)?;
    Ok((document, scene))
}

fn export_matrix_requests() -> [ExportRequest; 8] {
    [
        pdf_request(PdfMode::Editable),
        pdf_request(PdfMode::Flattened),
        pdf_request(PdfMode::Hybrid),
        ExportRequest {
            format: ExportFormat::Svg,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Html,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Html,
            html_mode: HtmlMode::Fixed,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Png,
            page: Some(0),
            dpi: 36,
            ..ExportRequest::default()
        },
        ExportRequest {
            format: ExportFormat::Jpeg,
            fidelity: Fidelity::BestEffort,
            page: Some(0),
            dpi: 36,
            ..ExportRequest::default()
        },
    ]
}

fn pdf_request(pdf_mode: PdfMode) -> ExportRequest {
    ExportRequest {
        format: ExportFormat::Pdf,
        pdf_mode,
        ..ExportRequest::default()
    }
}

fn a4_patch() -> Result<Patch, appcore_filemaker::FileMakerError> {
    Ok(Patch {
        sequence: 1,
        operations: vec![PatchOperation::Move {
            id: ElementId::new("review-marker")?,
            x: "185mm".parse()?,
            y: "112mm".parse()?,
        }],
    })
}

fn fonts() -> Result<FontManager, Box<dyn std::error::Error>> {
    let mut fonts = FontManager::default();
    fonts.register(FontAsset::new("NotoSans", NOTO_SANS.to_vec(), 0)?)?;
    Ok(fonts)
}

fn measure(
    case_name: &str,
    fallback_iterations: u64,
    mut operation: impl FnMut() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let iterations = iterations(fallback_iterations);
    memory_checkpoint("workload", true);
    let started = Instant::now();
    for _ in 0..iterations {
        operation()?;
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-filemaker::{case_name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

fn memory_checkpoint(phase: &str, settle: bool) {
    let Some(milliseconds) = std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1..=1_000).contains(value))
    else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::stdout().flush();
    if settle {
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}
