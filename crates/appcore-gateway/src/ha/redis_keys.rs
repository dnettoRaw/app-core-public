// =============================================================================
//        #######
//     ###       ###     F: redis_keys.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Single-slot Redis keys derived only from validated identities.

use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};

#[derive(Debug, Clone)]
pub(crate) struct RedisGatewayKeys {
    namespace: String,
}

impl RedisGatewayKeys {
    pub(crate) fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
        }
    }

    pub(crate) fn schema(&self) -> String {
        format!("{}:schema", self.namespace)
    }

    pub(crate) fn epoch(&self, tenant: &TenantId, instance: &InstanceId) -> String {
        format!("{}:epoch:{}", self.tenant_prefix(tenant), instance.as_str())
    }

    pub(crate) fn lease(&self, tenant: &TenantId, instance: &InstanceId) -> String {
        format!("{}:lease:{}", self.tenant_prefix(tenant), instance.as_str())
    }

    pub(crate) fn worker(&self, tenant: &TenantId, cluster: &ClusterId, core: &CoreId) -> String {
        format!(
            "{}:worker:{}:{}",
            self.tenant_prefix(tenant),
            cluster.as_str(),
            core.as_str()
        )
    }

    pub(crate) fn worker_capabilities(
        &self,
        tenant: &TenantId,
        cluster: &ClusterId,
        core: &CoreId,
    ) -> String {
        format!(
            "{}:worker-capabilities:{}:{}",
            self.tenant_prefix(tenant),
            cluster.as_str(),
            core.as_str()
        )
    }

    pub(crate) fn workers(&self, tenant: &TenantId) -> String {
        format!("{}:workers", self.tenant_prefix(tenant))
    }

    pub(crate) fn capability(&self, tenant: &TenantId, capability: &CapabilityName) -> String {
        format!(
            "{}:capability:{}",
            self.tenant_prefix(tenant),
            capability.as_str()
        )
    }

    pub(crate) fn session(&self, tenant: &TenantId, session_id: &str) -> String {
        format!("{}:session:{session_id}", self.tenant_prefix(tenant))
    }

    pub(crate) fn sessions(&self, tenant: &TenantId) -> String {
        format!("{}:sessions", self.tenant_prefix(tenant))
    }

    pub(crate) fn request(&self, tenant: &TenantId, request_id: &str) -> String {
        format!("{}:request:{request_id}", self.tenant_prefix(tenant))
    }

    pub(crate) fn requests(&self, tenant: &TenantId) -> String {
        format!("{}:requests", self.tenant_prefix(tenant))
    }

    fn tenant_prefix(&self, tenant: &TenantId) -> String {
        format!("{}:{{{}}}", self.namespace, tenant.as_str())
    }
}

#[cfg(test)]
#[path = "redis_keys_tests.rs"]
mod tests;
