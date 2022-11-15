// =============================================================================
//        #######
//     ###       ###     F: ids.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Small identifier types used by runtime public contracts.

use crate::error::{RuntimeError, RuntimeResult};

const MAX_IDENTIFIER_LEN: usize = 128;
const MAX_DISTRIBUTED_IDENTIFIER_LEN: usize = 80;

/// Validates a generic Runtime identifier.
pub fn validate_identifier(kind: &'static str, value: &str) -> RuntimeResult<()> {
    if value.is_empty() {
        return Err(RuntimeError::InvalidIdentifier {
            kind,
            value: value.to_string(),
            reason: "empty",
        });
    }
    if value.len() > MAX_IDENTIFIER_LEN {
        return Err(RuntimeError::InvalidIdentifier {
            kind,
            value: value.to_string(),
            reason: "too_long",
        });
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(RuntimeError::InvalidIdentifier {
            kind,
            value: value.to_string(),
            reason: "control_characters",
        });
    }
    if value.contains("..") {
        return Err(RuntimeError::InvalidIdentifier {
            kind,
            value: value.to_string(),
            reason: "path_traversal",
        });
    }
    if value
        .as_bytes()
        .iter()
        .any(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-')))
    {
        return Err(RuntimeError::InvalidIdentifier {
            kind,
            value: value.to_string(),
            reason: "invalid_char",
        });
    }
    Ok(())
}

/// Validates the stricter lowercase identifier used for distributed boundaries.
pub fn validate_distributed_identifier(kind: &'static str, value: &str) -> RuntimeResult<()> {
    if value.len() < 2 {
        return Err(RuntimeError::InvalidIdentifier {
            kind,
            value: value.to_string(),
            reason: "too_short",
        });
    }
    if value.len() > MAX_DISTRIBUTED_IDENTIFIER_LEN {
        return Err(RuntimeError::InvalidIdentifier {
            kind,
            value: value.to_string(),
            reason: "too_long",
        });
    }
    let Some(first) = value.as_bytes().first() else {
        return Err(RuntimeError::InvalidIdentifier {
            kind,
            value: value.to_string(),
            reason: "empty",
        });
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(RuntimeError::InvalidIdentifier {
            kind,
            value: value.to_string(),
            reason: "invalid_start",
        });
    }
    if value
        .as_bytes()
        .iter()
        .any(|b| !(b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-'))
    {
        return Err(RuntimeError::InvalidIdentifier {
            kind,
            value: value.to_string(),
            reason: "invalid_char",
        });
    }
    Ok(())
}

/// Unique identifier for an application.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct AppId(String);

/// Family/group identifier for compatible applications.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct AppFamily(String);

/// Sync isolation group (for example: dev, staging, production).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SyncGroup(String);

/// Public runtime contract version declared by an app/plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct RuntimeContractVersion(u16);

/// Wire protocol version used by distributed AppCore peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ProtocolVersion(u16);

impl RuntimeContractVersion {
    /// Creates an explicit Runtime contract version.
    pub fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the numeric contract version.
    pub fn as_u16(&self) -> u16 {
        self.0
    }
}

impl ProtocolVersion {
    /// Creates an explicit distributed protocol version.
    pub fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the numeric protocol version.
    pub fn as_u16(&self) -> u16 {
        self.0
    }

    /// Reports exact protocol compatibility.
    pub fn is_compatible_with(&self, other: ProtocolVersion) -> bool {
        self.0 == other.0
    }
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self(1)
    }
}

/// Unique node identifier in a runtime cluster.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

/// Stable command name (for example: runtime.start).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CommandName(String);

/// Stable event name (for example: RuntimeStarted).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct EventName(String);

/// Stable state name (for example: Running).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct StateName(String);

/// Stable query name (for example: runtime.status).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct QueryName(String);

/// Tenant isolation identifier for distributed AppCore deployments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct TenantId(String);

/// Cluster identifier inside a tenant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ClusterId(String);

/// Stable identifier for a logical Core.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CoreId(String);

/// Unique identifier for a running Core instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct InstanceId(String);

/// Generic capability name exposed by a Core.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CapabilityName(String);

macro_rules! impl_identifier {
    ($ty:ident, $kind:literal) => {
        impl $ty {
            /// Creates and validates an identifier.
            pub fn new(value: impl Into<String>) -> RuntimeResult<Self> {
                let value = value.into();
                validate_identifier($kind, &value)?;
                Ok(Self(value))
            }

            #[doc(hidden)]
            #[allow(dead_code)]
            pub(crate) fn unchecked(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Revalidates an identifier received from a serialized boundary.
            pub fn validate(&self) -> RuntimeResult<()> {
                validate_identifier($kind, &self.0)
            }

            /// Returns the identifier as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $ty {
            type Error = RuntimeError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $ty {
            type Error = RuntimeError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

macro_rules! impl_distributed_identifier {
    ($ty:ident, $kind:literal) => {
        impl $ty {
            /// Creates and validates a distributed identifier.
            pub fn new(value: impl Into<String>) -> RuntimeResult<Self> {
                let value = value.into();
                validate_distributed_identifier($kind, &value)?;
                Ok(Self(value))
            }

            #[doc(hidden)]
            #[allow(dead_code)]
            pub(crate) fn unchecked(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Revalidates an identifier received from a serialized boundary.
            pub fn validate(&self) -> RuntimeResult<()> {
                validate_distributed_identifier($kind, &self.0)
            }

            /// Returns the identifier as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $ty {
            type Error = RuntimeError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $ty {
            type Error = RuntimeError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

impl_identifier!(AppId, "AppId");
impl_identifier!(AppFamily, "AppFamily");
impl_identifier!(SyncGroup, "SyncGroup");
impl_identifier!(NodeId, "NodeId");
impl_identifier!(CommandName, "CommandName");
impl_identifier!(EventName, "EventName");
impl_identifier!(StateName, "StateName");
impl_identifier!(QueryName, "QueryName");
impl_distributed_identifier!(TenantId, "TenantId");
impl_distributed_identifier!(ClusterId, "ClusterId");
impl_distributed_identifier!(CoreId, "CoreId");
impl_distributed_identifier!(InstanceId, "InstanceId");
impl_identifier!(CapabilityName, "CapabilityName");
