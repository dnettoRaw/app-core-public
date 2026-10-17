// =============================================================================
//        #######
//     ###       ###     F: file_authority.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:42:56 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 1.0.4-rc.3
// =============================================================================

use super::*;
use appcore_distributed_contracts::control_plane::lease_authority::ServiceLeaseAuthority;

impl ServiceLeaseAuthority for FileControlPlane {
    fn acquire<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service: &'a ServiceId,
        ttl: u64,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease> {
        Box::pin(async move {
            validate_ttl(ttl)?;
            validate_scope(identity, service)?;
            let _lock = self.try_authority_lock()?;
            let control = self.load_control()?;
            let now = self.clock.now_ms();
            if let Some(current) = control.inspect_exact(identity, service)? {
                if current.expires_at_ms > now {
                    if current.acquired_at_ms > now
                        || current.expires_at_ms.saturating_sub(now) > 30_000
                    {
                        return Err(ControlPlaneError::Conflict(
                            "authority clock or grant lifetime is outside the profile".into(),
                        ));
                    }
                    if current.holder_core_id != identity.core_id {
                        return Err(ControlPlaneError::LeaseUnavailable);
                    }
                    return Ok(current);
                }
            }
            let lease = control
                .acquire_or_renew_service_lease(identity, service, ttl, now)
                .await?;
            self.save_control(control)?;
            Ok(lease)
        })
    }
    fn renew<'a>(
        &'a self,
        lease: &'a ServiceLeaderLease,
        ttl: u64,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease> {
        Box::pin(async move {
            validate_ttl(ttl)?;
            let _lock = self.try_authority_lock()?;
            let control = self.load_control()?;
            let renewed = control.renew_exact(lease, ttl, self.clock.now_ms())?;
            self.save_control(control)?;
            Ok(renewed)
        })
    }
    fn release<'a>(&'a self, lease: &'a ServiceLeaderLease) -> ControlPlaneFuture<'a, ()> {
        Box::pin(async move {
            let _lock = self.try_authority_lock()?;
            let control = self.load_control()?;
            control.check_exact(lease, self.clock.now_ms())?;
            control.release_service_lease(lease.clone()).await?;
            self.save_control(control)
        })
    }
    fn inspect<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service: &'a ServiceId,
    ) -> ControlPlaneFuture<'a, Option<ServiceLeaderLease>> {
        Box::pin(async move {
            let _lock = self.try_authority_lock()?;
            validate_scope(identity, service)?;
            self.load_control()?.inspect_exact(identity, service)
        })
    }
    fn check<'a>(&'a self, lease: &'a ServiceLeaderLease) -> ControlPlaneFuture<'a, ()> {
        Box::pin(async move {
            let _lock = self.try_authority_lock()?;
            self.load_control()?.check_exact(lease, self.clock.now_ms())
        })
    }
}
fn validate_ttl(ttl: u64) -> ControlPlaneResult<()> {
    if !(1..=30_000).contains(&ttl) {
        return Err(ControlPlaneError::Rejected(
            "service lease TTL must be in 1..30000 ms".into(),
        ));
    }
    Ok(())
}

impl FileControlPlane {
    /// Open a preprovisioned authority without initializing missing state.
    /// Use this on every arbiter restart. Initial provisioning is explicit via with_clock.
    /// Missing/corrupt state fails closed; external filesystem rollback is not detectable here.
    pub fn open_existing_with_clock(
        root: impl Into<PathBuf>,
        retention_ms: u64,
        clock: Arc<dyn Clock>,
    ) -> ControlPlaneResult<Self> {
        if retention_ms == 0 {
            return Err(ControlPlaneError::Rejected("invalid retention".into()));
        }
        let root = root.into();
        reject_symlink(&root)?;
        if !root.is_dir() {
            return Err(ControlPlaneError::Offline);
        }
        let control = Self {
            state_path: root.join(STATE_FILE),
            lock_path: root.join(LOCK_FILE),
            root,
            retention_ms,
            clock,
        };
        {
            let _lock = control.try_authority_lock()?;
            let _ = control.load_control()?;
        }
        Ok(control)
    }
    /// Execute a trusted, bounded local commit while serializing validation and effect
    /// with every grant/renewal/release through the same file authority lock.
    /// This is not a permit for a remote later write. Remote stores must atomically
    /// enforce the epoch and application idempotency at their own commit boundary.
    /// Host owns callback I/O bounds; do not reenter the authority from the callback.
    pub fn with_fenced_service_write<T>(
        &self,
        lease: &ServiceLeaderLease,
        commit: impl FnOnce() -> ControlPlaneResult<T>,
    ) -> ControlPlaneResult<T> {
        let _lock = self.try_authority_lock()?;
        self.load_control()?
            .check_exact(lease, self.clock.now_ms())?;
        commit()
    }
}

impl FileControlPlane {
    fn try_authority_lock(&self) -> ControlPlaneResult<File> {
        reject_symlink(&self.lock_path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.lock_path)
            .map_err(|error| transport_error("authority lock unavailable", error))?;
        file.try_lock_exclusive()
            .map_err(|error| transport_error("authority busy", error))?;
        Ok(file)
    }
}

fn validate_scope(identity: &CoreIdentity, service: &ServiceId) -> ControlPlaneResult<()> {
    if identity.tenant_id.validate().is_err()
        || identity.cluster_id.validate().is_err()
        || identity.core_id.validate().is_err()
        || identity.instance_id.validate().is_err()
        || identity.protocol_version.as_u16() != 1
        || ServiceId::new(service.as_str()).is_err()
    {
        return Err(ControlPlaneError::Rejected(
            "invalid authority scope".into(),
        ));
    }
    Ok(())
}
