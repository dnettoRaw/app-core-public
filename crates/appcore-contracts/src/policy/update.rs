// =============================================================================
//        #######
//     ###       ###     F: update.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Application update preferences, independent of update providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePolicy {
    channel: String,
    automatic: bool,
}

impl UpdatePolicy {
    /// Creates an update policy.
    pub fn new(channel: impl Into<String>, automatic: bool) -> ContractResult<Self> {
        let policy = Self {
            channel: channel.into(),
            automatic,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Returns the application-selected update channel.
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Reports whether automatic updates are allowed.
    pub fn is_automatic(&self) -> bool {
        self.automatic
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        validate_text("update.channel", &self.channel, 64)
    }
}
