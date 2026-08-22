// =============================================================================
//        #######
//     ###       ###     F: id.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiError, AiResult};
use std::fmt::{Display, Formatter};

const MAX_ID_BYTES: usize = 96;

fn validate_id(value: &str) -> AiResult<()> {
    if value.is_empty() || value.len() > MAX_ID_BYTES {
        return Err(AiError::InvalidInput("identifier length"));
    }
    if value.starts_with('/')
        || value.ends_with('/')
        || value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(AiError::InvalidInput("identifier characters"));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/' | b':')
    }) {
        return Err(AiError::InvalidInput("identifier characters"));
    }
    Ok(())
}

macro_rules! bounded_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Validates and owns an identifier.
            pub fn new(value: impl Into<String>) -> AiResult<Self> {
                let value = value.into();
                validate_id(&value)?;
                Ok(Self(value))
            }

            /// Returns the validated identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

bounded_id!(ModelId, "Validated backend-neutral model identity.");
bounded_id!(
    BackendId,
    "Validated inference or training backend identity."
);
bounded_id!(
    DeviceId,
    "Validated local or remote compute device identity."
);
bounded_id!(
    CapabilityId,
    "Validated application-independent AI capability identity."
);
bounded_id!(PeerId, "Validated backend-neutral swarm peer identity.");
