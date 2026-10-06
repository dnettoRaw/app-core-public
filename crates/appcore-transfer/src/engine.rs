// =============================================================================
//        #######
//     ###       ###     F: engine.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Joined, bounded workers with source rotation, retries and fail-closed publication.
use crate::{
    ObjectDescriptor, TransferControl, TransferError as E, TransferResult, TransferSession,
    VerifiedObject,
};
use std::{
    collections::{HashSet, VecDeque},
    io::Write,
    time::Duration,
};
/// An explicitly selected provider. Implementations must honor cancellation and
/// deadline and bound every blocking I/O operation; the executor never detaches
/// a thread or pretends it can forcibly interrupt arbitrary provider code.
pub trait BlockSource: Send + Sync {
    /// Writes exactly the requested opaque block to a bounded sink. Do not
    /// allocate a whole object or log payloads. The caller verifies the digest.
    fn read_block(
        &self,
        descriptor: &ObjectDescriptor,
        index: usize,
        control: &TransferControl,
        sink: &mut dyn Write,
    ) -> TransferResult<()>;
}
/// Deployment-approved source identity and provider; not discovery or authentication.
pub struct SourceBinding<'a> {
    id: String,
    source: &'a dyn BlockSource,
}
impl<'a> SourceBinding<'a> {
    /// Explicit admission by the caller after its tenant/cluster/source policy.
    /// Approval does not replace the authenticated Peer RPC host/client boundary.
    pub fn approved(id: impl Into<String>, source: &'a dyn BlockSource) -> TransferResult<Self> {
        let id = id.into();
        if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
            return Err(E::InvalidContract);
        }
        Ok(Self { id, source })
    }
    /// Bounded source label.
    pub fn id(&self) -> &str {
        &self.id
    }
}
/// Per-invocation scheduling limits; each source has at most one active worker.
#[derive(Debug, Clone, Copy)]
pub struct TransferOptions {
    /// Maximum simultaneous workers, 1..=8, also limited by admitted sources.
    pub max_parallel: usize,
    /// Maximum attempts for each block across all peers, 1..=32.
    pub max_attempts: usize,
    /// Base retry wait, at most one second; deterministic jitter adds up to this amount.
    pub retry_delay: Duration,
}
impl Default for TransferOptions {
    fn default() -> Self {
        Self {
            max_parallel: 4,
            max_attempts: 8,
            retry_delay: Duration::from_millis(20),
        }
    }
}
/// Final observable counters and a verified immutable object.
#[derive(Debug)]
pub struct TransferReport {
    /// Verified result, with no activation side effect.
    pub object: VerifiedObject,
    /// Blocks recovered from durable state before network work.
    pub resumed_blocks: usize,
    /// Number of provider attempts, including failures.
    pub attempts: usize,
    /// Number of peers rejected for invalid bytes or panics.
    pub quarantined_sources: usize,
    /// Verified downloaded bytes per input source; source labels are caller-owned.
    pub source_bytes: Vec<u64>,
}
/// Completes one session from bounded explicitly approved providers. On failure,
/// durable valid blocks remain resumable. Workers are joined before returning;
/// timeouts are cooperative and depend on provider-owned I/O bounds.
pub fn transfer(
    mut session: TransferSession<'_>,
    sources: &[SourceBinding<'_>],
    options: TransferOptions,
    control: &TransferControl,
) -> TransferResult<TransferReport> {
    validate_sources(sources, options)?;
    control.checkpoint()?;
    let descriptor = session.descriptor().clone();
    let missing = session.missing();
    let resumed_blocks = descriptor.block_count() - missing.len();
    let mut pending: VecDeque<_> = missing.into();
    let mut attempts = vec![0; descriptor.block_count()];
    let mut quarantined = vec![false; sources.len()];
    let mut source_bytes = vec![0; sources.len()];
    let mut cursor = 0;
    while !pending.is_empty() {
        control.checkpoint()?;
        let jobs = select_jobs(
            &mut pending,
            &mut attempts,
            &quarantined,
            &mut cursor,
            options,
        )?;
        let results = fetch_wave(&descriptor, sources, &jobs, control);
        let mut retry = false;
        for ((index, source), result) in jobs.into_iter().zip(results) {
            control.checkpoint()?;
            match result {
                Ok(bytes) => {
                    session.accept(index, &bytes)?;
                    source_bytes[source] += bytes.len() as u64;
                }
                Err(E::Cancelled) => return Err(E::Cancelled),
                Err(E::Deadline) => return Err(E::Deadline),
                Err(error) => {
                    if matches!(error, E::Integrity | E::Limit | E::InvalidContract) {
                        quarantined[source] = true;
                    }
                    pending.push_back(index);
                    retry = true;
                }
            }
        }
        if retry {
            let span = options.retry_delay.as_nanos() as u64;
            let seed = attempts.iter().sum::<usize>() as u64
                ^ cursor as u64
                ^ u64::from(descriptor.object_hash().as_bytes()[0]);
            let jitter = Duration::from_nanos(seed.wrapping_mul(6364136223846793005) % (span + 1));
            control.wait(options.retry_delay + jitter)?;
        }
    }
    control.checkpoint()?;
    let object = session.finish_with_control(control)?;
    control.checkpoint()?;
    Ok(TransferReport {
        object,
        resumed_blocks,
        attempts: attempts.iter().sum(),
        quarantined_sources: quarantined.iter().filter(|v| **v).count(),
        source_bytes,
    })
}
fn validate_sources(sources: &[SourceBinding<'_>], options: TransferOptions) -> TransferResult<()> {
    if sources.is_empty()
        || sources.len() > 16
        || options.max_parallel == 0
        || options.max_parallel > 8
        || options.max_attempts == 0
        || options.max_attempts > 32
        || options.retry_delay > Duration::from_secs(1)
    {
        return Err(E::InvalidContract);
    }
    let mut ids = HashSet::new();
    if sources.iter().any(|s| !ids.insert(&s.id)) {
        return Err(E::InvalidContract);
    }
    Ok(())
}
fn select_jobs(
    pending: &mut VecDeque<usize>,
    attempts: &mut [usize],
    quarantined: &[bool],
    cursor: &mut usize,
    options: TransferOptions,
) -> TransferResult<Vec<(usize, usize)>> {
    let mut used = vec![false; quarantined.len()];
    let mut jobs = Vec::new();
    for _ in 0..options.max_parallel.min(pending.len()) {
        let source = (0..quarantined.len())
            .map(|offset| (*cursor + offset) % quarantined.len())
            .find(|i| !used[*i] && !quarantined[*i]);
        let Some(source) = source else { break };
        let index = pending.pop_front().ok_or(E::InvalidContract)?;
        if attempts[index] >= options.max_attempts {
            return Err(E::SourcesExhausted);
        }
        attempts[index] += 1;
        used[source] = true;
        *cursor = (source + 1) % quarantined.len();
        jobs.push((index, source));
    }
    if jobs.is_empty() {
        return Err(E::SourcesExhausted);
    }
    Ok(jobs)
}
fn fetch_wave(
    descriptor: &ObjectDescriptor,
    sources: &[SourceBinding<'_>],
    jobs: &[(usize, usize)],
    control: &TransferControl,
) -> Vec<TransferResult<Vec<u8>>> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|(index, source)| {
                std::thread::Builder::new().spawn_scoped(scope, move || {
                    let mut sink = BlockSink {
                        bytes: Vec::with_capacity(descriptor.block_len(*index)?),
                        limit: descriptor.block_len(*index)?,
                        failure: None,
                        control,
                    };
                    let result = sources[*source]
                        .source
                        .read_block(descriptor, *index, control, &mut sink);
                    if let Some(error) = sink.failure {
                        return Err(error);
                    }
                    result?;
                    control.checkpoint()?;
                    descriptor.verify_block(*index, &sink.bytes)?;
                    Ok(sink.bytes)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| match h {
                Ok(h) => h.join().unwrap_or(Err(E::InvalidContract)),
                Err(_) => Err(E::Source),
            })
            .collect()
    })
}
struct BlockSink<'a> {
    bytes: Vec<u8>,
    limit: usize,
    failure: Option<E>,
    control: &'a TransferControl,
}
impl Write for BlockSink<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let result = self.control.checkpoint().and_then(|()| {
            if self
                .bytes
                .len()
                .checked_add(bytes.len())
                .is_none_or(|n| n > self.limit)
            {
                Err(E::Limit)
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            self.failure = Some(error);
            return Err(std::io::Error::other(error));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
