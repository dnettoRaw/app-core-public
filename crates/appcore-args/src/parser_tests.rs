// =============================================================================
//        #######
//     ###       ###     F: parser_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use crate::{
    ArgLimits, ArgumentSpec, CliErrorKind, CliParser, CliSpec, CommandSpec, OptionSpec, RawArgs,
    ValueType,
};

fn spec() -> CliSpec {
    CliSpec::new("demo")
        .option(OptionSpec::flag("verbose").short('v'))
        .command_required(true)
        .command(
            CommandSpec::new("publish")
                .option(OptionSpec::flag("force").short('f'))
                .command(CommandSpec::new("status").argument(ArgumentSpec::new("crate"))),
        )
}

#[test]
fn parses_nested_inherited_options_and_positionals() {
    let args = RawArgs::parse(["publish", "-v", "status", "-f", "crate"]).unwrap();
    let parsed = CliParser::new(&spec()).parse(&args).unwrap();
    assert_eq!(parsed.command_path(), &["publish", "status"]);
    assert!(parsed.has_flag("verbose"));
    assert!(parsed.has_flag("force"));
    assert_eq!(parsed.positionals(), &["crate"]);
}

#[test]
fn parses_grouped_flags_and_attached_short_value() {
    let spec = CliSpec::new("demo")
        .option(OptionSpec::flag("all").short('a'))
        .option(OptionSpec::flag("verbose").short('v'))
        .option(OptionSpec::value("output").short('o'));
    let parsed = CliParser::new(&spec)
        .parse(&RawArgs::parse(["-avoresult.txt"]).unwrap())
        .unwrap();
    assert!(parsed.has_flag("all"));
    assert!(parsed.has_flag("verbose"));
    assert_eq!(parsed.option_value("output"), Some("result.txt"));

    let equals = CliParser::new(&spec)
        .parse(&RawArgs::parse(["-o=result.txt"]).unwrap())
        .unwrap();
    assert_eq!(equals.option_value("output"), Some("result.txt"));
}

#[test]
fn validates_values_and_required_arguments() {
    let spec = CliSpec::new("demo")
        .option(OptionSpec::value("count").value_type(ValueType::U64))
        .argument(
            ArgumentSpec::new("mode")
                .required(true)
                .possible_value("fast"),
        );
    let invalid = CliParser::new(&spec)
        .parse(&RawArgs::parse(["--count", "x", "fast"]).unwrap())
        .unwrap_err();
    assert_eq!(invalid.kind(), CliErrorKind::InvalidValue);
    let missing = CliParser::new(&spec)
        .parse(&RawArgs::parse(Vec::<String>::new()).unwrap())
        .unwrap_err();
    assert_eq!(missing.kind(), CliErrorKind::MissingValue);
}

#[test]
fn enforces_option_relationships() {
    let spec = CliSpec::new("demo")
        .option(OptionSpec::flag("json").conflicts_with("plain"))
        .option(OptionSpec::flag("plain"));
    let error = CliParser::new(&spec)
        .parse(&RawArgs::parse(["--json", "--plain"]).unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), CliErrorKind::OptionConflict);
}

#[test]
fn bounds_raw_arguments_but_accepts_empty_values() {
    let limits = ArgLimits {
        max_words: 1,
        max_word_bytes: 4,
        max_total_bytes: 4,
    };
    assert!(RawArgs::parse_with_limits([""], limits).is_ok());
    assert_eq!(
        RawArgs::parse_with_limits(["hello"], limits)
            .unwrap_err()
            .kind(),
        CliErrorKind::InvalidInput
    );
}

#[test]
fn terminal_option_bypasses_required_inputs() {
    let spec = CliSpec::new("demo")
        .command_required(true)
        .option(OptionSpec::flag("help").terminal(true))
        .command(CommandSpec::new("run").argument(ArgumentSpec::new("file").required(true)));
    let parsed = CliParser::new(&spec)
        .parse(&RawArgs::parse(["--help"]).unwrap())
        .unwrap();
    assert!(parsed.has_flag("help"));
}

