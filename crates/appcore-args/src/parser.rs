// =============================================================================
//        #######
//     ###       ###     F: parser.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Defines bounded parser contracts and behavior for this crate.

use crate::error::{CliError, CliErrorKind};
use crate::raw::RawArgs;
use crate::spec::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec, ValueMode, ValueType};
use crate::suggestion;

use std::collections::HashSet;

pub struct CliParser<'a> {
    spec: &'a CliSpec,
}

impl<'a> CliParser<'a> {
    pub fn new(spec: &'a CliSpec) -> Self {
        Self { spec }
    }

    pub fn parse(&self, args: &RawArgs) -> Result<crate::ParsedCli, CliError> {
        self.parse_with_env(args, |name| std::env::var_os(name))
    }
    /// Parses with an injected environment lookup; argv wins over env and defaults.
    pub fn parse_with_env(
        &self,
        args: &RawArgs,
        env: impl FnMut(&str) -> Option<std::ffi::OsString>,
    ) -> Result<crate::ParsedCli, CliError> {
        let raw = crate::RawOsArgs::parse_with_limits(
            args.words().iter().map(std::ffi::OsString::from),
            args.limits(),
        )?;
        self.parse_os_with_env(&raw, env)?.into_text()
    }
    /// Preserves native path arguments and passthrough bytes without UTF-8 conversion.
    pub fn parse_os(&self, args: &crate::RawOsArgs) -> Result<crate::ParsedOsCli, CliError> {
        self.parse_os_with_env(args, |name| std::env::var_os(name))
    }
    /// Native parser with deterministic environment injection for hosts and tests.
    pub fn parse_os_with_env(
        &self,
        args: &crate::RawOsArgs,
        mut env: impl FnMut(&str) -> Option<std::ffi::OsString>,
    ) -> Result<crate::ParsedOsCli, CliError> {
        self.spec.validate().map_err(|error| {
            CliError::new(CliErrorKind::InvalidSpecification, error.to_string())
        })?;
        let mut state = ParseState::new(self.spec);
        let mut index = 0;
        while index < args.words().len() {
            let word = &args.words()[index];
            let text = word.to_str();
            if text == Some("--") {
                state
                    .passthrough
                    .extend_from_slice(&args.words()[index + 1..]);
                break;
            }
            let bytes = word.as_encoded_bytes();
            index = if bytes.starts_with(b"--") {
                parse_long_option(args.words(), index, &mut state)?
            } else if bytes.starts_with(b"-")
                && bytes.len() > 1
                && !text.is_some_and(|s| state.accepts_negative_positional(s))
            {
                parse_short_options(args.words(), index, &mut state)?
            } else if state.positionals.is_empty() && text.is_some_and(|s| state.enter_command(s)) {
                index + 1
            } else {
                state.push_positional(word)?;
                index + 1
            };
        }
        if !has_terminal_option(&state) {
            state.resolve_sources(args, &mut env)?;
        }
        state.finish()
    }
}

