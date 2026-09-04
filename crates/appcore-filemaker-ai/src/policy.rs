// =============================================================================
//        #######
//     ###       ###     F: policy.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{BridgeError, BridgeResult};

/// Explicit tool and result budgets for one AI editing session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiBridgePolicy {
    /// Maximum calls in one session.
    pub max_tool_calls: usize,
    /// Maximum UTF-8 JSON argument bytes accepted by one call.
    pub max_argument_bytes: usize,
    /// Maximum operations accepted in one patch call.
    pub max_patch_operations: usize,
    /// Maximum serialized/base64 result bytes, counted without retaining a second JSON buffer.
    pub max_result_bytes: usize,
    /// Whether preview/export tools may encode artifact bytes.
    pub allow_artifact_bytes: bool,
    /// Whether `load` may replace an existing document and its AI policy.
    pub allow_document_replacement: bool,
    /// Exact allowed tool names; empty enables the standard set.
    pub allowed_tools: BTreeSet<String>,
}

impl Default for AiBridgePolicy {
    fn default() -> Self {
        Self {
            max_tool_calls: 128,
            max_argument_bytes: 64 * 1024,
            max_patch_operations: 32,
            max_result_bytes: 4 * 1024 * 1024,
            allow_artifact_bytes: true,
            allow_document_replacement: false,
            allowed_tools: BTreeSet::new(),
        }
    }
}

impl AiBridgePolicy {
    pub(crate) fn validate(&self) -> BridgeResult<()> {
        if self.max_tool_calls == 0
            || self.max_tool_calls > 1_000_000
            || self.max_argument_bytes == 0
            || self.max_argument_bytes > 1024 * 1024
            || self.max_patch_operations == 0
            || self.max_patch_operations > 1_024
            || self.max_result_bytes == 0
            || self.max_result_bytes > 64 * 1024 * 1024
            || self.allowed_tools.len() > 32
        {
            return Err(BridgeError::Policy(
                "bridge budgets are zero or outside supported bounds".to_owned(),
            ));
        }
        let definitions = crate::tools::tool_definitions();
        if self
            .allowed_tools
            .iter()
            .any(|name| !definitions.iter().any(|tool| tool.name == *name))
        {
            return Err(BridgeError::Policy(
                "allowed-tools policy contains an unknown tool".to_owned(),
            ));
        }
        Ok(())
    }

    pub(crate) fn allows(&self, name: &str) -> bool {
        self.allowed_tools.is_empty() || self.allowed_tools.contains(name)
    }
}
