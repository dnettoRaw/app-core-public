// =============================================================================
//        #######
//     ###       ###     F: shared_lease.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Shared resource lease contracts and filesystem implementation.

use crate::shared_lease_state::{
    read_epoch, read_state, reject_symlink, remove_state, write_epoch, write_state,
};
use crate::{ProviderError, ProviderResult};
use fs2::FileExt;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

/// Owner identity used in shared-resource leases.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LeaseOwner(String);

impl LeaseOwner {
    /// Creates and validates a lease owner.
    pub fn new(value: impl Into<String>) -> ProviderResult<Self> {
        let value = value.into();
        validate_label("lease owner", &value)?;
        Ok(Self(value))
    }

    /// Returns the owner as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Fencing token returned to a lease holder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseToken {
    /// Resource governed by this token.
    pub resource: String,
    /// Owner that acquired the token.
    pub owner: LeaseOwner,
    /// Monotonic fencing epoch.
    pub epoch: u64,
}

/// Durable shared-resource lease state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedResourceLease {
    /// Current fencing token.
    pub token: LeaseToken,
    /// Acquisition timestamp in milliseconds.
    pub acquired_at_ms: u64,
    /// Last heartbeat timestamp in milliseconds.
    pub heartbeat_at_ms: u64,
    /// Expiration timestamp in milliseconds.
    pub expires_at_ms: u64,
}

impl SharedResourceLease {
    /// Reports whether this lease is expired at `now_ms` under `policy`.
    pub fn is_expired(&self, now_ms: u64, policy: &LeasePolicy) -> bool {
        now_ms.saturating_sub(policy.clock_skew_ms) >= self.expires_at_ms
    }
}

/// Lease heartbeat request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseHeartbeat {
    /// Token being renewed.
    pub token: LeaseToken,
    /// Caller timestamp in milliseconds.
    pub now_ms: u64,
}

/// Lease timing policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeasePolicy {
    /// Lease time-to-live in milliseconds.
    pub ttl_ms: u64,
    /// Expected heartbeat interval in milliseconds.
    pub heartbeat_ms: u64,
    /// Accepted local clock tolerance in milliseconds.
    pub clock_skew_ms: u64,
}

impl LeasePolicy {
    /// Creates a lease policy.
    pub fn new(ttl_ms: u64, heartbeat_ms: u64, clock_skew_ms: u64) -> ProviderResult<Self> {
        let policy = Self {
            ttl_ms,
            heartbeat_ms,
            clock_skew_ms,
        };
        policy.validate()?;
        Ok(policy)
    }

    fn validate(self) -> ProviderResult<()> {
        if self.ttl_ms == 0
            || self.heartbeat_ms == 0
            || self.ttl_ms <= self.heartbeat_ms
            || self.clock_skew_ms >= self.ttl_ms
        {
            return Err(ProviderError::InvalidConfiguration(
                "lease ttl must exceed heartbeat and clock skew".to_string(),
            ));
        }
        Ok(())
    }
}

/// Result of a fencing check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseDecision {
    /// The token is current and may write.
    Allowed,
    /// No lease exists for the resource.
    NoLease,
    /// The lease exists but has expired.
    Expired,
    /// Another owner holds the lease.
    WrongOwner,
    /// The supplied fencing epoch is stale.
    StaleEpoch,
}

/// Provider-independent shared-resource lease repository.
pub trait LeaseRepository: Send + Sync {
    /// Acquires a lease or recovers an expired one.
    fn acquire(
        &self,
        resource: &str,
        owner: LeaseOwner,
        now_ms: u64,
    ) -> ProviderResult<SharedResourceLease>;
    /// Renews a lease held by the same owner and fencing epoch.
    fn heartbeat(&self, heartbeat: LeaseHeartbeat) -> ProviderResult<SharedResourceLease>;
    /// Releases a current lease.
    fn release(&self, token: &LeaseToken) -> ProviderResult<()>;
    /// Returns the current durable lease state, expired or not.
    fn current(&self, resource: &str) -> ProviderResult<Option<SharedResourceLease>>;
    /// Checks whether a writer still owns the current fencing token.
    fn check_fence(&self, token: &LeaseToken, now_ms: u64) -> ProviderResult<LeaseDecision>;
}

/// Filesystem-backed lease repository for OS-lock-capable shared filesystems.
#[derive(Debug, Clone)]
pub struct FileLeaseRepository {
    root: PathBuf,
    policy: LeasePolicy,
}

impl FileLeaseRepository {
    /// Opens a repository under a shared filesystem root.
    pub fn open(root: impl Into<PathBuf>, policy: LeasePolicy) -> ProviderResult<Self> {
        let root = root.into();
        policy.validate()?;
        reject_symlink(&root)?;
        fs::create_dir_all(&root)
            .map_err(|error| initialization(format!("lease root create failed: {error}")))?;
        reject_symlink(&root)?;
        if !root.is_dir() {
            return Err(invalid("lease root is not a directory"));
        }
        Ok(Self { root, policy })
    }

    /// Returns the configured lease policy.
    pub fn policy(&self) -> LeasePolicy {
        self.policy
    }

    fn lock_path(&self, resource: &str) -> ProviderResult<PathBuf> {
        Ok(self.root.join(format!("{}.lock", resource_file(resource)?)))
    }

    fn state_path(&self, resource: &str) -> ProviderResult<PathBuf> {
        Ok(self
            .root
            .join(format!("{}.lease", resource_file(resource)?)))
    }

    fn epoch_path(&self, resource: &str) -> ProviderResult<PathBuf> {
        Ok(self
            .root
            .join(format!("{}.epoch", resource_file(resource)?)))
    }

