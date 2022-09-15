// =============================================================================
//        #######
//     ###       ###     F: parser.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::error::{CliError, CliErrorKind};
use crate::raw::RawArgs;
use crate::spec::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec, ValueMode, ValueType};
use crate::suggestion;
use crate::value_parser::{option_value, validate_value};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCli {
    command_path: Vec<String>,
    options: Vec<ParsedOption>,
    positionals: Vec<String>,
    passthrough: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedOption {
    name: String,
    value: Option<String>,
}

pub struct CliParser<'a> {
    spec: &'a CliSpec,
}

impl ParsedCli {
    pub fn command_path(&self) -> &[String] {
        &self.command_path
    }
    pub fn options(&self) -> &[ParsedOption] {
        &self.options
    }
    pub fn positionals(&self) -> &[String] {
        &self.positionals
    }
    pub fn passthrough(&self) -> &[String] {
        &self.passthrough
    }
    pub fn has_flag(&self, name: &str) -> bool {
        self.options.iter().any(|option| option.name == name)
    }
    pub fn option_value(&self, name: &str) -> Option<&str> {
        self.options
            .iter()
            .rev()
            .find(|option| option.name == name)
            .and_then(ParsedOption::value)
    }
    pub fn option_values<'b>(&'b self, name: &'b str) -> impl Iterator<Item = &'b str> + 'b {
        self.options
            .iter()
            .filter(move |option| option.name == name)
            .filter_map(ParsedOption::value)
    }
    pub fn option_occurrences(&self, name: &str) -> usize {
        self.options
            .iter()
            .filter(|option| option.name == name)
            .count()
    }
}

impl ParsedOption {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

impl<'a> CliParser<'a> {
    pub fn new(spec: &'a CliSpec) -> Self {
        Self { spec }
    }

    pub fn parse(&self, args: &RawArgs) -> Result<ParsedCli, CliError> {
        self.spec.validate().map_err(|error| {
            CliError::new(CliErrorKind::InvalidSpecification, error.to_string())
        })?;
        let mut state = ParseState::new(self.spec);
        let mut index = 0usize;
        while index < args.words().len() {
            let word = &args.words()[index];
            if word == "--" {
                state
                    .passthrough
                    .extend_from_slice(&args.words()[index + 1..]);
                break;
            }
            index = if word.starts_with("--") {
                parse_long_option(args.words(), index, &mut state)?
            } else if is_short_option(word) && !state.accepts_negative_positional(word) {
                parse_short_options(args.words(), index, &mut state)?
            } else if state.positionals.is_empty() && state.enter_command(word) {
                index + 1
            } else {
                state.push_positional(word)?;
                index + 1
            };
        }
        state.finish()
    }
}

struct ParseState<'a> {
    spec: &'a CliSpec,
    commands: Vec<&'a CommandSpec>,
    command_path: Vec<String>,
    options: Vec<ParsedOption>,
    positionals: Vec<String>,
    passthrough: Vec<String>,
}

impl<'a> ParseState<'a> {
    fn new(spec: &'a CliSpec) -> Self {
        Self {
            spec,
            commands: Vec::new(),
            command_path: Vec::new(),
            options: Vec::new(),
            positionals: Vec::new(),
            passthrough: Vec::new(),
        }
    }

    fn enter_command(&mut self, word: &str) -> bool {
        if let Some(command) = self
            .available_commands()
            .iter()
            .find(|command| command.matches(word))
        {
            self.command_path.push(command.name().to_string());
            self.commands.push(command);
            true
        } else {
            false
        }
    }

