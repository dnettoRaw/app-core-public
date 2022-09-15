// =============================================================================
//        #######
//     ###       ###     F: completion.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::spec::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec, ValueMode, ValueType};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionRequest {
    words: Vec<String>,
    cursor_word: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionCandidate {
    value: String,
    description: String,
    kind: CompletionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Command,
    Option,
    Value,
}

pub struct CompletionEngine<'a> {
    spec: &'a CliSpec,
}

impl CompletionRequest {
    pub fn new(words: Vec<String>, cursor_word: usize) -> Self {
        Self { words, cursor_word }
    }
    pub fn words(&self) -> &[String] {
        &self.words
    }
    pub fn cursor_word(&self) -> usize {
        self.cursor_word
    }
}

impl CompletionCandidate {
    pub fn new(
        value: impl Into<String>,
        description: impl Into<String>,
        kind: CompletionKind,
    ) -> Self {
        Self {
            value: value.into(),
            description: description.into(),
            kind,
        }
    }
    pub fn value(&self) -> &str {
        &self.value
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn kind(&self) -> CompletionKind {
        self.kind
    }
}

impl<'a> CompletionEngine<'a> {
    pub fn new(spec: &'a CliSpec) -> Self {
        Self { spec }
    }

    pub fn complete(&self, request: &CompletionRequest) -> Vec<CompletionCandidate> {
        if self.spec.validate().is_err() {
            return Vec::new();
        }
        let (words, cursor) = normalize_words(self.spec.name(), request);
        let prefix = words.get(cursor).map(String::as_str).unwrap_or("");
        let context = CompletionContext::analyze(self.spec, &words[..cursor.min(words.len())]);
        if context.passthrough {
            return Vec::new();
        }
        if let Some((name, value_prefix)) = long_inline_value(prefix) {
            return context
                .find_long_option(name)
                .map(|option| value_candidates(option.possible_values(), value_prefix, Some(name)))
                .unwrap_or_default();
        }
        if let Some(option) = context.pending_value {
            return value_candidates(option.possible_values(), prefix, None);
        }
        let mut candidates = Vec::new();
        if prefix.starts_with('-') {
            context.push_options(prefix, &mut candidates);
            context.push_argument_values(prefix, &mut candidates);
            return candidates;
        }
        context.push_commands(prefix, &mut candidates);
        context.push_argument_values(prefix, &mut candidates);
        candidates
    }
}

struct CompletionContext<'a> {
    spec: &'a CliSpec,
    commands: Vec<&'a CommandSpec>,
    used_options: HashSet<&'a str>,
    positionals: usize,
    pending_value: Option<&'a OptionSpec>,
    passthrough: bool,
}

impl<'a> CompletionContext<'a> {
    fn analyze(spec: &'a CliSpec, words: &[String]) -> Self {
        let mut context = Self {
            spec,
            commands: Vec::new(),
            used_options: HashSet::new(),
            positionals: 0,
            pending_value: None,
            passthrough: false,
        };
        for word in words {
            if context.consume(word) {
                break;
            }
        }
        context
    }

    fn consume(&mut self, word: &str) -> bool {
        if self.pending_value.take().is_some() {
            return false;
        }
        if word == "--" {
            self.passthrough = true;
            return true;
        }
        if let Some(raw) = word.strip_prefix("--") {
            self.consume_long(raw);
            return false;
        }
        if word.starts_with('-') && word.len() > 1 && !self.accepts_negative_positional(word) {
            self.consume_short(word);
            return false;
        }
        if self.positionals == 0 {
            if let Some(command) = self
                .available_commands()
                .iter()
                .find(|command| command.matches(word))
            {
                self.commands.push(command);
                return false;
            }
        }
        self.positionals += 1;
        false
    }

    fn consume_long(&mut self, raw: &str) {
        let (name, has_value) = raw
            .split_once('=')
            .map_or((raw, false), |(name, _)| (name, true));
        if let Some(option) = self.find_long_option(name) {
            self.used_options.insert(option.long());
            if option.value_mode() == ValueMode::Required && !has_value {
                self.pending_value = Some(option);
            }
        }
    }

