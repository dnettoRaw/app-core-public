// =============================================================================
//        #######
//     ###       ###     F: authorization.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 10:22:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.3
// =============================================================================

//! Explicit per-block source policy, independent of discovery and transport authentication.
use crate::{
    BlockSource, ObjectDescriptor, SourceBinding, TransferControl, TransferError as E,
    TransferResult,
};
use std::io::Write;

/// Deployment-bound scope used for every source-policy decision.
/// Construct from trusted local configuration and authenticated discovery, not remote labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSourceScopeV1 {
    /// Tenant owning this transfer.
    pub tenant_id: String,
    /// Cluster owning this transfer.
    pub cluster_id: String,
    /// Core requesting the opaque object.
    pub requester_core_id: String,
    /// Core supplying blocks, independently resolved to an authenticated endpoint.
    pub source_core_id: String,
}
impl TransferSourceScopeV1 {
    /// Checks the bounded 128-byte ASCII identity profile, not authenticity.
    pub fn validate(&self) -> TransferResult<()> {
        for value in [
            &self.tenant_id,
            &self.cluster_id,
            &self.requester_core_id,
            &self.source_core_id,
        ] {
            if value.is_empty()
                || value.len() > 128
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            {
                return Err(E::InvalidContract);
            }
        }
        Ok(())
    }
}

/// Trusted source/redistribution policy. Implementations must bound work and fail closed
/// on expiry, revoked grants, missing authority or scope/object mismatch. Integrity alone
/// never grants access. Authentication and server-side object policy remain mandatory.
pub trait TransferSourceAuthorizer: Send + Sync {
    /// Authorizes this exact scope and immutable descriptor. Return Unauthorized for denial.
    /// Check complete descriptor identity, not an untrusted advertised label or role.
    fn authorize(
        &self,
        scope: &TransferSourceScopeV1,
        descriptor: &ObjectDescriptor,
        control: &TransferControl,
    ) -> TransferResult<()>;
}

/// Checks current policy before requesting a block and again before engine acceptance.
/// Policy checks are observations, not an atomic transaction with remote revocation.
/// A denial after reading discards that block; already durable blocks remain resumable.
/// Use through transfer, which buffers each block; arbitrary direct sinks can receive
/// bytes before the final check. Discovery maps Core to endpoint outside this wrapper.
/// No unsigned fallback is provided. Already verified local blocks can complete without
/// source requests; deployment separately authorizes local cache use and serving objects.
pub struct AuthorizedBlockSource<'a> {
    scope: TransferSourceScopeV1,
    source: &'a dyn BlockSource,
    policy: &'a dyn TransferSourceAuthorizer,
}
impl<'a> AuthorizedBlockSource<'a> {
    /// Binds one independently authenticated provider to its trusted scope and policy.
    pub fn new(
        scope: TransferSourceScopeV1,
        source: &'a dyn BlockSource,
        policy: &'a dyn TransferSourceAuthorizer,
    ) -> TransferResult<Self> {
        scope.validate()?;
        Ok(Self {
            scope,
            source,
            policy,
        })
    }
    /// Admits the wrapper using its scoped Core ID as the engine's unique source label.
    /// Up to 16 distinct providers may be selected by deployment discovery.
    pub fn binding(&self) -> TransferResult<SourceBinding<'_>> {
        SourceBinding::approved(self.scope.source_core_id.clone(), self)
    }
}
impl BlockSource for AuthorizedBlockSource<'_> {
    fn read_block(
        &self,
        descriptor: &ObjectDescriptor,
        index: usize,
        control: &TransferControl,
        sink: &mut dyn Write,
    ) -> TransferResult<()> {
        control.checkpoint()?;
        descriptor.block_len(index)?;
        self.policy.authorize(&self.scope, descriptor, control)?;
        control.checkpoint()?;
        self.source.read_block(descriptor, index, control, sink)?;
        control.checkpoint()?;
        self.policy.authorize(&self.scope, descriptor, control)?;
        control.checkpoint()
    }
}
