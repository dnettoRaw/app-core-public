// =============================================================================
//        #######
//     ###       ###     F: spec_validation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec, SpecError, ValueMode, ValueType};
use std::collections::HashSet;

pub(crate) fn validate_spec(spec: &CliSpec) -> Result<(), SpecError> {
    validate_name("CLI", spec.name())?;
    validate_scope(
        spec.name(),
        spec.commands(),
        spec.options(),
        spec.arguments(),
        &[],
    )
}

fn validate_scope(
    path: &str,
    commands: &[CommandSpec],
    options: &[OptionSpec],
    arguments: &[ArgumentSpec],
    inherited_options: &[&OptionSpec],
) -> Result<(), SpecError> {
    validate_commands(path, commands)?;
    validate_options(path, options, inherited_options)?;
    validate_arguments(path, arguments)?;
    let mut visible_options = inherited_options.to_vec();
    visible_options.extend(options);
    for command in commands {
        validate_scope(
            &format!("{path} {}", command.name()),
            command.commands(),
            command.options(),
            command.arguments(),
            &visible_options,
        )?;
    }
    Ok(())
}

fn validate_commands(path: &str, commands: &[CommandSpec]) -> Result<(), SpecError> {
    let mut names = HashSet::new();
    for command in commands {
        validate_name("command", command.name())?;
        for name in
            std::iter::once(command.name()).chain(command.aliases().iter().map(String::as_str))
        {
            validate_name("command alias", name)?;
            if !names.insert(name) {
                return Err(error(format!(
                    "duplicate command or alias `{name}` under `{path}`"
                )));
            }
        }
    }
    Ok(())
}

fn validate_options(
    path: &str,
    options: &[OptionSpec],
    inherited: &[&OptionSpec],
) -> Result<(), SpecError> {
    let mut longs = inherited
        .iter()
        .map(|option| option.long())
        .collect::<HashSet<_>>();
    let mut shorts = inherited
        .iter()
        .filter_map(|option| option.short_name())
        .collect::<HashSet<_>>();
    for option in options {
        validate_option(path, option, &mut longs, &mut shorts)?;
    }
    for option in options {
        validate_option_relationships(path, option, &longs)?;
    }
    Ok(())
}

fn validate_option<'a>(
    path: &str,
    option: &'a OptionSpec,
    longs: &mut HashSet<&'a str>,
    shorts: &mut HashSet<char>,
) -> Result<(), SpecError> {
    validate_name("option", option.long())?;
    if !longs.insert(option.long()) {
        return Err(error(format!(
            "duplicate option `--{}` under `{path}`",
            option.long()
        )));
    }
    if let Some(short) = option.short_name() {
        if short == '-' || short.is_whitespace() || !short.is_ascii() {
            return Err(error(format!(
                "invalid short option `-{short}` under `{path}`"
            )));
        }
        if !shorts.insert(short) {
            return Err(error(format!(
                "duplicate short option `-{short}` under `{path}`"
            )));
        }
    }
    if option.value_mode() == ValueMode::Forbidden
        && (!option.possible_values().is_empty() || option.value_type_kind() != ValueType::String)
    {
        return Err(error(format!(
            "flag `--{}` cannot declare value validation",
            option.long()
        )));
    }
    validate_possible_values(
        option.possible_values(),
        &format!("option `--{}`", option.long()),
    )
}

fn validate_option_relationships(
    path: &str,
    option: &OptionSpec,
    visible: &HashSet<&str>,
) -> Result<(), SpecError> {
    for related in option.conflicts().iter().chain(option.requirements()) {
        if related == option.long() || !visible.contains(related.as_str()) {
            return Err(error(format!(
                "option `--{}` references unknown or self option `--{related}` under `{path}`",
                option.long()
            )));
        }
    }
    Ok(())
}

fn validate_arguments(path: &str, arguments: &[ArgumentSpec]) -> Result<(), SpecError> {
    let mut names = HashSet::new();
    let mut optional_seen = false;
    for (index, argument) in arguments.iter().enumerate() {
        validate_name("argument", argument.name())?;
        if !names.insert(argument.name()) {
            return Err(error(format!(
                "duplicate argument `{}` under `{path}`",
                argument.name()
            )));
        }
        if argument.is_required() && optional_seen {
            return Err(error(format!(
                "required argument `{}` follows an optional argument under `{path}`",
                argument.name()
            )));
        }
        optional_seen |= !argument.is_required();
        if argument.is_multiple() && index + 1 != arguments.len() {
            return Err(error(format!(
                "variadic argument `{}` must be last under `{path}`",
                argument.name()
            )));
        }
        validate_possible_values(
            argument.possible_values(),
            &format!("argument `{}`", argument.name()),
        )?;
    }
    Ok(())
}

fn validate_possible_values(values: &[String], owner: &str) -> Result<(), SpecError> {
    let mut unique = HashSet::new();
    for value in values {
        if value.is_empty() {
            return Err(error(format!("{owner} contains an empty possible value")));
        }
        if !unique.insert(value) {
            return Err(error(format!(
                "{owner} contains duplicate possible value `{value}`"
            )));
        }
    }
    Ok(())
}

fn validate_name(kind: &str, name: &str) -> Result<(), SpecError> {
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(error(format!("invalid {kind} name `{name}`")))
    }
}

fn error(message: impl Into<String>) -> SpecError {
    SpecError::new_internal(message)
}