    fn with_lock<T>(
        &self,
        resource: &str,
        operation: impl FnOnce(&Path) -> ProviderResult<T>,
    ) -> ProviderResult<T> {
        reject_symlink(&self.root)?;
        let lock_path = self.lock_path(resource)?;
        reject_symlink(&lock_path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|error| initialization(format!("lease lock open failed: {error}")))?;
        file.lock_exclusive()
            .map_err(|error| initialization(format!("lease lock failed: {error}")))?;
        let result = operation(&self.state_path(resource)?);
        FileExt::unlock(&file).map_err(|_| initialization("lease unlock failed"))?;
        result
    }
}

impl LeaseRepository for FileLeaseRepository {
    fn acquire(
        &self,
        resource: &str,
        owner: LeaseOwner,
        now_ms: u64,
    ) -> ProviderResult<SharedResourceLease> {
        self.with_lock(resource, |state_path| {
            let current = read_state(state_path, resource)?;
            if let Some(current) = &current {
                if !current.is_expired(now_ms, &self.policy) && current.token.owner != owner {
                    return Err(ProviderError::Initialization(
                        "shared resource lease is held by another owner".to_string(),
                    ));
                }
            }
            let epoch_path = self.epoch_path(resource)?;
            let previous_epoch = read_epoch(&epoch_path)?
                .into_iter()
                .chain(current.iter().map(|lease| lease.token.epoch))
                .max()
                .unwrap_or(0);
            let next_epoch = previous_epoch
                .checked_add(1)
                .ok_or_else(|| initialization("shared resource fencing epoch exhausted"))?;
            let expires_at_ms = lease_expiration(now_ms, self.policy.ttl_ms)?;
            let lease = SharedResourceLease {
                token: LeaseToken {
                    resource: resource.to_string(),
                    owner,
                    epoch: next_epoch,
                },
                acquired_at_ms: now_ms,
                heartbeat_at_ms: now_ms,
                expires_at_ms,
            };
            write_epoch(&epoch_path, next_epoch)?;
            write_state(state_path, &lease)?;
            Ok(lease)
        })
    }

    fn heartbeat(&self, heartbeat: LeaseHeartbeat) -> ProviderResult<SharedResourceLease> {
        self.with_lock(&heartbeat.token.resource, |state_path| {
            let mut current =
                read_state(state_path, &heartbeat.token.resource)?.ok_or_else(|| {
                    ProviderError::Initialization("shared resource lease is absent".to_string())
                })?;
            ensure_current(&current, &heartbeat.token, heartbeat.now_ms, self.policy)?;
            if heartbeat.now_ms < current.heartbeat_at_ms {
                return Err(initialization("lease heartbeat clock moved backwards"));
            }
            current.heartbeat_at_ms = heartbeat.now_ms;
            current.expires_at_ms = lease_expiration(heartbeat.now_ms, self.policy.ttl_ms)?;
            write_state(state_path, &current)?;
            Ok(current)
        })
    }

    fn release(&self, token: &LeaseToken) -> ProviderResult<()> {
        self.with_lock(&token.resource, |state_path| {
            let current = read_state(state_path, &token.resource)?.ok_or_else(|| {
                ProviderError::Initialization("shared resource lease is absent".to_string())
            })?;
            ensure_same_token(&current, token)?;
            remove_state(state_path)
        })
    }

    fn current(&self, resource: &str) -> ProviderResult<Option<SharedResourceLease>> {
        self.with_lock(resource, |state_path| read_state(state_path, resource))
    }

    fn check_fence(&self, token: &LeaseToken, now_ms: u64) -> ProviderResult<LeaseDecision> {
        self.with_lock(&token.resource, |state_path| {
            let Some(current) = read_state(state_path, &token.resource)? else {
                return Ok(LeaseDecision::NoLease);
            };
            if current.is_expired(now_ms, &self.policy) {
                return Ok(LeaseDecision::Expired);
            }
            if current.token.owner != token.owner {
                return Ok(LeaseDecision::WrongOwner);
            }
            if current.token.epoch != token.epoch {
                return Ok(LeaseDecision::StaleEpoch);
            }
            Ok(LeaseDecision::Allowed)
        })
    }
}

fn lease_expiration(now_ms: u64, ttl_ms: u64) -> ProviderResult<u64> {
    now_ms
        .checked_add(ttl_ms)
        .ok_or_else(|| initialization("lease expiration timestamp overflow"))
}

fn ensure_current(
    current: &SharedResourceLease,
    token: &LeaseToken,
    now_ms: u64,
    policy: LeasePolicy,
) -> ProviderResult<()> {
    ensure_same_token(current, token)?;
    if current.is_expired(now_ms, &policy) {
        return Err(ProviderError::Initialization(
            "shared resource lease has expired".to_string(),
        ));
    }
    Ok(())
}

fn ensure_same_token(current: &SharedResourceLease, token: &LeaseToken) -> ProviderResult<()> {
    if current.token != *token {
        return Err(ProviderError::Initialization(
            "shared resource lease fencing token mismatch".to_string(),
        ));
    }
    Ok(())
}

fn resource_file(resource: &str) -> ProviderResult<String> {
    validate_label("lease resource", resource)?;
    Ok(resource.to_string())
}

fn validate_label(field: &'static str, value: &str) -> ProviderResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(ProviderError::InvalidConfiguration(format!(
            "invalid {field}"
        )));
    }
    Ok(())
}

fn initialization(message: impl Into<String>) -> ProviderError {
    ProviderError::Initialization(message.into())
}

fn invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::InvalidConfiguration(message.into())
}

#[cfg(test)]
#[path = "shared_lease_tests.rs"]
mod tests;
