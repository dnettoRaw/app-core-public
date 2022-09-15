// =============================================================================
//        #######
//     ###       ###     F: spec.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliSpec {
    name: String,
    about: String,
    version: Option<String>,
    commands: Vec<CommandSpec>,
    options: Vec<OptionSpec>,
    arguments: Vec<ArgumentSpec>,
    command_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    name: String,
    aliases: Vec<String>,
    about: String,
    commands: Vec<CommandSpec>,
    options: Vec<OptionSpec>,
    arguments: Vec<ArgumentSpec>,
    command_required: bool,
    hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionSpec {
    long: String,
    short: Option<char>,
    value: ValueMode,
    value_name: String,
    value_type: ValueType,
    possible_values: Vec<String>,
    about: String,
    required: bool,
    repeatable: bool,
    detached_optional_value: bool,
    terminal: bool,
    hidden: bool,
    conflicts_with: Vec<String>,
    requires: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgumentSpec {
    name: String,
    about: String,
    value_type: ValueType,
    possible_values: Vec<String>,
    required: bool,
    multiple: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueMode {
    Forbidden,
    Required,
    Optional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    String,
    Bool,
    I64,
    U64,
}

impl fmt::Display for ValueType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::String => "text",
            Self::Bool => "true or false",
            Self::I64 => "a signed integer",
            Self::U64 => "an unsigned integer",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecError {
    message: String,
}

impl CliSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            about: String::new(),
            version: None,
            commands: Vec::new(),
            options: Vec::new(),
            arguments: Vec::new(),
            command_required: false,
        }
    }
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = about.into();
        self
    }
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
    pub fn command(mut self, command: CommandSpec) -> Self {
        self.commands.push(command);
        self
    }
    pub fn option(mut self, option: OptionSpec) -> Self {
        self.options.push(option);
        self
    }
    pub fn argument(mut self, argument: ArgumentSpec) -> Self {
        self.arguments.push(argument);
        self
    }
    pub fn command_required(mut self, required: bool) -> Self {
        self.command_required = required;
        self
    }
    pub fn validate(&self) -> Result<(), SpecError> {
        crate::spec_validation::validate_spec(self)
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn about_text(&self) -> &str {
        &self.about
    }
    pub fn version_text(&self) -> Option<&str> {
        self.version.as_deref()
    }
    pub fn commands(&self) -> &[CommandSpec] {
        &self.commands
    }
    pub fn options(&self) -> &[OptionSpec] {
        &self.options
    }
    pub fn arguments(&self) -> &[ArgumentSpec] {
        &self.arguments
    }
    pub fn is_command_required(&self) -> bool {
        self.command_required
    }
}

impl CommandSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            aliases: Vec::new(),
            about: String::new(),
            commands: Vec::new(),
            options: Vec::new(),
            arguments: Vec::new(),
            command_required: false,
            hidden: false,
        }
    }
    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = about.into();
        self
    }
    pub fn command(mut self, command: CommandSpec) -> Self {
        self.commands.push(command);
        self
    }
    pub fn option(mut self, option: OptionSpec) -> Self {
        self.options.push(option);
        self
    }
    pub fn argument(mut self, argument: ArgumentSpec) -> Self {
        self.arguments.push(argument);
        self
    }
    pub fn command_required(mut self, required: bool) -> Self {
        self.command_required = required;
        self
    }
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }
    pub fn matches(&self, value: &str) -> bool {
        self.name == value || self.aliases.iter().any(|alias| alias == value)
    }
    pub fn about_text(&self) -> &str {
        &self.about
    }
    pub fn commands(&self) -> &[CommandSpec] {
        &self.commands
    }
    pub fn options(&self) -> &[OptionSpec] {
        &self.options
    }
    pub fn arguments(&self) -> &[ArgumentSpec] {
        &self.arguments
    }
    pub fn is_command_required(&self) -> bool {
        self.command_required
    }
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
}