struct ParseState<'a> {
    spec: &'a CliSpec,
    commands: Vec<&'a CommandSpec>,
    command_path: Vec<String>,
    options: Vec<crate::native::ParsedOsOption>,
    positionals: Vec<std::ffi::OsString>,
    passthrough: Vec<std::ffi::OsString>,
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

    fn push_option(
        &mut self,
        option: &OptionSpec,
        value: Option<std::ffi::OsString>,
    ) -> Result<(), CliError> {
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
            crate::native::validate_native(
                value,
                option.kind,
                option.value_type_kind(),
                option.possible_values(),
                &format!("--{}", option.long()),
            )?;
        }
        self.options.push(crate::native::ParsedOsOption {
            name: option.long().to_string(),
            value,
        });
        Ok(())
    }

    fn push_positional(&mut self, value: &std::ffi::OsStr) -> Result<(), CliError> {
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
                format!("unknown command `{}`", value.to_string_lossy()),
                suggestion::closest(value.to_str().unwrap_or(""), candidates),
                "",
            );
            return Err(CliError::new(CliErrorKind::UnexpectedArgument, message));
        }
        let argument = argument.ok_or_else(|| {
            CliError::new(
                CliErrorKind::UnexpectedArgument,
                format!("unexpected argument `{}`", value.to_string_lossy()),
            )
        })?;
        crate::native::validate_native(
            value,
            argument.kind,
            argument.value_type_kind(),
            argument.possible_values(),
            argument.name(),
        )?;
        self.positionals.push(value.to_os_string());
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

    fn resolve_sources(
        &mut self,
        args: &crate::RawOsArgs,
        env: &mut impl FnMut(&str) -> Option<std::ffi::OsString>,
    ) -> Result<(), CliError> {
        let limits = args.limits();
        let mut total = args
            .words()
            .iter()
            .map(|w| w.as_encoded_bytes().len())
            .sum::<usize>();
        let mut count = args.words().len();
        for option in self.visible_options() {
            if self.options.iter().any(|o| o.name == option.long()) {
                continue;
            }
            let value = option
                .environment
                .as_deref()
                .and_then(&mut *env)
                .or_else(|| option.default.clone());
            if let Some(value) = value {
                crate::native::validate_word(&value, count, total, limits)?;
                total += value.as_encoded_bytes().len();
                count += 1;
                let value = if option.value_mode() == ValueMode::Forbidden {
                    match value.to_str() {
                        Some("true") => None,
                        Some("false") => continue,
                        _ => {
                            return Err(CliError::new(
                                CliErrorKind::InvalidValue,
                                format!(
                                    "--{} environment/default must be true or false",
                                    option.long()
                                ),
                            ))
                        }
                    }
                } else {
                    Some(value)
                };
                self.push_option(option, value).map_err(|error| {
                    CliError::new(
                        error.kind(),
                        format!("invalid environment/default value for --{}", option.long()),
                    )
                })?;
            }
        }
        Ok(())
    }
    fn finish(self) -> Result<crate::ParsedOsCli, CliError> {
        if !has_terminal_option(&self) {
            validate_required_command(&self)?;
            validate_required_arguments(&self)?;
            validate_option_rules(&self)?;
        }
        Ok(crate::ParsedOsCli {
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
    words: &[std::ffi::OsString],
    index: usize,
    state: &mut ParseState<'_>,
) -> Result<usize, CliError> {
    let (prefix, inline) = crate::native::split_attached(&words[index], b'=');
    let name = prefix
        .to_str()
        .and_then(|s| s.strip_prefix("--"))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CliError::new(CliErrorKind::UnknownOption, "invalid long option name"))?;
    let option = state.find_long_option(name).ok_or_else(|| {
        let candidates = state
            .visible_options()
            .into_iter()
            .filter(|o| !o.is_hidden())
            .map(OptionSpec::long);
        CliError::new(
            CliErrorKind::UnknownOption,
            suggestion::append(
                format!("unknown option `--{name}`"),
                suggestion::closest(name, candidates),
                "--",
            ),
        )
    })?;
    let (value, next) = crate::native::option_value(words, index, inline, option, false)?;
    state.push_option(option, value)?;
    Ok(next)
}

fn parse_short_options(
    words: &[std::ffi::OsString],
    index: usize,
    state: &mut ParseState<'_>,
) -> Result<usize, CliError> {
    let bytes = words[index].as_encoded_bytes();
    let mut offset = 1;
    while offset < bytes.len() {
        let short = bytes[offset] as char;
        let option = state.find_short_option(short).ok_or_else(|| {
            CliError::new(
                CliErrorKind::UnknownOption,
                format!("unknown option `-{short}`"),
            )
        })?;
        offset += 1;
        let remainder = (offset < bytes.len()).then(|| {
            let start = offset + usize::from(bytes[offset] == b'=');
            crate::native::os_slice(&words[index], start, bytes.len()).to_os_string()
        });
        let (value, next) = crate::native::option_value(words, index, remainder, option, true)?;
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
    let visible = state.visible_options();
    let mut groups = HashSet::new();
    for option in &visible {
        if present.contains(option.long()) {
            if let Some(group) = option.exclusive_group.as_deref() {
                if !groups.insert(group) {
                    return Err(CliError::new(
                        CliErrorKind::OptionConflict,
                        format!("group `{group}` accepts at most one option"),
                    ));
                }
            }
        }
    }
    for option in visible {
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
            .find(|name| present.contains(name.as_str()) || groups.contains(name.as_str()))
        {
            return Err(CliError::new(
                CliErrorKind::OptionConflict,
                format!("option `--{}` conflicts with `--{conflict}`", option.long()),
            ));
        }
        if let Some(required) = option
            .requirements()
            .iter()
            .find(|name| !present.contains(name.as_str()) && !groups.contains(name.as_str()))
        {
            return Err(CliError::new(
                CliErrorKind::MissingRequirement,
                format!("option `--{}` requires `--{required}`", option.long()),
            ));
        }
    }
    Ok(())
}