    fn available_commands(&self) -> &'a [CommandSpec] {
        self.commands
            .last()
            .map(|command| command.commands())
            .unwrap_or_else(|| self.spec.commands())
    }

    fn active_arguments(&self) -> &'a [ArgumentSpec] {
        self.commands
            .last()
            .map(|command| command.arguments())
            .unwrap_or_else(|| self.spec.arguments())
    }

    fn visible_options(&self) -> Vec<&'a OptionSpec> {
        let mut options = self.spec.options().iter().collect::<Vec<_>>();
        for command in &self.commands {
            options.extend(command.options());
        }
        options
    }

    fn find_long_option(&self, name: &str) -> Option<&'a OptionSpec> {
        self.visible_options()
            .into_iter()
            .rev()
            .find(|option| option.long() == name)
    }

    fn find_short_option(&self, short: char) -> Option<&'a OptionSpec> {
        self.visible_options()
            .into_iter()
            .rev()
            .find(|option| option.short_name() == Some(short))
    }

    fn push_option(&mut self, option: &OptionSpec, value: Option<String>) -> Result<(), CliError> {
        if !option.is_repeatable()
            && self
                .options
                .iter()
                .any(|parsed| parsed.name == option.long())
        {
            return Err(CliError::new(
                CliErrorKind::DuplicateOption,
                format!("option `--{}` cannot be repeated", option.long()),
            ));
        }
        if let Some(value) = value.as_deref() {
            validate_value(
                value,
                option.value_type_kind(),
                option.possible_values(),
                &format!("--{}", option.long()),
            )?;
        }
        self.options.push(ParsedOption {
            name: option.long().to_string(),
            value,
        });
        Ok(())
    }

    fn push_positional(&mut self, value: &str) -> Result<(), CliError> {
        let argument = self.next_argument();
        if argument.is_none()
            && self.positionals.is_empty()
            && !self.available_commands().is_empty()
        {
            let candidates = self
                .available_commands()
                .iter()
                .filter(|command| !command.is_hidden())
                .flat_map(|command| {
                    std::iter::once(command.name())
                        .chain(command.aliases().iter().map(String::as_str))
                });
            let message = suggestion::append(
                format!("unknown command `{value}`"),
                suggestion::closest(value, candidates),
                "",
            );
            return Err(CliError::new(CliErrorKind::UnexpectedArgument, message));
        }
        let argument = argument.ok_or_else(|| {
            CliError::new(
                CliErrorKind::UnexpectedArgument,
                format!("unexpected argument `{value}`"),
            )
        })?;
        validate_value(
            value,
            argument.value_type_kind(),
            argument.possible_values(),
            argument.name(),
        )?;
        self.positionals.push(value.to_string());
        Ok(())
    }

    fn next_argument(&self) -> Option<&'a ArgumentSpec> {
        let arguments = self.active_arguments();
        arguments
            .get(self.positionals.len())
            .or_else(|| arguments.last().filter(|argument| argument.is_multiple()))
    }

    fn accepts_negative_positional(&self, value: &str) -> bool {
        let Some(argument) = self.next_argument() else {
            return false;
        };
        argument.value_type_kind() == ValueType::I64
            && value.parse::<i64>().is_ok()
            && value
                .chars()
                .nth(1)
                .is_none_or(|short| self.find_short_option(short).is_none())
    }

    fn finish(self) -> Result<ParsedCli, CliError> {
        if !has_terminal_option(&self) {
            validate_required_command(&self)?;
            validate_required_arguments(&self)?;
            validate_option_rules(&self)?;
        }
        Ok(ParsedCli {
            command_path: self.command_path,
            options: self.options,
            positionals: self.positionals,
            passthrough: self.passthrough,
        })
    }
}

fn has_terminal_option(state: &ParseState<'_>) -> bool {
    state.visible_options().iter().any(|option| {
        option.is_terminal()
            && state
                .options
                .iter()
                .any(|parsed| parsed.name == option.long())
    })
}

