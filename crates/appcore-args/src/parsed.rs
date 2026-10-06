// =============================================================================
//        #######
//     ###       ###     F: parsed.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/05 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2
// =============================================================================

//! Parsed UTF-8 command-line results.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCli {
    pub(crate) command_path: Vec<String>,
    pub(crate) options: Vec<ParsedOption>,
    pub(crate) positionals: Vec<String>,
    pub(crate) passthrough: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ParsedOption {
    pub(crate) name: String,
    pub(crate) value: Option<String>,
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
    /// Returns a path from a textual value. Use `parse_os` for native non-UTF-8 paths.
    pub fn option_path(&self, name: &str) -> Option<std::path::PathBuf> {
        self.option_value(name).map(std::path::PathBuf::from)
    }
    /// Parses a numeric socket address without DNS resolution.
    pub fn option_socket_addr(
        &self,
        name: &str,
    ) -> Result<Option<std::net::SocketAddr>, crate::CliError> {
        self.option_value(name)
            .map(|v| {
                v.parse().map_err(|_| {
                    crate::CliError::new(
                        crate::CliErrorKind::InvalidValue,
                        format!("invalid socket address for --{name}"),
                    )
                })
            })
            .transpose()
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

impl std::fmt::Debug for ParsedOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedOption")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .finish()
    }
}
