// =============================================================================
//        #######
//     ###       ###     F: registry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{
    CapabilityError, CapabilityRequest, CapabilityResponse, CapabilityResult,
    LocalCapabilityHandler,
};
use appcore_core::{CapabilityDescriptor, CapabilityName};
use std::collections::HashMap;
use std::sync::Arc;

/// Registered local capability descriptor and its handler.
#[derive(Clone)]
pub struct LocalCapabilityProvider {
    descriptor: CapabilityDescriptor,
    handler: Arc<dyn LocalCapabilityHandler>,
}

impl std::fmt::Debug for LocalCapabilityProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalCapabilityProvider")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl LocalCapabilityProvider {
    pub(crate) fn new(
        descriptor: CapabilityDescriptor,
        handler: Arc<dyn LocalCapabilityHandler>,
    ) -> Self {
        Self {
            descriptor,
            handler,
        }
    }

    /// Returns the immutable descriptor advertised by this provider.
    pub fn descriptor(&self) -> &CapabilityDescriptor {
        &self.descriptor
    }

    /// Reports the current handler health state.
    pub fn is_healthy(&self) -> bool {
        self.handler.is_healthy()
    }

    /// Delegates a request to the registered handler.
    pub fn handle(&self, request: &CapabilityRequest) -> CapabilityResult<CapabilityResponse> {
        self.handler.handle(request)
    }
}

/// In-process registry of local capability handlers.
#[derive(Debug, Clone, Default)]
pub struct CapabilityRegistry {
    local: HashMap<CapabilityName, LocalCapabilityProvider>,
}

impl CapabilityRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an owned local handler.
    pub fn register_handler<H>(&mut self, handler: H) -> CapabilityResult<()>
    where
        H: LocalCapabilityHandler + 'static,
    {
        self.register_shared_handler(Arc::new(handler))
    }

    /// Registers a shared local handler.
    pub fn register_shared_handler(
        &mut self,
        handler: Arc<dyn LocalCapabilityHandler>,
    ) -> CapabilityResult<()> {
        let descriptor = handler.descriptor();
        if self.local.contains_key(&descriptor.name) {
            return Err(CapabilityError::HandlerAlreadyRegistered(
                descriptor.name.clone(),
            ));
        }
        self.local.insert(
            descriptor.name.clone(),
            LocalCapabilityProvider::new(descriptor, handler),
        );
        Ok(())
    }

    /// Returns the local provider registered for a capability.
    pub fn get(&self, capability: &CapabilityName) -> Option<&LocalCapabilityProvider> {
        self.local.get(capability)
    }

    /// Returns descriptors for all registered local providers.
    pub fn descriptors(&self) -> Vec<CapabilityDescriptor> {
        self.local
            .values()
            .map(|provider| provider.descriptor.clone())
            .collect()
    }
}
