// =============================================================================
//        #######
//     ###       ###     F: hardware_sampler.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::resource_platform::PlatformHardwareProbe;
use crate::{AiError, AiResult, HardwareProbe, ResourceSnapshot};
use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Low-cardinality sampler counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HardwareSamplerMetrics {
    /// Successful physical samples.
    pub samples: u64,
    /// Failed physical samples.
    pub sample_failures: u64,
    /// Reads served from the short-lived cache.
    pub cache_hits: u64,
    /// Age of the latest valid sample.
    pub snapshot_age: Option<Duration>,
}

#[derive(Debug, Default)]
struct SamplerState {
    cached: Option<(Instant, ResourceSnapshot)>,
    last_failure: Option<(Instant, AiError)>,
    sampling: bool,
}

/// On-demand bounded sampler with cache and single-flight refresh.
///
/// It owns no polling thread: idle CPU cost is zero, and the first reader after
/// `sampling_interval` performs one refresh while concurrent readers wait.
pub struct HardwareSampler<P> {
    probe: P,
    sampling_interval: Duration,
    state: Mutex<SamplerState>,
    sampled: Condvar,
    samples: AtomicU64,
    sample_failures: AtomicU64,
    cache_hits: AtomicU64,
}

impl<P: Debug> Debug for HardwareSampler<P> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HardwareSampler")
            .field("probe", &self.probe)
            .field("sampling_interval", &self.sampling_interval)
            .finish_non_exhaustive()
    }
}

impl<P: HardwareProbe> HardwareSampler<P> {
    /// Creates a sampler with a non-zero refresh interval.
    pub fn new(probe: P, sampling_interval: Duration) -> AiResult<Self> {
        if sampling_interval.is_zero() {
            return Err(AiError::InvalidInput("hardware sampling interval"));
        }
        Ok(Self::with_valid_interval(probe, sampling_interval))
    }

    fn with_valid_interval(probe: P, sampling_interval: Duration) -> Self {
        Self {
            probe,
            sampling_interval,
            state: Mutex::new(SamplerState::default()),
            sampled: Condvar::new(),
            samples: AtomicU64::new(0),
            sample_failures: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
        }
    }

    /// Forces one single-flight refresh for diagnostics and certification.
    pub fn refresh(&self) -> AiResult<ResourceSnapshot> {
        self.sample_inner(true)
    }

    /// Returns sampler counters without exposing device identities.
    pub fn metrics(&self) -> HardwareSamplerMetrics {
        let age = self
            .state
            .lock()
            .ok()
            .and_then(|state| state.cached.as_ref().map(|(at, _)| at.elapsed()));
        HardwareSamplerMetrics {
            samples: self.samples.load(Ordering::Relaxed),
            sample_failures: self.sample_failures.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            snapshot_age: age,
        }
    }

    fn sample_inner(&self, force: bool) -> AiResult<ResourceSnapshot> {
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        loop {
            if !force {
                if let Some((sampled_at, snapshot)) = &state.cached {
                    if sampled_at.elapsed() < self.sampling_interval {
                        self.cache_hits.fetch_add(1, Ordering::Relaxed);
                        return Ok(snapshot.clone());
                    }
                }
                if let Some((failed_at, error)) = &state.last_failure {
                    if failed_at.elapsed() < self.sampling_interval {
                        return Err(error.clone());
                    }
                }
            }
            if !state.sampling {
                state.sampling = true;
                break;
            }
            state = self
                .sampled
                .wait(state)
                .map_err(|_| AiError::InternalState)?;
            if !state.sampling {
                if let Some((_, snapshot)) = &state.cached {
                    self.cache_hits.fetch_add(1, Ordering::Relaxed);
                    return Ok(snapshot.clone());
                }
                if let Some((_, error)) = &state.last_failure {
                    return Err(error.clone());
                }
            }
        }
        drop(state);

        let sampled = self.probe.sample().map(stamp_snapshot);
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        state.sampling = false;
        let result = match sampled {
            Ok(snapshot) => {
                self.samples.fetch_add(1, Ordering::Relaxed);
                state.cached = Some((Instant::now(), snapshot.clone()));
                state.last_failure = None;
                Ok(snapshot)
            }
            Err(error) => {
                self.sample_failures.fetch_add(1, Ordering::Relaxed);
                state.last_failure = Some((Instant::now(), error.clone()));
                Err(error)
            }
        };
        self.sampled.notify_all();
        result
    }
}

impl<P: HardwareProbe> HardwareProbe for HardwareSampler<P> {
    fn sample(&self) -> AiResult<ResourceSnapshot> {
        self.sample_inner(false)
    }
}

/// Production system probe with cached static discovery and sampled dynamics.
#[derive(Clone, Debug)]
pub struct SystemHardwareProbe {
    sampler: Arc<HardwareSampler<PlatformHardwareProbe>>,
}

impl SystemHardwareProbe {
    /// Creates an independent system probe with a custom sampling interval.
    pub fn with_sampling_interval(sampling_interval: Duration) -> AiResult<Self> {
        Ok(Self {
            sampler: Arc::new(HardwareSampler::new(
                PlatformHardwareProbe::new(),
                sampling_interval,
            )?),
        })
    }

    /// Forces a bounded physical refresh for a diagnostic tool.
    pub fn refresh(&self) -> AiResult<ResourceSnapshot> {
        self.sampler.refresh()
    }

    /// Returns low-cardinality sampling metrics.
    #[must_use]
    pub fn metrics(&self) -> HardwareSamplerMetrics {
        self.sampler.metrics()
    }
}

impl Default for SystemHardwareProbe {
    fn default() -> Self {
        // appcore-norm: allow(global-state) reason: one shared sampler prevents duplicate default hardware scans
        static DEFAULT: OnceLock<Arc<HardwareSampler<PlatformHardwareProbe>>> = OnceLock::new();
        let sampler = DEFAULT
            .get_or_init(|| {
                Arc::new(HardwareSampler::with_valid_interval(
                    PlatformHardwareProbe::new(),
                    Duration::from_secs(1),
                ))
            })
            .clone();
        Self { sampler }
    }
}

impl HardwareProbe for SystemHardwareProbe {
    fn sample(&self) -> AiResult<ResourceSnapshot> {
        self.sampler.sample()
    }
}

fn stamp_snapshot(mut snapshot: ResourceSnapshot) -> ResourceSnapshot {
    snapshot.captured_at_unix_ms.get_or_insert_with(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
            })
    });
    snapshot
}
