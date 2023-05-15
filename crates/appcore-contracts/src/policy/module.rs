// =============================================================================
//        #######
//     ###       ###     F: module.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Application module declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleDeclaration {
    id: ModuleId,
    version: String,
    required: bool,
}

impl ModuleDeclaration {
    /// Creates an application module declaration.
    pub fn new(id: ModuleId, version: impl Into<String>, required: bool) -> ContractResult<Self> {
        let module = Self {
            id,
            version: version.into(),
            required,
        };
        module.validate()?;
        Ok(module)
    }

    /// Returns the module identity.
    pub fn id(&self) -> &ModuleId {
        &self.id
    }

    /// Returns the module version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Reports whether the module is required.
    pub fn is_required(&self) -> bool {
        self.required
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        validate_text("module.version", &self.version, 64)
    }
}
