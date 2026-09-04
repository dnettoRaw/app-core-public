// =============================================================================
//        #######
//     ###       ###     F: spec.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded spec contracts and behavior for this crate.

use appcore_args::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec, ValueType};

pub(crate) fn build() -> CliSpec {
    CliSpec::new("appcore-filemaker")
        .about("Deterministic declarative document, canvas, and dataset compiler")
        .version(env!("CARGO_PKG_VERSION"))
        .command_required(true)
        .option(OptionSpec::flag("json").about("Emit stable JSON output"))
        .option(
            OptionSpec::value("data")
                .value_name("FILE")
                .about("Typed data JSON"),
        )
        .option(
            OptionSpec::value("assets-root")
                .value_name("DIR")
                .about("Sandbox root for includes and assets"),
        )
        .option(
            OptionSpec::value("font")
                .value_name("NAME=FILE")
                .repeatable(true)
                .about("Register an explicit font"),
        )
        .option(
            OptionSpec::value("font-fallback")
                .value_name("NAME")
                .repeatable(true)
                .about("Append a registered font to deterministic fallback order"),
        )
        .option(
            OptionSpec::value("patch")
                .value_name("FILE")
                .repeatable(true)
                .about("Apply an ordered runtime patch JSON file before layout"),
        )
        .option(OptionSpec::flag("strict").about("Reject preflight warnings"))
        .option(
            OptionSpec::value("dpi")
                .value_type(ValueType::U64)
                .value_name("DPI")
                .about("Raster resolution (default 144)"),
        )
        .option(OptionSpec::flag("help").short('h').terminal(true))
        .option(OptionSpec::flag("version").terminal(true))
        .command(template_command(
            "check",
            "Parse and validate template schema",
        ))
        .command(template_command(
            "validate",
            "Bind and validate resolved layout",
        ))
        .command(render_command())
        .command(debug_command())
        .command(mask_command())
        .command(inspect_command())
        .command(explain_command())
        .command(free_regions_command())
        .command(preflight_command())
        .command(CommandSpec::new("schema").about("Describe the supported schema"))
        .command(CommandSpec::new("capabilities").about("List exporter capabilities"))
        .command(
            CommandSpec::new("migrate")
                .about("Reserved migration command; never mutates without a future explicit flag")
                .argument(template_argument()),
        )
}

fn template_command(name: &str, about: &str) -> CommandSpec {
    CommandSpec::new(name)
        .about(about)
        .argument(template_argument())
}

fn page_command(name: &str, about: &str) -> CommandSpec {
    template_command(name, about).option(page_option())
}

fn render_command() -> CommandSpec {
    template_command("render", "Compile and render an output artifact")
        .option(output_option())
        .option(render_format_option())
        .option(page_option())
        .option(
            OptionSpec::value("table")
                .value_name("ELEMENT")
                .about("Table element selected for CSV output"),
        )
        .option(
            OptionSpec::value("jpeg-quality")
                .value_type(ValueType::U64)
                .value_name("1..100"),
        )
        .option(
            OptionSpec::value("pdf-mode")
                .possible_value("editable")
                .possible_value("flattened")
                .possible_value("hybrid"),
        )
        .option(
            OptionSpec::value("html-mode")
                .possible_value("semantic")
                .possible_value("fixed"),
        )
        .option(OptionSpec::flag("best-effort"))
}

fn debug_command() -> CommandSpec {
    page_command("debug", "Derive a non-mutating debug overlay")
        .option(
            OptionSpec::value("grid")
                .value_type(ValueType::U64)
                .possible_value("1")
                .possible_value("5")
                .possible_value("10")
                .possible_value("20"),
        )
        .option(view_option())
}

fn mask_command() -> CommandSpec {
    page_command("mask", "Export a geometry-derived collision mask")
        .option(output_option())
        .option(
            OptionSpec::value("format")
                .required(true)
                .possible_value("json")
                .possible_value("svg")
                .possible_value("png")
                .possible_value("pdf"),
        )
        .option(view_option())
}

fn view_option() -> OptionSpec {
    OptionSpec::value("view")
        .possible_value("collision")
        .possible_value("layout")
        .possible_value("visual")
        .possible_value("combined")
}

fn inspect_command() -> CommandSpec {
    page_command("inspect", "Inspect an element or page").option(
        OptionSpec::value("id")
            .value_name("ELEMENT")
            .conflicts_with("page"),
    )
}

fn explain_command() -> CommandSpec {
    template_command("explain", "Explain resolved layout provenance")
        .option(OptionSpec::value("id").value_name("ELEMENT").required(true))
}

fn free_regions_command() -> CommandSpec {
    page_command("free-regions", "Query free resolved page rectangles")
        .option(
            OptionSpec::value("minimum-width")
                .value_name("LENGTH")
                .required(true),
        )
        .option(
            OptionSpec::value("minimum-height")
                .value_name("LENGTH")
                .required(true),
        )
}

fn preflight_command() -> CommandSpec {
    template_command("preflight", "Run exporter-aware preflight")
        .option(export_format_option())
        .option(page_option())
        .option(
            OptionSpec::value("pdf-mode")
                .possible_value("editable")
                .possible_value("flattened")
                .possible_value("hybrid"),
        )
        .option(
            OptionSpec::value("html-mode")
                .possible_value("semantic")
                .possible_value("fixed"),
        )
        .option(OptionSpec::flag("best-effort"))
        .option(OptionSpec::flag("require-accessibility"))
}

fn template_argument() -> ArgumentSpec {
    ArgumentSpec::new("TEMPLATE")
        .required(true)
        .about("Version 1.0 YAML template")
}

fn output_option() -> OptionSpec {
    OptionSpec::value("output")
        .short('o')
        .value_name("FILE")
        .required(true)
}

fn export_format_option() -> OptionSpec {
    OptionSpec::value("format")
        .required(true)
        .possible_value("pdf")
        .possible_value("svg")
        .possible_value("png")
        .possible_value("jpeg")
        .possible_value("html")
}

fn render_format_option() -> OptionSpec {
    export_format_option().possible_value("csv")
}

fn page_option() -> OptionSpec {
    OptionSpec::value("page")
        .value_type(ValueType::U64)
        .value_name("INDEX")
}