#[test]
fn optional_values_only_consume_unambiguous_attached_input() {
    let spec = CliSpec::new("demo")
        .option(OptionSpec::value("color").short('c').optional_value())
        .argument(ArgumentSpec::new("file"));

    let detached = CliParser::new(&spec)
        .parse(&RawArgs::parse(["--color", "input.txt"]).unwrap())
        .unwrap();
    assert_eq!(detached.option_value("color"), None);
    assert_eq!(detached.positionals(), &["input.txt"]);

    let long = CliParser::new(&spec)
        .parse(&RawArgs::parse(["--color=blue"]).unwrap())
        .unwrap();
    let short = CliParser::new(&spec)
        .parse(&RawArgs::parse(["-cblue"]).unwrap())
        .unwrap();
    assert_eq!(long.option_value("color"), Some("blue"));
    assert_eq!(short.option_value("color"), Some("blue"));
}

#[test]
fn optional_values_consume_valid_detached_input_only_when_enabled() {
    let spec = CliSpec::new("demo")
        .option(
            OptionSpec::value("enabled")
                .optional_value()
                .detached_optional_value(true)
                .value_type(ValueType::Bool),
        )
        .argument(ArgumentSpec::new("input"));

    let parsed = CliParser::new(&spec)
        .parse(&RawArgs::parse(["--enabled", "false", "input.txt"]).unwrap())
        .unwrap();
    assert_eq!(parsed.option_value("enabled"), Some("false"));
    assert_eq!(parsed.positionals(), &["input.txt"]);

    let positional = CliParser::new(&spec)
        .parse(&RawArgs::parse(["--enabled", "input.txt"]).unwrap())
        .unwrap();
    assert_eq!(positional.option_value("enabled"), None);
    assert_eq!(positional.positionals(), &["input.txt"]);
}

#[test]
fn preserves_passthrough_without_parsing_it_as_options() {
    let parsed = CliParser::new(&CliSpec::new("demo"))
        .parse(&RawArgs::parse(["--", "--unknown", "value"]).unwrap())
        .unwrap();

    assert_eq!(parsed.passthrough(), &["--unknown", "value"]);
}

#[test]
fn rejects_nul_bytes_at_the_raw_input_boundary() {
    let error = RawArgs::parse(["bad\0value"]).unwrap_err();
    assert_eq!(error.kind(), CliErrorKind::InvalidInput);
}

#[test]
fn accepts_negative_signed_positionals() {
    let spec = CliSpec::new("demo").argument(
        ArgumentSpec::new("offset")
            .value_type(ValueType::I64)
            .required(true),
    );
    let parsed = CliParser::new(&spec)
        .parse(&RawArgs::parse(["-42"]).unwrap())
        .unwrap();

    assert_eq!(parsed.positionals(), &["-42"]);
}

#[test]
fn reports_unknown_commands_with_a_bounded_suggestion() {
    let spec = CliSpec::new("demo")
        .command_required(true)
        .command(CommandSpec::new("publish"))
        .command(CommandSpec::new("status"));
    let error = CliParser::new(&spec)
        .parse(&RawArgs::parse(["publsh"]).unwrap())
        .unwrap_err();

    assert_eq!(error.kind(), CliErrorKind::UnexpectedArgument);
    assert!(error.to_string().contains("did you mean `publish`"));
}

#[test]
fn suggests_close_options_and_possible_values() {
    let spec = CliSpec::new("demo").option(
        OptionSpec::value("format")
            .possible_value("text")
            .possible_value("json"),
    );
    let option = CliParser::new(&spec)
        .parse(&RawArgs::parse(["--formt", "text"]).unwrap())
        .unwrap_err();
    let value = CliParser::new(&spec)
        .parse(&RawArgs::parse(["--format", "jsom"]).unwrap())
        .unwrap_err();

    assert_eq!(option.kind(), CliErrorKind::UnknownOption);
    assert!(option.to_string().contains("did you mean `--format`"));
    assert_eq!(value.kind(), CliErrorKind::InvalidValue);
    assert!(value.to_string().contains("did you mean `json`"));
}
