// =============================================================================
//        #######
//     ###       ###     F: catalog.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::policy::enforce_requirements;
use crate::{CapabilityError, CapabilityRequest, CapabilityResult};
use appcore_contracts::ServiceId;
use appcore_core::{CapabilityDescriptor, CapabilityName, CoreIdentity};
use appcore_distributed_contracts::ServiceLeadershipGuard;
use std::collections::HashMap;

/// Runtime context used to authorize one local capability invocation.
pub struct CapabilityEnforcementContext<'a> {
    pub(crate) identity: &'a CoreIdentity,
    pub(crate) service_id: &'a ServiceId,
    pub(crate) leadership: Option<&'a dyn ServiceLeadershipGuard>,
    pub(crate) now_ms: u64,
    pub(crate) writes_allowed: bool,
}

impl<'a> CapabilityEnforcementContext<'a> {
    /// Creates a context without leadership and with writes enabled.
    pub fn new(identity: &'a CoreIdentity, service_id: &'a ServiceId, now_ms: u64) -> Self {
        Self {
            identity,
            service_id,
            leadership: None,
            now_ms,
            writes_allowed: true,
        }
    }

    /// Supplies the service-scoped leadership guard used for fenced writes.
    pub fn with_leadership(mut self, leadership: &'a dyn ServiceLeadershipGuard) -> Self {
        self.leadership = Some(leadership);
        self
    }

    /// Declares whether the host's current operational mode permits writes.
    pub fn with_writes_allowed(mut self, writes_allowed: bool) -> Self {
        self.writes_allowed = writes_allowed;
        self
    }
}

/// Immutable-source catalog of capability descriptors composed by a host.
#[derive(Debug, Clone, Default)]
pub struct CapabilityCatalog {
    descriptors: HashMap<CapabilityName, CapabilityDescriptor>,
}

impl CapabilityCatalog {
    /// Creates an empty descriptor catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a catalog and rejects duplicate capability names.
    pub fn from_descriptors(
        descriptors: impl IntoIterator<Item = CapabilityDescriptor>,
    ) -> CapabilityResult<Self> {
        let mut catalog = Self::new();
        for descriptor in descriptors {
            catalog.register_descriptor(descriptor)?;
        }
        Ok(catalog)
    }

    /// Registers one descriptor without attaching an executable handler.
    pub fn register_descriptor(
        &mut self,
        descriptor: CapabilityDescriptor,
    ) -> CapabilityResult<()> {
        if self.descriptors.contains_key(&descriptor.name) {
            return Err(CapabilityError::DescriptorAlreadyRegistered(
                descriptor.name.clone(),
            ));
        }
        self.descriptors.insert(descriptor.name.clone(), descriptor);
        Ok(())
    }

    /// Returns the declared descriptor for `capability`.
    pub fn descriptor(&self, capability: &CapabilityName) -> Option<&CapabilityDescriptor> {
        self.descriptors.get(capability)
    }

    /// Returns all descriptors in deterministic capability-name order.
    pub fn descriptors(&self) -> Vec<&CapabilityDescriptor> {
        let mut descriptors = self.descriptors.values().collect::<Vec<_>>();
        descriptors.sort_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
        descriptors
    }

    /// Resolves a locally declared descriptor and validates request semantics.
    pub fn resolve_local(
        &self,
        request: &CapabilityRequest,
    ) -> CapabilityResult<&CapabilityDescriptor> {
        let descriptor = self
            .descriptor(&request.capability)
            .ok_or_else(|| CapabilityError::CapabilityNotDeclared(request.capability.clone()))?;
        crate::policy::enforce_request_requirements(request, descriptor)?;
        Ok(descriptor)
    }

    /// Resolves and authorizes a local invocation against host and lease state.
    pub fn authorize_local(
        &self,
        request: &CapabilityRequest,
        context: CapabilityEnforcementContext<'_>,
    ) -> CapabilityResult<()> {
        let descriptor = self.resolve_local(request)?;
        enforce_requirements(
            context.identity,
            context.service_id,
            request,
            descriptor,
            &context.identity.core_id,
            context.leadership,
            context.writes_allowed,
            context.now_ms,
        )
    }
}
