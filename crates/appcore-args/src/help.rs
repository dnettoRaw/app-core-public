// =============================================================================
//        #######
//     ###       ###     F: help.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec, SpecError, ValueMode};

pub struct HelpRenderer<'a> {
    spec: &'a CliSpec,
    width: usize,
}

impl<'a> HelpRenderer<'a> {
    pub fn new(spec: &'a CliSpec) -> Self {
        Self { spec, width: 100 }
    }

    pub fn width(mut self, width: usize) -> Self {
        self.width = width.max(40);
        self
    }

    pub fn render(&self, command_path: &[&str]) -> Result<String, SpecError> {
        self.spec.validate()?;
        let resolved = find_commands(self.spec, command_path)?;
        let command = resolved.last().copied();
        let name = full_name(self.spec.name(), command_path);
        let mut output = heading(self.spec, command, &name);
        output.push_str("Usage:\n  ");
        output.push_str(&usage(&name, self.spec, command, &resolved));
        output.push('\n');
        let commands = command
            .map(CommandSpec::commands)
            .unwrap_or_else(|| self.spec.commands());
        render_commands(&mut output, commands, self.width);
        let arguments = command
            .map(CommandSpec::arguments)
            .unwrap_or_else(|| self.spec.arguments());
        render_arguments(&mut output, arguments, self.width);
        render_options(
            &mut output,
            visible_options(self.spec, &resolved),
            self.width,
        );
        Ok(output)
    }
}

fn heading(spec: &CliSpec, command: Option<&CommandSpec>, name: &str) -> String {
    let mut output = name.to_string();
    if let Some(version) = spec.version_text() {
        output.push(' ');
        output.push_str(version);
    }
    output.push('\n');
    let about = command
        .map(CommandSpec::about_text)
        .unwrap_or_else(|| spec.about_text());
    if !about.is_empty() {
        output.push_str(about);
        output.push_str("\n\n");
    }
    output
}

fn find_commands<'a>(spec: &'a CliSpec, path: &[&str]) -> Result<Vec<&'a CommandSpec>, SpecError> {
    let mut resolved = Vec::new();
    for name in path {
        let commands = resolved
            .last()
            .copied()
            .map(CommandSpec::commands)
            .unwrap_or_else(|| spec.commands());
        let command = commands
            .iter()
            .find(|command| command.matches(name))
            .ok_or_else(|| {
                SpecError::new_internal(format!("unknown help command `{}`", path.join(" ")))
            })?;
        resolved.push(command);
    }
    Ok(resolved)
}

fn full_name(binary: &str, path: &[&str]) -> String {
    if path.is_empty() {
        binary.to_string()
    } else {
        format!("{binary} {}", path.join(" "))
    }
}

fn usage(
    name: &str,
    spec: &CliSpec,
    command: Option<&CommandSpec>,
    resolved: &[&CommandSpec],
) -> String {
    let mut usage = name.to_string();
    if !visible_options(spec, resolved).is_empty() {
        usage.push_str(" [OPTIONS]");
    }
    let commands = command
        .map(CommandSpec::commands)
        .unwrap_or_else(|| spec.commands());
    let required = command
        .map(CommandSpec::is_command_required)
        .unwrap_or_else(|| spec.is_command_required());
    if !commands.is_empty() {
        usage.push_str(if required { " <COMMAND>" } else { " [COMMAND]" });
    }
    let arguments = command
        .map(CommandSpec::arguments)
        .unwrap_or_else(|| spec.arguments());
    for argument in arguments {
        usage.push(' ');
        usage.push_str(&argument_usage(argument));
    }
    usage
}

fn argument_usage(argument: &ArgumentSpec) -> String {
    let suffix = if argument.is_multiple() { "..." } else { "" };
    if argument.is_required() {
        format!("<{}{suffix}>", argument.name())
    } else {
        format!("[{}{suffix}]", argument.name())
    }
}

fn visible_options<'a>(spec: &'a CliSpec, commands: &[&'a CommandSpec]) -> Vec<&'a OptionSpec> {
    let mut options = spec.options().iter().collect::<Vec<_>>();
    for command in commands {
        options.extend(command.options());
    }
    options
}

fn render_commands(output: &mut String, commands: &[CommandSpec], width: usize) {
    let rows = commands
        .iter()
        .filter(|command| !command.is_hidden())
        .map(|command| (command.name().to_string(), command.about_text()))
        .collect::<Vec<_>>();
    render_rows(output, "Commands", rows, width);
}

fn render_arguments(output: &mut String, arguments: &[ArgumentSpec], width: usize) {
    let rows = arguments
        .iter()
        .map(|argument| (argument_usage(argument), argument.about_text()))
        .collect::<Vec<_>>();
    render_rows(output, "Arguments", rows, width);
}

fn render_options(output: &mut String, options: Vec<&OptionSpec>, width: usize) {
    let rows = options
        .into_iter()
        .filter(|option| !option.is_hidden())
        .map(|option| (option_usage(option), option.about_text()))
        .collect::<Vec<_>>();
    render_rows(output, "Options", rows, width);
}

fn option_usage(option: &OptionSpec) -> String {
    let mut usage = option
        .short_name()
        .map(|short| format!("-{short}, "))
        .unwrap_or_default();
    usage.push_str("--");
    usage.push_str(option.long());
    match option.value_mode() {
        ValueMode::Forbidden => {}
        ValueMode::Required => usage.push_str(&format!(" <{}>", option.value_name_text())),
        ValueMode::Optional => usage.push_str(&format!("[=<{}>]", option.value_name_text())),
    }
    usage
}

fn render_rows(output: &mut String, title: &str, rows: Vec<(String, &str)>, width: usize) {
    if rows.is_empty() {
        return;
    }
    output.push('\n');
    output.push_str(title);
    output.push_str(":\n");
    let label_width = rows
        .iter()
        .map(|(label, _)| label.len())
        .max()
        .unwrap_or(0)
        .min(width / 2);
    for (label, about) in rows {
        output.push_str("  ");
        output.push_str(&label);
        output.push_str(&" ".repeat(label_width.saturating_sub(label.len()) + 2));
        output.push_str(about);
        output.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::HelpRenderer;
    use crate::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec};

    #[test]
    fn renders_root_and_command_help() {
        let spec = CliSpec::new("demo")
            .version("1.0.0")
            .about("Demo tool.")
            .option(OptionSpec::flag("help").short('h').about("Show help."))
            .command(
                CommandSpec::new("run")
                    .about("Run it.")
                    .argument(ArgumentSpec::new("file").required(true)),
            );
        let root = HelpRenderer::new(&spec).render(&[]).unwrap();
        let command = HelpRenderer::new(&spec).render(&["run"]).unwrap();
        assert!(root.contains("demo 1.0.0"));
        assert!(root.contains("Commands:"));
        assert!(command.contains("demo run [OPTIONS] <file>"));
    }

    #[test]
    fn nested_help_includes_inherited_options() {
        let spec = CliSpec::new("demo").command(
            CommandSpec::new("publish")
                .option(OptionSpec::flag("dry-run"))
                .command(CommandSpec::new("status")),
        );

        let help = HelpRenderer::new(&spec)
            .render(&["publish", "status"])
            .unwrap();

        assert!(help.contains("demo publish status [OPTIONS]"));
        assert!(help.contains("--dry-run"));
    }
}