fn parse_long_option(
    words: &[String],
    index: usize,
    state: &mut ParseState<'_>,
) -> Result<usize, CliError> {
    let raw = words[index].strip_prefix("--").unwrap_or_default();
    let (name, inline) = raw
        .split_once('=')
        .map_or((raw, None), |(name, value)| (name, Some(value.to_string())));
    if name.is_empty() {
        return Err(CliError::new(
            CliErrorKind::UnknownOption,
            "empty long option is not accepted",
        ));
    }
    let option = state.find_long_option(name).ok_or_else(|| {
        let candidates = state
            .visible_options()
            .into_iter()
            .filter(|option| !option.is_hidden())
            .map(OptionSpec::long);
        let message = suggestion::append(
            format!("unknown option `--{name}`"),
            suggestion::closest(name, candidates),
            "--",
        );
        CliError::new(CliErrorKind::UnknownOption, message)
    })?;
    let (value, next) = option_value(words, index, inline, option, false)?;
    state.push_option(option, value)?;
    Ok(next)
}

fn parse_short_options(
    words: &[String],
    index: usize,
    state: &mut ParseState<'_>,
) -> Result<usize, CliError> {
    let raw = words[index].strip_prefix('-').unwrap_or_default();
    let mut chars = raw.char_indices().peekable();
    while let Some((_, short)) = chars.next() {
        let option = state.find_short_option(short).ok_or_else(|| {
            CliError::new(
                CliErrorKind::UnknownOption,
                format!("unknown option `-{short}`"),
            )
        })?;
        let remainder = chars.peek().map(|(offset, _)| {
            raw[*offset..]
                .strip_prefix('=')
                .unwrap_or(&raw[*offset..])
                .to_string()
        });
        let (value, next) = option_value(words, index, remainder, option, true)?;
        state.push_option(option, value)?;
        if option.value_mode() != ValueMode::Forbidden {
            return Ok(next);
        }
    }
    Ok(index + 1)
}

fn validate_required_command(state: &ParseState<'_>) -> Result<(), CliError> {
    let required = state
        .commands
        .last()
        .map(|command| command.is_command_required())
        .unwrap_or_else(|| state.spec.is_command_required());
    if required && !state.available_commands().is_empty() {
        return Err(CliError::new(
            CliErrorKind::MissingCommand,
            format!(
                "a command is required after `{}`",
                if state.command_path.is_empty() {
                    state.spec.name().to_string()
                } else {
                    state.command_path.join(" ")
                }
            ),
        ));
    }
    Ok(())
}

fn validate_required_arguments(state: &ParseState<'_>) -> Result<(), CliError> {
    let provided = state.positionals.len();
    for (index, argument) in state.active_arguments().iter().enumerate() {
        if argument.is_required() && index >= provided {
            return Err(CliError::new(
                CliErrorKind::MissingValue,
                format!("missing required argument `<{}>`", argument.name()),
            ));
        }
    }
    Ok(())
}

fn validate_option_rules(state: &ParseState<'_>) -> Result<(), CliError> {
    let present = state
        .options
        .iter()
        .map(|option| option.name.as_str())
        .collect::<HashSet<_>>();
    for option in state.visible_options() {
        if option.is_required() && !present.contains(option.long()) {
            return Err(CliError::new(
                CliErrorKind::MissingOption,
                format!("required option `--{}` was not provided", option.long()),
            ));
        }
        if !present.contains(option.long()) {
            continue;
        }
        if let Some(conflict) = option
            .conflicts()
            .iter()
            .find(|name| present.contains(name.as_str()))
        {
            return Err(CliError::new(
                CliErrorKind::OptionConflict,
                format!("option `--{}` conflicts with `--{conflict}`", option.long()),
            ));
        }
        if let Some(required) = option
            .requirements()
            .iter()
            .find(|name| !present.contains(name.as_str()))
        {
            return Err(CliError::new(
                CliErrorKind::MissingRequirement,
                format!("option `--{}` requires `--{required}`", option.long()),
            ));
        }
    }
    Ok(())
}

fn is_short_option(word: &str) -> bool {
    word.starts_with('-') && !word.starts_with("--") && word.len() > 1
}
