// =============================================================================
//        #######
//     ###       ###     F: shell.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellScriptError {
    message: String,
}

impl Shell {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            "powershell" | "pwsh" => Some(Self::PowerShell),
            _ => None,
        }
    }
}

impl fmt::Display for Shell {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::PowerShell => "powershell",
        })
    }
}

pub fn render_dynamic_completion_script(
    binary: &str,
    completion_command: &[&str],
    shell: Shell,
) -> Result<String, ShellScriptError> {
    validate_command_token("binary", binary)?;
    for part in completion_command {
        validate_command_token("completion command", part)?;
    }
    Ok(match shell {
        Shell::Bash => bash_script(binary, completion_command),
        Shell::Zsh => zsh_script(binary, completion_command),
        Shell::Fish => fish_script(binary, completion_command),
        Shell::PowerShell => powershell_script(binary, completion_command),
    })
}

impl fmt::Display for ShellScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ShellScriptError {}

fn validate_command_token(kind: &str, value: &str) -> Result<(), ShellScriptError> {
    let valid = !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(ShellScriptError {
            message: format!("unsafe {kind} token `{value}`"),
        })
    }
}

fn command_prefix(binary: &str, completion_command: &[&str]) -> String {
    let mut parts = vec![binary.to_string()];
    parts.extend(completion_command.iter().map(|part| part.to_string()));
    parts.join(" ")
}

fn bash_script(binary: &str, completion_command: &[&str]) -> String {
    let function = function_name(binary);
    let prefix = command_prefix(binary, completion_command);
    format!(
        r#"{function}() {{
    local IFS=$'\n'
    COMPREPLY=( $({prefix} bash "$COMP_CWORD" "${{COMP_WORDS[@]}}") )
}}
complete -o bashdefault -o default -F {function} {binary}
"#
    )
}

fn zsh_script(binary: &str, completion_command: &[&str]) -> String {
    let function = function_name(binary);
    let prefix = command_prefix(binary, completion_command);
    format!(
        r#"{function}() {{
    local -a completions
    completions=("${{(@f)$({prefix} zsh "$((CURRENT - 1))" "${{words[@]}}")}}")
    if (( ${{#completions}} )); then
        compadd -- "${{completions[@]}}"
    else
        _files
    fi
}}
compdef {function} {binary}
"#
    )
}

fn fish_script(binary: &str, completion_command: &[&str]) -> String {
    let prefix = command_prefix(binary, completion_command);
    format!(
        "complete -c {binary} -a '(set -l words (commandline -opc); set -a words (commandline -ct); set -l cursor (math (count $words) - 1); {prefix} fish $cursor $words)'\n"
    )
}

fn powershell_script(binary: &str, completion_command: &[&str]) -> String {
    let prefix = command_prefix(binary, completion_command);
    format!(
        r#"Register-ArgumentCompleter -Native -CommandName '{binary}' -ScriptBlock {{
    param($wordToComplete, $commandAst, $cursorPosition)
    $words = @($commandAst.CommandElements | ForEach-Object {{ $_.ToString() }})
    $cursorWord = [Math]::Max(0, $words.Count - 1)
    $custom = @(& {prefix} powershell $cursorWord @words)
    if ($custom.Count -gt 0) {{
        $custom | ForEach-Object {{ [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }}
    }} else {{
        Get-ChildItem -Name -Path "$wordToComplete*" -ErrorAction SilentlyContinue |
            ForEach-Object {{ [System.Management.Automation.CompletionResult]::new($_, $_, 'ProviderItem', $_) }}
    }}
}}
"#
    )
}

fn function_name(binary: &str) -> String {
    let normalized = binary
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("_{normalized}_complete")
}

#[cfg(test)]
mod tests {
    use super::{render_dynamic_completion_script, Shell};

    #[test]
    fn renders_bash_script_for_binary() {
        let script =
            render_dynamic_completion_script("appcore-dev", &["complete"], Shell::Bash).unwrap();

        assert!(script
            .contains("complete -o bashdefault -o default -F _appcore_dev_complete appcore-dev"));
    }

    #[test]
    fn generated_scripts_preserve_native_file_completion() {
        let zsh = render_dynamic_completion_script("demo", &["complete"], Shell::Zsh).unwrap();
        let fish = render_dynamic_completion_script("demo", &["complete"], Shell::Fish).unwrap();
        let powershell =
            render_dynamic_completion_script("demo", &["complete"], Shell::PowerShell).unwrap();

        assert!(zsh.contains("_files"));
        assert!(!fish.contains(" -f "));
        assert!(powershell.contains("Get-ChildItem"));
    }

    #[test]
    fn rejects_shell_metacharacters_in_command_tokens() {
        let error =
            render_dynamic_completion_script("appcore-dev;echo", &["complete"], Shell::PowerShell)
                .unwrap_err();

        assert!(error.to_string().contains("unsafe binary token"));
    }
}
