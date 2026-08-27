// =============================================================================
//        #######
//     ###       ###     F: redis_operations.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================

//! Validated Redis Gateway registry operations.

use super::redis_provider::status_result;
use super::redis_scripts;
use super::redis_validation::{
    absolute_ttl, bounded_expiry, decode, encode, ensure_live, ensure_same_owner, strings,
    validate_request, validate_request_shape,
};
use super::{
    GatewayInstanceLease, GatewayRegistryError, GatewayRegistryResult, GatewayRequestFence,
    GatewaySessionRecord, GatewayWorkerRecord, GatewayWorkerRegistration,
    RedisGatewayRegistryProvider, GATEWAY_HA_SCHEMA_V2, MAX_GATEWAY_INSTANCE_LEASE_TTL_MS,
    MAX_GATEWAY_RESOLVE_CANDIDATES,
};
use crate::config::{
    MAX_GATEWAY_CLIENTS_PER_TENANT, MAX_GATEWAY_PENDING_PER_TENANT, MAX_GATEWAY_REQUEST_TIMEOUT,
    MAX_GATEWAY_WORKERS_PER_TENANT,
};
use crate::GATEWAY_CONNECTION_TOKEN_TTL_MS;
use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};

impl RedisGatewayRegistryProvider {
    pub(crate) async fn acquire_instance_inner(
        &self,
        tenant: &TenantId,
        cluster: &ClusterId,
        instance: &InstanceId,
        url: &super::GatewayFederationUrl,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewayInstanceLease> {
        let expires_at_ms = bounded_expiry(now_ms, ttl_ms, MAX_GATEWAY_INSTANCE_LEASE_TTL_MS)?;
        let keys = [
            self.keys.epoch(tenant, instance),
            self.keys.lease(tenant, instance),
        ];
        let arguments = strings(&[
            GATEWAY_HA_SCHEMA_V2,
            tenant.as_str(),
            cluster.as_str(),
            instance.as_str(),
            url.expose(),
            &now_ms.to_string(),
            &expires_at_ms.to_string(),
            &ttl_ms.to_string(),
        ]);
        let epoch: i64 = self
            .script(redis_scripts::ACQUIRE_INSTANCE, &keys, &arguments)
            .await?;
        if epoch <= 0 {
            status_result(epoch)?;
            return Err(GatewayRegistryError::Unavailable);
        }
        GatewayInstanceLease::new(
            tenant.clone(),
            cluster.clone(),
            instance.clone(),
            url.clone(),
            u64::try_from(epoch).map_err(|_| GatewayRegistryError::Unavailable)?,
            expires_at_ms,
        )
    }

    pub(crate) async fn renew_instance_inner(
        &self,
        lease: &GatewayInstanceLease,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewayInstanceLease> {
        lease.validate()?;
        if lease.is_expired(now_ms) {
            return Err(GatewayRegistryError::Expired);
        }
        let expires_at_ms = bounded_expiry(now_ms, ttl_ms, MAX_GATEWAY_INSTANCE_LEASE_TTL_MS)?;
        let status: i64 = self
            .script(
                redis_scripts::RENEW_INSTANCE,
                &[self.keys.lease(lease.tenant_id(), lease.instance_id())],
                &strings(&[
                    &lease.epoch().to_string(),
                    &now_ms.to_string(),
                    &expires_at_ms.to_string(),
                    &ttl_ms.to_string(),
                ]),
            )
            .await?;
        status_result(status)?;
        GatewayInstanceLease::new(
            lease.tenant_id().clone(),
            lease.cluster_id().clone(),
            lease.instance_id().clone(),
            lease.federation_url().clone(),
            lease.epoch(),
            expires_at_ms,
        )
    }

    pub(crate) async fn release_instance_inner(
        &self,
        lease: &GatewayInstanceLease,
    ) -> GatewayRegistryResult<()> {
        lease.validate()?;
        self.status_script(
            redis_scripts::RELEASE_INSTANCE,
            &[self.keys.lease(lease.tenant_id(), lease.instance_id())],
            &strings(&[&lease.epoch().to_string()]),
        )
        .await
    }

    pub(crate) async fn check_instance_inner(
        &self,
        lease: &GatewayInstanceLease,
        now_ms: u64,
    ) -> GatewayRegistryResult<()> {
        lease.validate()?;
        if lease.is_expired(now_ms) {
            return Err(GatewayRegistryError::Expired);
        }
        self.status_script(
            redis_scripts::CHECK_INSTANCE,
            &[self.keys.lease(lease.tenant_id(), lease.instance_id())],
            &strings(&[&lease.epoch().to_string(), &now_ms.to_string()]),
        )
        .await
    }

    pub(crate) async fn register_worker_inner(
        &self,
        lease: &GatewayInstanceLease,
        registration: GatewayWorkerRegistration,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewayWorkerRecord> {
        lease.validate()?;
        registration.validate()?;
        ensure_live(lease, now_ms)?;
        let desired = bounded_expiry(now_ms, ttl_ms, MAX_GATEWAY_INSTANCE_LEASE_TTL_MS)?;
        let expires_at_ms = desired.min(lease.expires_at_ms());
        let worker = GatewayWorkerRecord::new(lease.clone(), registration, expires_at_ms)?;
        let worker_key = self
            .keys
            .worker(lease.tenant_id(), lease.cluster_id(), &worker.core_id);
        let keys = [
            self.keys.lease(lease.tenant_id(), lease.instance_id()),
            worker_key,
            self.keys
                .worker_capabilities(lease.tenant_id(), lease.cluster_id(), &worker.core_id),
            self.keys.workers(lease.tenant_id()),
        ];
        let mut arguments = strings(&[
            &lease.epoch().to_string(),
            &now_ms.to_string(),
            &worker.generation.to_string(),
            &expires_at_ms.to_string(),
            &encode(&worker)?,
            &expires_at_ms.saturating_sub(now_ms).to_string(),
            &MAX_GATEWAY_WORKERS_PER_TENANT.to_string(),
        ]);
        arguments.extend(
            worker
                .capabilities
                .iter()
                .map(|capability| self.keys.capability(lease.tenant_id(), capability)),
        );
        self.status_script(redis_scripts::REGISTER_WORKER, &keys, &arguments)
            .await?;
        Ok(worker)
    }

    pub(crate) async fn renew_worker_inner(
        &self,
        lease: &GatewayInstanceLease,
        worker: &GatewayWorkerRecord,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewayWorkerRecord> {
        lease.validate()?;
        worker.validate()?;
        ensure_same_owner(lease, &worker.owner)?;
        ensure_live(lease, now_ms)?;
        let desired = bounded_expiry(now_ms, ttl_ms, MAX_GATEWAY_INSTANCE_LEASE_TTL_MS)?;
        let expires_at_ms = desired.min(lease.expires_at_ms());
        let renewed = GatewayWorkerRecord {
            owner: lease.clone(),
            installation_id: worker.installation_id.clone(),
            core_id: worker.core_id.clone(),
            generation: worker.generation,
            capabilities: worker.capabilities.clone(),
            expires_at_ms,
        };
        let keys = self.worker_keys(lease, &worker.core_id);
        self.status_script(
            redis_scripts::RENEW_WORKER,
            &keys,
            &strings(&[
                &lease.epoch().to_string(),
                &now_ms.to_string(),
                &worker.generation.to_string(),
                &expires_at_ms.to_string(),
                &encode(&renewed)?,
                &expires_at_ms.saturating_sub(now_ms).to_string(),
            ]),
        )
        .await?;
        Ok(renewed)
    }

    pub(crate) async fn remove_worker_inner(
        &self,
        lease: &GatewayInstanceLease,
        worker: &GatewayWorkerRecord,
    ) -> GatewayRegistryResult<()> {
        lease.validate()?;
        worker.validate()?;
        ensure_same_owner(lease, &worker.owner)?;
        self.status_script(
            redis_scripts::REMOVE_WORKER,
            &self.worker_keys(lease, &worker.core_id),
            &strings(&[&lease.epoch().to_string(), &worker.generation.to_string()]),
        )
        .await
    }

    pub(crate) async fn resolve_worker_inner(
        &self,
        tenant: &TenantId,
        cluster: &ClusterId,
        core: &CoreId,
        now_ms: u64,
    ) -> GatewayRegistryResult<Option<GatewayWorkerRecord>> {
        let encoded: Option<String> = self
            .script(
                redis_scripts::RESOLVE_WORKER,
                &[self.keys.worker(tenant, cluster, core)],
                &strings(&[&now_ms.to_string()]),
            )
            .await?;
        let Some(encoded) = encoded else {
            return Ok(None);
        };
        let worker: GatewayWorkerRecord = decode(&encoded)?;
        worker.validate()?;
        if worker.owner.tenant_id() != tenant
            || worker.owner.cluster_id() != cluster
            || &worker.core_id != core
            || worker.is_expired(now_ms)
        {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(Some(worker))
    }

    pub(crate) async fn resolve_capability_inner(
        &self,
        tenant: &TenantId,
        capability: &CapabilityName,
        limit: usize,
        now_ms: u64,
    ) -> GatewayRegistryResult<Vec<GatewayWorkerRecord>> {
        if limit == 0 || limit > MAX_GATEWAY_RESOLVE_CANDIDATES {
            return Err(GatewayRegistryError::InvalidContract);
        }
        let encoded: Vec<String> = self
            .script(
                redis_scripts::RESOLVE_CAPABILITY,
                &[self.keys.capability(tenant, capability)],
                &strings(&[&now_ms.to_string(), &limit.to_string()]),
            )
            .await?;
        let mut workers = Vec::with_capacity(encoded.len());
        for value in encoded {
            let worker: GatewayWorkerRecord = decode(&value)?;
            worker.validate()?;
            if worker.owner.tenant_id() != tenant
                || worker.is_expired(now_ms)
                || !worker.capabilities.contains(capability)
            {
                return Err(GatewayRegistryError::InvalidContract);
            }
            workers.push(worker);
        }
        Ok(workers)
    }

    pub(crate) async fn register_session_inner(
        &self,
        lease: &GatewayInstanceLease,
        session: GatewaySessionRecord,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewaySessionRecord> {
        lease.validate()?;
        session.validate()?;
        ensure_same_owner(lease, &session.owner)?;
        ensure_live(lease, now_ms)?;
        let ttl_ms = absolute_ttl(
            session.expires_at_ms,
            now_ms,
            GATEWAY_CONNECTION_TOKEN_TTL_MS,
        )?;
        let registered =
            GatewaySessionRecord::new(lease.clone(), session.session_id, session.expires_at_ms)?;
        self.status_script(
            redis_scripts::REGISTER_SESSION,
            &[
                self.keys.lease(lease.tenant_id(), lease.instance_id()),
                self.keys.session(lease.tenant_id(), &registered.session_id),
                self.keys.sessions(lease.tenant_id()),
            ],
            &strings(&[
                &lease.epoch().to_string(),
                &now_ms.to_string(),
                &registered.expires_at_ms.to_string(),
                &encode(&registered)?,
                &ttl_ms.to_string(),
                &MAX_GATEWAY_CLIENTS_PER_TENANT.to_string(),
            ]),
        )
        .await?;
        Ok(registered)
    }

    pub(crate) async fn remove_session_inner(
        &self,
        lease: &GatewayInstanceLease,
        session: &GatewaySessionRecord,
    ) -> GatewayRegistryResult<()> {
        lease.validate()?;
        session.validate()?;
        ensure_same_owner(lease, &session.owner)?;
        self.status_script(
            redis_scripts::REMOVE_SESSION,
            &[
                self.keys.session(lease.tenant_id(), &session.session_id),
                self.keys.lease(lease.tenant_id(), lease.instance_id()),
                self.keys.sessions(lease.tenant_id()),
            ],
            &strings(&[&lease.epoch().to_string()]),
        )
        .await
    }

    pub(crate) async fn claim_request_inner(
        &self,
        origin: &GatewayInstanceLease,
        target: &GatewayWorkerRecord,
        request_id: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewayRequestFence> {
        origin.validate()?;
        target.validate()?;
        ensure_live(origin, now_ms)?;
        if target.is_expired(now_ms) {
            return Err(GatewayRegistryError::Expired);
        }
        let ttl_ms = absolute_ttl(
            expires_at_ms,
            now_ms,
            u64::try_from(MAX_GATEWAY_REQUEST_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
        )?;
        let request = GatewayRequestFence::new(origin, target, request_id, expires_at_ms)?;
        let keys = [
            self.keys.lease(origin.tenant_id(), origin.instance_id()),
            self.keys
                .lease(target.owner.tenant_id(), target.owner.instance_id()),
            self.keys.worker(
                target.owner.tenant_id(),
                target.owner.cluster_id(),
                &target.core_id,
            ),
            self.keys.request(origin.tenant_id(), request_id),
            self.keys.requests(origin.tenant_id()),
        ];
        self.status_script(
            redis_scripts::CLAIM_REQUEST,
            &keys,
            &strings(&[
                &origin.epoch().to_string(),
                &target.owner.epoch().to_string(),
                &target.generation.to_string(),
                &now_ms.to_string(),
                &expires_at_ms.to_string(),
                &encode(&request)?,
                &ttl_ms.to_string(),
                &MAX_GATEWAY_PENDING_PER_TENANT.to_string(),
            ]),
        )
        .await?;
        Ok(request)
    }

    pub(crate) async fn complete_request_inner(
        &self,
        request: &GatewayRequestFence,
        now_ms: u64,
    ) -> GatewayRegistryResult<()> {
        validate_request(request, now_ms)?;
        let keys = self.request_fence_keys(request);
        self.status_script(
            redis_scripts::COMPLETE_REQUEST,
            &keys,
            &strings(&[
                &request.origin_epoch.to_string(),
                &request.target_epoch.to_string(),
                &request.worker_generation.to_string(),
                &now_ms.to_string(),
            ]),
        )
        .await
    }

    pub(crate) async fn check_request_inner(
        &self,
        request: &GatewayRequestFence,
        now_ms: u64,
    ) -> GatewayRegistryResult<()> {
        validate_request(request, now_ms)?;
        let keys = self.request_fence_keys(request);
        self.status_script(
            redis_scripts::CHECK_REQUEST,
            &keys,
            &strings(&[
                &request.origin_epoch.to_string(),
                &request.target_epoch.to_string(),
                &request.worker_generation.to_string(),
                &now_ms.to_string(),
                &encode(request)?,
            ]),
        )
        .await
    }

    pub(crate) async fn cancel_request_inner(
        &self,
        request: &GatewayRequestFence,
    ) -> GatewayRegistryResult<()> {
        validate_request_shape(request)?;
        self.status_script(
            redis_scripts::CANCEL_REQUEST,
            &[
                self.keys.request(&request.tenant_id, &request.request_id),
                self.keys
                    .lease(&request.tenant_id, &request.origin_instance_id),
                self.keys.requests(&request.tenant_id),
            ],
            &strings(&[&request.origin_epoch.to_string()]),
        )
        .await
    }

    async fn status_script(
        &self,
        source: &str,
        keys: &[String],
        arguments: &[String],
    ) -> GatewayRegistryResult<()> {
        let status: i64 = self.script(source, keys, arguments).await?;
        status_result(status)
    }

    fn worker_keys(&self, lease: &GatewayInstanceLease, core: &CoreId) -> [String; 4] {
        [
            self.keys.lease(lease.tenant_id(), lease.instance_id()),
            self.keys
                .worker(lease.tenant_id(), lease.cluster_id(), core),
            self.keys
                .worker_capabilities(lease.tenant_id(), lease.cluster_id(), core),
            self.keys.workers(lease.tenant_id()),
        ]
    }

    fn request_fence_keys(&self, request: &GatewayRequestFence) -> [String; 5] {
        [
            self.keys
                .lease(&request.tenant_id, &request.origin_instance_id),
            self.keys
                .lease(&request.tenant_id, &request.target_instance_id),
            self.keys.worker(
                &request.tenant_id,
                &request.target_cluster_id,
                &request.target_core_id,
            ),
            self.keys.request(&request.tenant_id, &request.request_id),
            self.keys.requests(&request.tenant_id),
        ]
    }
}
