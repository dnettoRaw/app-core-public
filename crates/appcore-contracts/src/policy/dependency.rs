// =============================================================================
//        #######
//     ###       ###     F: dependency.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Dependency on another application contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationDependency {
    application_id: ApplicationId,
    version_requirement: String,
    optional: bool,
}

impl ApplicationDependency {
    /// Creates an application dependency.
    pub fn new(
        application_id: ApplicationId,
        version_requirement: impl Into<String>,
        optional: bool,
    ) -> ContractResult<Self> {
        let dependency = Self {
            application_id,
            version_requirement: version_requirement.into(),
            optional,
        };
        dependency.validate()?;
        Ok(dependency)
    }

    /// Returns the depended-on application.
    pub fn application_id(&self) -> &ApplicationId {
        &self.application_id
    }

    /// Returns the version requirement expression.
    pub fn version_requirement(&self) -> &str {
        &self.version_requirement
    }

    /// Reports whether the dependency is optional.
    pub fn is_optional(&self) -> bool {
        self.optional
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        validate_text(
            "dependency.version_requirement",
            &self.version_requirement,
            128,
        )
    }
}