impl OptionSpec {
    pub fn flag(long: impl Into<String>) -> Self {
        Self::new(long, ValueMode::Forbidden)
    }
    pub fn value(long: impl Into<String>) -> Self {
        Self::new(long, ValueMode::Required)
    }
    fn new(long: impl Into<String>, value: ValueMode) -> Self {
        Self {
            long: long.into(),
            short: None,
            value,
            value_name: "VALUE".into(),
            value_type: ValueType::String,
            possible_values: Vec::new(),
            about: String::new(),
            required: false,
            repeatable: false,
            detached_optional_value: false,
            terminal: false,
            hidden: false,
            conflicts_with: Vec::new(),
            requires: Vec::new(),
        }
    }
    pub fn short(mut self, short: char) -> Self {
        self.short = Some(short);
        self
    }
    pub fn optional_value(mut self) -> Self {
        self.value = ValueMode::Optional;
        self
    }
    pub fn value_name(mut self, name: impl Into<String>) -> Self {
        self.value_name = name.into();
        self
    }
    pub fn value_type(mut self, value_type: ValueType) -> Self {
        self.value_type = value_type;
        self
    }
    pub fn possible_value(mut self, value: impl Into<String>) -> Self {
        self.possible_values.push(value.into());
        self
    }
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = about.into();
        self
    }
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }
    pub fn repeatable(mut self, repeatable: bool) -> Self {
        self.repeatable = repeatable;
        self
    }
    /// Allows an optional value to be supplied as the following argument.
    ///
    /// Optional values remain attached-only by default because a detached text
    /// value may otherwise consume a positional argument. Prefer a bounded
    /// [`ValueType`] such as [`ValueType::Bool`] when enabling this behavior.
    pub fn detached_optional_value(mut self, enabled: bool) -> Self {
        self.detached_optional_value = enabled;
        self
    }
    pub fn terminal(mut self, terminal: bool) -> Self {
        self.terminal = terminal;
        self
    }
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }
    pub fn conflicts_with(mut self, long: impl Into<String>) -> Self {
        self.conflicts_with.push(long.into());
        self
    }
    pub fn requires(mut self, long: impl Into<String>) -> Self {
        self.requires.push(long.into());
        self
    }
    pub fn long(&self) -> &str {
        &self.long
    }
    pub fn short_name(&self) -> Option<char> {
        self.short
    }
    pub fn value_mode(&self) -> ValueMode {
        self.value
    }
    pub fn value_name_text(&self) -> &str {
        &self.value_name
    }
    pub fn value_type_kind(&self) -> ValueType {
        self.value_type
    }
    pub fn possible_values(&self) -> &[String] {
        &self.possible_values
    }
    pub fn about_text(&self) -> &str {
        &self.about
    }
    pub fn is_required(&self) -> bool {
        self.required
    }
    pub fn is_repeatable(&self) -> bool {
        self.repeatable
    }
    pub fn accepts_detached_optional_value(&self) -> bool {
        self.detached_optional_value
    }
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
    pub fn conflicts(&self) -> &[String] {
        &self.conflicts_with
    }
    pub fn requirements(&self) -> &[String] {
        &self.requires
    }
}

impl ArgumentSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            about: String::new(),
            value_type: ValueType::String,
            possible_values: Vec::new(),
            required: false,
            multiple: false,
        }
    }
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = about.into();
        self
    }
    pub fn value_type(mut self, value_type: ValueType) -> Self {
        self.value_type = value_type;
        self
    }
    pub fn possible_value(mut self, value: impl Into<String>) -> Self {
        self.possible_values.push(value.into());
        self
    }
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn about_text(&self) -> &str {
        &self.about
    }
    pub fn value_type_kind(&self) -> ValueType {
        self.value_type
    }
    pub fn possible_values(&self) -> &[String] {
        &self.possible_values
    }
    pub fn is_required(&self) -> bool {
        self.required
    }
    pub fn is_multiple(&self) -> bool {
        self.multiple
    }
}

impl SpecError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
    pub(crate) fn new_internal(message: impl Into<String>) -> Self {
        Self::new(message)
    }
}
impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for SpecError {}
