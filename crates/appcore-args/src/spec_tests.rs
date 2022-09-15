// =============================================================================
//        #######
//     ###       ###     F: spec_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use crate::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec};

#[test]
fn rejects_duplicate_command_aliases() {
    let spec = CliSpec::new("demo")
        .command(CommandSpec::new("run").alias("start"))
        .command(CommandSpec::new("start"));
    assert!(spec
        .validate()
        .unwrap_err()
        .to_string()
        .contains("duplicate command or alias"));
}

#[test]
fn rejects_non_terminal_variadic_argument() {
    let spec = CliSpec::new("demo")
        .argument(ArgumentSpec::new("files").multiple(true))
        .argument(ArgumentSpec::new("out"));
    assert!(spec
        .validate()
        .unwrap_err()
        .to_string()
        .contains("must be last"));
}

#[test]
fn rejects_unknown_option_relationship() {
    let spec = CliSpec::new("demo").option(OptionSpec::flag("json").requires("output"));
    assert!(spec
        .validate()
        .unwrap_err()
        .to_string()
        .contains("unknown or self option"));
}

#[test]
fn rejects_an_option_that_shadows_an_inherited_option() {
    let spec = CliSpec::new("demo")
        .option(OptionSpec::flag("verbose").short('v'))
        .command(CommandSpec::new("run").option(OptionSpec::flag("verbose")));
    assert!(spec
        .validate()
        .unwrap_err()
        .to_string()
        .contains("duplicate option"));
}
