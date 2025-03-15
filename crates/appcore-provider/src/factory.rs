// =============================================================================
//        #######
//     ###       ###     F: factory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{ProviderContext, ProviderError, ProviderResult, ProviderRole, SecretProvider};
use appcore_contracts::{ProviderConfig, ProviderId};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

/// Factory for one provider implementation and output interface.
pub trait ProviderFactory<T>: Send + Sync {
    /// Infrastructure role implemented by this factory.
    fn role(&self) -> ProviderRole;
    /// Stable provider identity selected in deployment manifests.
    fn provider_id(&self) -> &'static str;
    /// Validates configuration and constructs a provider instance.
    fn create(
        &self,
        config: &ProviderConfig,
        context: &ProviderContext,
        secrets: &dyn SecretProvider,
    ) -> ProviderResult<T>;
}

/// Registry of explicit provider factories for one output interface.
pub struct ProviderRegistry<T> {
    factories: BTreeMap<(ProviderRole, String), Arc<dyn ProviderFactory<T>>>,
}

impl<T> Debug for ProviderRegistry<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderRegistry")
            .field("factory_count", &self.factories.len())
            .finish()
    }
}

impl<T> Default for ProviderRegistry<T> {
    fn default() -> Self {
        Self {
            factories: BTreeMap::new(),
        }
    }
}

impl<T> ProviderRegistry<T> {
    /// Creates an empty provider registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one factory and rejects ambiguous duplicate ownership.
    pub fn register<F>(&mut self, factory: F) -> ProviderResult<()>
    where
        F: ProviderFactory<T> + 'static,
    {
        let key = (factory.role(), factory.provider_id().to_string());
        if self.factories.contains_key(&key) {
            return Err(ProviderError::DuplicateFactory {
                role: key.0,
                provider_id: key.1,
            });
        }
        self.factories.insert(key, Arc::new(factory));
        Ok(())
    }

    /// Constructs the provider explicitly selected by a deployment manifest.
    pub fn create(
        &self,
        role: ProviderRole,
        config: &ProviderConfig,
        context: &ProviderContext,
        secrets: &dyn SecretProvider,
    ) -> ProviderResult<T> {
        let key = (role, config.provider_id().as_str().to_string());
        let factory = self
            .factories
            .get(&key)
            .ok_or_else(|| ProviderError::Unavailable {
                role,
                provider_id: config.provider_id().as_str().to_string(),
            })?;
        factory.create(config, context, secrets)
    }

    /// Reports whether a role and provider ID are available.
    pub fn contains(&self, role: ProviderRole, provider_id: &ProviderId) -> bool {
        self.factories
            .contains_key(&(role, provider_id.as_str().to_string()))
    }
}
