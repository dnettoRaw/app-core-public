// =============================================================================
//        #######
//     ###       ###     F: identifiers.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 23:21:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Validated identifiers shared by manifests.

use crate::{ContractError, ContractResult};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

pub(crate) fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> ContractResult<()> {
    if value.trim().is_empty() {
        return Err(ContractError::Empty { field });
    }
    if value.len() > max_bytes {
        return Err(ContractError::TooLong { field, max_bytes });
    }
    if value.chars().any(char::is_control) {
        return Err(ContractError::InvalidValue {
            field,
            reason: "control characters are not allowed",
        });
    }
    Ok(())
}

pub(crate) fn validate_identifier(field: &'static str, value: &str) -> ContractResult<()> {
    validate_text(field, value, 128)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(ContractError::InvalidIdentifier { field });
    };
    if !first.is_ascii_alphanumeric()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:+".contains(character))
        || !value
            .chars()
            .last()
            .is_some_and(|character| character.is_ascii_alphanumeric())
    {
        return Err(ContractError::InvalidIdentifier { field });
    }
    Ok(())
}

pub(crate) fn is_sensitive_key(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    [
        "secret",
        "password",
        "passwd",
        "token",
        "private_key",
        "credential",
    ]
    .iter()
    .any(|fragment| normalized.contains(fragment))
}

pub(crate) fn looks_like_local_path(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || value.as_bytes().get(1) == Some(&b':')
}

pub(crate) fn looks_like_url(value: &str) -> bool {
    value.contains("://")
}

macro_rules! define_identifier {
    ($name:ident, $field:literal, $docs:literal) => {
        #[doc = $docs]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates a validated identifier.
            pub fn new(value: impl Into<String>) -> ContractResult<Self> {
                let value = value.into();
                validate_identifier($field, &value)?;
                Ok(Self(value))
            }

            /// Returns the identifier as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consumes the identifier and returns the owned string.
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = ContractError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

define_identifier!(
    ApplicationId,
    "application_id",
    "Stable identity of an application."
);
define_identifier!(
    ServiceId,
    "service_id",
    "Identity of an independently coordinated service."
);
define_identifier!(
    CapabilityId,
    "capability_id",
    "Generic capability name resolved by the runtime."
);
define_identifier!(
    ProviderId,
    "provider_id",
    "Identity of a deployment provider or adapter."
);
define_identifier!(
    NodeId,
    "node_id",
    "Identity of the physical or virtual runtime node."
);
define_identifier!(
    CoreId,
    "core_id",
    "Identity of one executable core hosted on a node."
);
define_identifier!(
    InstallationId,
    "installation_id",
    "Identity of one application installation."
);
define_identifier!(ModuleId, "module_id", "Identity of an application module.");
define_identifier!(JobId, "job_id", "Identity of a provider-owned runtime job.");
define_identifier!(
    FeatureId,
    "feature_id",
    "Identity of a declared runtime or application feature."
);
define_identifier!(
    BuildId,
    "build_id",
    "Identity of an immutable runtime or application build."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_reject_paths_and_whitespace() {
        assert!(ApplicationId::new("app.example").is_ok());
        assert!(ApplicationId::new("../app").is_err());
        assert!(ApplicationId::new("app example").is_err());
    }
}
