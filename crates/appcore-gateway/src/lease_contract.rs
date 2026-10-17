// =============================================================================
//        #######
//     ###       ###     F: lease_contract.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:42:56 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 2.0.0-alpha.6
// =============================================================================

//! Explicit request-bound service authority adapter contracts.
use appcore_contracts::ServiceId;
use appcore_distributed_contracts::control_plane::v1::ServiceLeaderLease;
use appcore_security::{compute_request_hash, RequestValidationDetails};
use appcore_types::CoreIdentity;
use serde::{Deserialize, Serialize};

/// Optional composition route; absent from the default Gateway router.
pub const GATEWAY_SERVICE_LEASE_PATH: &str = "/v1/gateway/service-lease";
/// Maximum encoded request accepted by the adapter.
pub const MAX_SERVICE_LEASE_REQUEST_BYTES: usize = 16 * 1024;
/// Exact operation, bound together with identity and service in the signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServiceLeaseOperationV1 {
    /// Acquire or observe the same live holder without extending its expiry.
    Acquire {
        /// Requested lifetime, 1..30000 milliseconds.
        ttl_ms: u64,
    },
    /// Renew an exact current unexpired grant.
    Renew {
        /// Current grant.
        lease: ServiceLeaderLease,
        /// Requested lifetime.
        ttl_ms: u64,
    },
    /// Release an exact live grant.
    Release {
        /// Current grant.
        lease: ServiceLeaderLease,
    },
    /// Read-only authority observation, not write permission.
    Inspect,
    /// Check the exact current grant; storage must still enforce fencing at commit.
    Check {
        /// Grant to validate.
        lease: ServiceLeaderLease,
    },
}
/// Authenticated issuer must approve every identity/service/operation before signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceLeaseRequestV1 {
    /// Must be 1; no interpretation of unknown versions.
    pub version: u16,
    /// Tenant/cluster/Core identity selected by the credential issuer, not trusted unsigned input.
    pub identity: CoreIdentity,
    /// Independent data or update authority selected by deployment.
    pub service_id: ServiceId,
    /// Exact requested operation.
    pub action: ServiceLeaseOperationV1,
}
impl ServiceLeaseRequestV1 {
    /// Scoped token name. Mutation and inspection are separate token purposes.
    pub fn token_name(&self) -> &'static str {
        match self.action {
            ServiceLeaseOperationV1::Acquire { .. } => "gateway.service-lease.acquire",
            ServiceLeaseOperationV1::Renew { .. } => "gateway.service-lease.renew",
            ServiceLeaseOperationV1::Release { .. } => "gateway.service-lease.release",
            ServiceLeaseOperationV1::Inspect => "gateway.service-lease.inspect",
            ServiceLeaseOperationV1::Check { .. } => "gateway.service-lease.check",
        }
    }
    /// Query for inspect/check; command for all mutations.
    pub fn token_purpose(&self) -> &'static str {
        if matches!(
            self.action,
            ServiceLeaseOperationV1::Inspect | ServiceLeaseOperationV1::Check { .. }
        ) {
            "query"
        } else {
            "command"
        }
    }
    /// Validate duplicated scope and bounded TTL before authentication/provider work.
    pub fn validate(&self) -> Result<(), crate::GatewayError> {
        if self.version != 1 || self.identity.protocol_version.as_u16() != 1 {
            return Err(crate::GatewayError::Protocol(
                "NO MORE SUPPORTED PLEASE UPDATE".into(),
            ));
        }
        let (lease, ttl) = match &self.action {
            ServiceLeaseOperationV1::Acquire { ttl_ms } => (None, Some(*ttl_ms)),
            ServiceLeaseOperationV1::Renew { lease, ttl_ms } => (Some(lease), Some(*ttl_ms)),
            ServiceLeaseOperationV1::Release { lease }
            | ServiceLeaseOperationV1::Check { lease } => (Some(lease), None),
            ServiceLeaseOperationV1::Inspect => (None, None),
        };
        if self.identity.tenant_id.validate().is_err()
            || self.identity.cluster_id.validate().is_err()
            || self.identity.core_id.validate().is_err()
            || self.identity.instance_id.validate().is_err()
            || self.identity.protocol_version.as_u16() != 1
            || ServiceId::new(self.service_id.as_str()).is_err()
            || self.version != 1
            || ttl.is_some_and(|v| !(1..=30_000).contains(&v))
            || lease.is_some_and(|l| {
                l.tenant_id != self.identity.tenant_id
                    || l.cluster_id != self.identity.cluster_id
                    || l.service_id != self.service_id
                    || l.holder_core_id != self.identity.core_id
                    || l.epoch == 0
                    || l.expires_at_ms <= l.acquired_at_ms
            })
        {
            return Err(crate::GatewayError::Protocol(
                "invalid service authority request".into(),
            ));
        }
        Ok(())
    }
    /// Canonical full-operation token hash, including tenant/cluster/holder/service/epoch/expiry.
    pub fn request_hash(&self) -> Result<String, crate::GatewayError> {
        self.validate()?;
        let payload = serde_json::to_string(self)
            .map_err(|_| crate::GatewayError::Protocol("invalid service request".into()))?;
        if payload.len() > MAX_SERVICE_LEASE_REQUEST_BYTES {
            return Err(crate::GatewayError::Protocol(
                "service request too large".into(),
            ));
        }
        Ok(compute_request_hash(&RequestValidationDetails {
            purpose: self.token_purpose().into(),
            name: self.token_name().into(),
            id: self.identity.tenant_id.as_str().into(),
            idempotency_key: None,
            payload,
            subject: Some(self.identity.core_id.as_str().into()),
            audience: Some(self.identity.cluster_id.as_str().into()),
        }))
    }
}
