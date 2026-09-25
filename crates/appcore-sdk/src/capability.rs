//! Application capability declarations and command coverage helpers.

use appcore_contracts::CapabilityId;
use appcore_core::{CommandName, RuntimeResult};
use serde::{Deserialize, Serialize};

use crate::{AppError, AppResult};

/// Maximum declarations and command mappings retained by one helper.
pub const MAX_CAPABILITY_ENTRIES: usize = 256;

/// Reusable result shape for authorization owned by the host or application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum CapabilityOutcome<T> {
    /// The caller was authenticated and authorized by the owning policy.
    Authorized(T),
    /// Authentication is required before the operation can continue.
    AuthenticationRequired,
    /// Authentication exists but the owning policy denied the operation.
    PermissionDenied,
}

impl<T> CapabilityOutcome<T> {
    /// Converts the outcome into a result without deciding authorization.
    pub fn into_result(self) -> Result<T, CapabilityOutcomeError> {
        match self {
            Self::Authorized(value) => Ok(value),
            Self::AuthenticationRequired => Err(CapabilityOutcomeError::AuthenticationRequired),
            Self::PermissionDenied => Err(CapabilityOutcomeError::PermissionDenied),
        }
    }
}

/// Uniform non-business authorization outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityOutcomeError {
    /// No valid authentication context was supplied.
    AuthenticationRequired,
    /// The owning authorization policy rejected the request.
    PermissionDenied,
}

/// Result of checking registered command coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityCoverage {
    /// Commands without a capability mapping.
    pub uncovered_commands: Vec<String>,
    /// Number of declared capabilities.
    pub declared_capabilities: usize,
    /// Number of command mappings.
    pub command_mappings: usize,
}

impl CapabilityCoverage {
    /// Returns whether every supplied command has a capability mapping.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.uncovered_commands.is_empty()
    }
}

/// Bounded declaration and command-to-capability mapping registry.
#[derive(Debug, Clone, Default)]
pub struct CapabilityRegistry {
    declarations: Vec<CapabilityDeclaration>,
    command_mappings: Vec<(CommandName, CapabilityId)>,
}

impl CapabilityRegistry {
    /// Creates an empty helper registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares one capability, rejecting duplicate identities and overflow.
    pub fn declare(&mut self, declaration: CapabilityDeclaration) -> AppResult<()> {
        if self.declarations.len() >= MAX_CAPABILITY_ENTRIES {
            return Err(AppError::Capability(
                "capability declaration limit exceeded".to_owned(),
            ));
        }
        if self
            .declarations
            .iter()
            .any(|existing| existing.id() == declaration.id())
        {
            return Err(AppError::Capability(format!(
                "capability `{}` is already declared",
                declaration.id().as_str()
            )));
        }
        self.declarations.push(declaration);
        Ok(())
    }

    /// Maps one command to a previously declared capability.
    pub fn map_command(&mut self, command: CommandName, capability: CapabilityId) -> AppResult<()> {
        if !self
            .declarations
            .iter()
            .any(|item| item.id() == &capability)
        {
            return Err(AppError::Capability(format!(
                "capability `{}` is not declared",
                capability.as_str()
            )));
        }
        if self.command_mappings.len() >= MAX_CAPABILITY_ENTRIES {
            return Err(AppError::Capability(
                "command mapping limit exceeded".to_owned(),
            ));
        }
        if self
            .command_mappings
            .iter()
            .any(|(existing, _)| existing == &command)
        {
            return Err(AppError::Capability(format!(
                "command `{}` is already mapped",
                command.as_str()
            )));
        }
        self.command_mappings.push((command, capability));
        Ok(())
    }

    /// Returns the immutable capability declarations.
    pub fn declarations(&self) -> &[CapabilityDeclaration] {
        &self.declarations
    }

    /// Returns the capability mapped to a command, when present.
    pub fn capability_for(&self, command: &CommandName) -> Option<&CapabilityId> {
        self.command_mappings
            .iter()
            .find(|(mapped, _)| mapped == command)
            .map(|(_, capability)| capability)
    }

    /// Checks that every registered command has a capability mapping.
    pub fn coverage(&self, commands: &[CommandName]) -> CapabilityCoverage {
        let uncovered_commands = commands
            .iter()
            .filter(|command| self.capability_for(command).is_none())
            .map(|command| command.as_str().to_owned())
            .collect();
        CapabilityCoverage {
            uncovered_commands,
            declared_capabilities: self.declarations.len(),
            command_mappings: self.command_mappings.len(),
        }
    }
}

/// Declares a capability and maps it to a command in one checked operation.
pub fn declare_command_capability(
    registry: &mut CapabilityRegistry,
    declaration: CapabilityDeclaration,
    command: CommandName,
) -> AppResult<()> {
    let capability = declaration.id().clone();
    registry.declare(declaration)?;
    registry.map_command(command, capability)
}

/// Creates a checked command name for helper and application registration code.
pub fn command_name(value: impl Into<String>) -> RuntimeResult<CommandName> {
    CommandName::new(value)
}

/// Re-export the contract types needed to construct declarations.
pub use appcore_contracts::{
    CapabilityClass, CapabilityDeclaration, CapabilityMode, CapabilityVisibility,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_mapping_and_coverage_are_bounded() {
        let id = CapabilityId::new("document.export").unwrap();
        let declaration = CapabilityDeclaration::new(
            id.clone(),
            "1",
            CapabilityMode::Command,
            CapabilityVisibility::Local,
        )
        .unwrap();
        let mut registry = CapabilityRegistry::new();
        declare_command_capability(
            &mut registry,
            declaration,
            command_name("document.export.run").unwrap(),
        )
        .unwrap();
        let commands = vec![
            command_name("document.export.run").unwrap(),
            command_name("document.export.preview").unwrap(),
        ];
        let coverage = registry.coverage(&commands);
        assert!(!coverage.is_complete());
        assert_eq!(coverage.uncovered_commands, vec!["document.export.preview"]);
        assert_eq!(registry.capability_for(&commands[0]), Some(&id));
    }

    #[test]
    fn authorization_outcomes_are_uniform_without_policy_decisions() {
        assert_eq!(
            CapabilityOutcome::<()>::AuthenticationRequired.into_result(),
            Err(CapabilityOutcomeError::AuthenticationRequired)
        );
        assert_eq!(CapabilityOutcome::Authorized(7).into_result(), Ok(7));
    }
}