    fn consume_short(&mut self, word: &str) {
        let raw = word.trim_start_matches('-');
        let mut chars = raw.char_indices().peekable();
        while let Some((_, short)) = chars.next() {
            let Some(option) = self.find_short_option(short) else {
                return;
            };
            self.used_options.insert(option.long());
            if option.value_mode() != ValueMode::Forbidden {
                if chars.peek().is_none() && option.value_mode() == ValueMode::Required {
                    self.pending_value = Some(option);
                }
                return;
            }
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

    fn next_argument(&self) -> Option<&'a ArgumentSpec> {
        let arguments = self.active_arguments();
        arguments
            .get(self.positionals)
            .or_else(|| arguments.last().filter(|argument| argument.is_multiple()))
    }

    fn accepts_negative_positional(&self, value: &str) -> bool {
        self.next_argument().is_some_and(|argument| {
            argument.value_type_kind() == ValueType::I64
                && value.parse::<i64>().is_ok()
                && value
                    .chars()
                    .nth(1)
                    .is_none_or(|short| self.find_short_option(short).is_none())
        })
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

    fn push_commands(&self, prefix: &str, candidates: &mut Vec<CompletionCandidate>) {
        if self.positionals > 0 {
            return;
        }
        for command in self
            .available_commands()
            .iter()
            .filter(|command| !command.is_hidden() && command.name().starts_with(prefix))
        {
            candidates.push(CompletionCandidate::new(
                command.name(),
                command.about_text(),
                CompletionKind::Command,
            ));
        }
    }

    fn push_options(&self, prefix: &str, candidates: &mut Vec<CompletionCandidate>) {
        for option in self
            .visible_options()
            .into_iter()
            .filter(|option| !option.is_hidden())
        {
            if !option.is_repeatable() && self.used_options.contains(option.long()) {
                continue;
            }
            let long = format!("--{}", option.long());
            if long.starts_with(prefix) {
                candidates.push(CompletionCandidate::new(
                    long,
                    option.about_text(),
                    CompletionKind::Option,
                ));
            }
            if let Some(short) = option.short_name() {
                let short = format!("-{short}");
                if short.starts_with(prefix) {
                    candidates.push(CompletionCandidate::new(
                        short,
                        option.about_text(),
                        CompletionKind::Option,
                    ));
                }
            }
        }
    }

    fn push_argument_values(&self, prefix: &str, candidates: &mut Vec<CompletionCandidate>) {
        if let Some(argument) = self.next_argument() {
            candidates.extend(value_candidates(argument.possible_values(), prefix, None));
        }
    }
}

fn normalize_words(binary: &str, request: &CompletionRequest) -> (Vec<String>, usize) {
    if request.words().first().is_some_and(|word| word == binary) {
        (
            request.words()[1..].to_vec(),
            request.cursor_word().saturating_sub(1),
        )
    } else {
        (request.words().to_vec(), request.cursor_word())
    }
}

fn long_inline_value(prefix: &str) -> Option<(&str, &str)> {
    prefix.strip_prefix("--")?.split_once('=')
}

fn value_candidates(
    values: &[String],
    prefix: &str,
    long_option: Option<&str>,
) -> Vec<CompletionCandidate> {
    values
        .iter()
        .filter(|value| value.starts_with(prefix))
        .map(|value| {
            let rendered = long_option
                .map(|name| format!("--{name}={value}"))
                .unwrap_or_else(|| value.clone());
            CompletionCandidate::new(rendered, "", CompletionKind::Value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{CompletionEngine, CompletionKind, CompletionRequest};
    use crate::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec};

    #[test]
    fn completes_nested_commands_and_inherited_options() {
        let spec = CliSpec::new("demo")
            .option(OptionSpec::flag("verbose"))
            .command(CommandSpec::new("publish").command(CommandSpec::new("status")));
        let nested = CompletionRequest::new(vec!["demo".into(), "publish".into(), "s".into()], 2);
        let options =
            CompletionRequest::new(vec!["demo".into(), "publish".into(), "--v".into()], 2);
        assert_eq!(
            CompletionEngine::new(&spec).complete(&nested)[0].value(),
            "status"
        );
        assert_eq!(
            CompletionEngine::new(&spec).complete(&options)[0].value(),
            "--verbose"
        );
    }

    #[test]
    fn completes_option_and_argument_values() {
        let spec = CliSpec::new("demo")
            .option(
                OptionSpec::value("color")
                    .possible_value("red")
                    .possible_value("green"),
            )
            .argument(ArgumentSpec::new("mode").possible_value("fast"));
        let option = CompletionRequest::new(vec!["demo".into(), "--color".into(), "g".into()], 2);
        let inline = CompletionRequest::new(vec!["demo".into(), "--color=r".into()], 1);
        let argument = CompletionRequest::new(vec!["demo".into(), "f".into()], 1);
        assert_eq!(
            CompletionEngine::new(&spec).complete(&option)[0].value(),
            "green"
        );
        assert_eq!(
            CompletionEngine::new(&spec).complete(&inline)[0].value(),
            "--color=red"
        );
        assert_eq!(
            CompletionEngine::new(&spec).complete(&argument)[0].kind(),
            CompletionKind::Value
        );
    }

    #[test]
    fn hides_hidden_and_consumed_non_repeatable_options() {
        let spec = CliSpec::new("demo")
            .option(OptionSpec::flag("visible"))
            .option(OptionSpec::flag("internal").hidden(true));
        let request =
            CompletionRequest::new(vec!["demo".into(), "--visible".into(), "--".into()], 2);
        assert!(CompletionEngine::new(&spec).complete(&request).is_empty());
    }

    #[test]
    fn invalid_specs_do_not_produce_candidates() {
        let spec = CliSpec::new("demo")
            .option(OptionSpec::flag("verbose"))
            .option(OptionSpec::flag("verbose"));
        let request = CompletionRequest::new(vec!["demo".into(), "--v".into()], 1);

        assert!(CompletionEngine::new(&spec).complete(&request).is_empty());
    }

    #[test]
    fn completes_declared_negative_positional_values() {
        let spec = CliSpec::new("demo").argument(
            ArgumentSpec::new("offset")
                .value_type(crate::ValueType::I64)
                .possible_value("-10"),
        );
        let request = CompletionRequest::new(vec!["demo".into(), "-1".into()], 1);

        let candidates = CompletionEngine::new(&spec).complete(&request);
        assert!(candidates
            .iter()
            .any(|candidate| candidate.value() == "-10"));
    }
}
