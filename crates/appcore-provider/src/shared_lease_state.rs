// =============================================================================
//        #######
//     ###       ###     F: shared_lease_state.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 12:07:11 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Durable encoding for shared-resource leases and fencing high-water marks.

use crate::shared_lease::{LeaseOwner, LeaseToken, SharedResourceLease};
use crate::{ProviderError, ProviderResult};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_STATE_BYTES: u64 = 4 * 1024;
const LEASE_FORMAT: &str = "appcore-shared-lease-v1";
const EPOCH_FORMAT: &str = "appcore-shared-lease-epoch-v1";
// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn read_state(
    path: &Path,
    expected_resource: &str,
) -> ProviderResult<Option<SharedResourceLease>> {
    let Some(text) = read_optional_bounded(path)? else {
        return Ok(None);
    };
    parse_state(&text, expected_resource).map(Some)
}

pub(crate) fn write_state(path: &Path, lease: &SharedResourceLease) -> ProviderResult<()> {
    let text = format!(
        "format={LEASE_FORMAT}\nresource={}\nowner={}\nepoch={}\nacquired_at_ms={}\nheartbeat_at_ms={}\nexpires_at_ms={}\n",
        lease.token.resource,
        lease.token.owner.as_str(),
        lease.token.epoch,
        lease.acquired_at_ms,
        lease.heartbeat_at_ms,
        lease.expires_at_ms
    );
    atomic_replace(path, text.as_bytes())
}

pub(crate) fn read_epoch(path: &Path) -> ProviderResult<Option<u64>> {
    let Some(text) = read_optional_bounded(path)? else {
        return Ok(None);
    };
    let mut format_seen = false;
    let mut epoch = None;
    for line in text.lines() {
        let (key, value) = split_field(line)?;
        match key {
            "format" if value == EPOCH_FORMAT && !format_seen => format_seen = true,
            "epoch" if epoch.is_none() => epoch = Some(parse_u64(value)?),
            _ => return Err(invalid("lease epoch state is malformed")),
        }
    }
    if !format_seen {
        return Err(invalid("lease epoch state has unsupported format"));
    }
    epoch
        .map(Some)
        .ok_or_else(|| invalid("lease epoch state missing epoch"))
}

pub(crate) fn write_epoch(path: &Path, epoch: u64) -> ProviderResult<()> {
    atomic_replace(
        path,
        format!("format={EPOCH_FORMAT}\nepoch={epoch}\n").as_bytes(),
    )
}

pub(crate) fn remove_state(path: &Path) -> ProviderResult<()> {
    reject_symlink(path)?;
    fs::remove_file(path)
        .map_err(|error| initialization(format!("lease release failed: {error}")))?;
    sync_parent(path.parent().unwrap_or_else(|| Path::new(".")))
}

pub(crate) fn reject_symlink(path: &Path) -> ProviderResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(invalid("lease path symlink")),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(initialization(format!(
            "lease path inspect failed: {error}"
        ))),
    }
}

fn read_optional_bounded(path: &Path) -> ProviderResult<Option<String>> {
    reject_symlink(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(initialization(format!("lease read failed: {error}"))),
    };
    let mut bytes = Vec::new();
    file.take(MAX_STATE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| initialization(format!("lease read failed: {error}")))?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(invalid("lease state exceeds size limit"));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| invalid("lease state is not UTF-8"))
}

fn parse_state(text: &str, expected_resource: &str) -> ProviderResult<SharedResourceLease> {
    let mut format_seen = false;
    let mut resource = None;
    let mut owner = None;
    let mut epoch = None;
    let mut acquired_at_ms = None;
    let mut heartbeat_at_ms = None;
    let mut expires_at_ms = None;
    for line in text.lines() {
        let (key, value) = split_field(line)?;
        match key {
            "format" if value == LEASE_FORMAT && !format_seen => format_seen = true,
            "resource" if resource.is_none() => resource = Some(value.to_string()),
            "owner" if owner.is_none() => owner = Some(LeaseOwner::new(value)?),
            "epoch" if epoch.is_none() => epoch = Some(parse_u64(value)?),
            "acquired_at_ms" if acquired_at_ms.is_none() => {
                acquired_at_ms = Some(parse_u64(value)?)
            }
            "heartbeat_at_ms" if heartbeat_at_ms.is_none() => {
                heartbeat_at_ms = Some(parse_u64(value)?)
            }
            "expires_at_ms" if expires_at_ms.is_none() => expires_at_ms = Some(parse_u64(value)?),
            _ => return Err(invalid("lease state is malformed")),
        }
    }
    if !format_seen {
        return Err(invalid("lease state has unsupported format"));
    }
    let resource = resource.ok_or_else(|| invalid("lease state missing resource"))?;
    if resource != expected_resource {
        return Err(invalid("lease state resource mismatch"));
    }
    Ok(SharedResourceLease {
        token: LeaseToken {
            resource,
            owner: owner.ok_or_else(|| invalid("lease state missing owner"))?,
            epoch: epoch.ok_or_else(|| invalid("lease state missing epoch"))?,
        },
        acquired_at_ms: acquired_at_ms
            .ok_or_else(|| invalid("lease state missing acquired_at_ms"))?,
        heartbeat_at_ms: heartbeat_at_ms
            .ok_or_else(|| invalid("lease state missing heartbeat_at_ms"))?,
        expires_at_ms: expires_at_ms.ok_or_else(|| invalid("lease state missing expires_at_ms"))?,
    })
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> ProviderResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temporary = parent.join(format!(
        ".lease.{}-{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = write_and_rename(&temporary, path, parent, bytes);
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn write_and_rename(temp: &Path, path: &Path, parent: &Path, bytes: &[u8]) -> ProviderResult<()> {
    reject_symlink(path)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(temp)
        .map_err(|error| initialization(format!("lease temp create failed: {error}")))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| initialization(format!("lease write failed: {error}")))?;
    fs::rename(temp, path)
        .map_err(|error| initialization(format!("lease rename failed: {error}")))?;
    sync_parent(parent)
}

fn split_field(line: &str) -> ProviderResult<(&str, &str)> {
    line.split_once('=')
        .ok_or_else(|| invalid("lease state is malformed"))
}

fn parse_u64(value: &str) -> ProviderResult<u64> {
    value
        .parse::<u64>()
        .map_err(|_| invalid("lease integer field is malformed"))
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> ProviderResult<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| initialization(format!("lease parent sync failed: {error}")))
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> ProviderResult<()> {
    Ok(())
}

fn initialization(message: impl Into<String>) -> ProviderError {
    ProviderError::Initialization(message.into())
}

fn invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::InvalidConfiguration(message.into())
}
