// =============================================================================
//        #######
//     ###       ###     F: macos_timing.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

//! Local native-input timing; clock values never become serialized UI V1 data.
use crate::macos_window::UiNativeActivation;
use std::time::{Duration, Instant};
/// Local monotonic timing attached to a trusted activation; it adds no authority.
#[derive(Clone, Copy)]
pub struct UiNativeInputTiming {
    callback: Instant,
    input_to_callback: Option<Duration>,
}
impl std::fmt::Debug for UiNativeInputTiming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiNativeInputTiming")
            .field("input_to_callback", &self.input_to_callback)
            .finish_non_exhaustive()
    }
}
impl UiNativeInputTiming {
    /// Callback-to-observation elapsed time, including queue residence.
    pub fn callback_elapsed(&self) -> Duration {
        self.callback.elapsed()
    }
    /// Event-to-observation elapsed time when an event was correlated by the native pump.
    /// Includes event queueing and native tracking (e.g. mouse-down until release).
    /// None for uncorrelated/accessibility callbacks or when timing is disabled/invalid.
    pub fn input_elapsed(&self) -> Option<Duration> {
        self.input_to_callback?.checked_add(self.callback.elapsed())
    }
}
/// Activation plus bounded local timing metadata; not a wire format.
#[derive(Debug)]
pub struct UiNativeTimedActivation {
    /// Unchanged trusted native-control activation.
    pub activation: UiNativeActivation,
    /// Read-only observation; never grants permission to run a command.
    pub timing: UiNativeInputTiming,
}
impl UiNativeTimedActivation {
    pub(crate) fn new(activation: UiNativeActivation) -> Self {
        let callback = Instant::now();
        Self {
            activation,
            timing: UiNativeInputTiming {
                callback,
                input_to_callback: dispatch_age(callback),
            },
        }
    }
}
#[cfg(feature = "native-input-timing")]
thread_local! {
    static DISPATCH: std::cell::Cell<Option<(Instant, Duration)>> = const { std::cell::Cell::new(None) };
}
#[cfg(feature = "native-input-timing")]
fn dispatch_age(callback: Instant) -> Option<Duration> {
    DISPATCH.with(|slot| {
        let (started, age) = slot.get()?;
        age.checked_add(callback.checked_duration_since(started)?)
    })
}
#[cfg(not(feature = "native-input-timing"))]
fn dispatch_age(_: Instant) -> Option<Duration> {
    None
}
/// Restores correlation across nested dispatch and unwinding; no global stale currentEvent lookup.
#[cfg(feature = "native-input-timing")]
pub(crate) struct DispatchScope(Option<(Instant, Duration)>);
#[cfg(feature = "native-input-timing")]
impl DispatchScope {
    pub(crate) fn enter(timestamp: f64, input: bool) -> Self {
        if !input {
            return Self::from_age(None);
        }
        let uptime = objc2_foundation::NSProcessInfo::processInfo().systemUptime();
        Self::from_age(event_age(timestamp, uptime))
    }
    fn from_age(age: Option<Duration>) -> Self {
        Self(DISPATCH.with(|slot| slot.replace(age.map(|age| (Instant::now(), age)))))
    }
}
#[cfg(feature = "native-input-timing")]
impl Drop for DispatchScope {
    fn drop(&mut self) {
        DISPATCH.with(|slot| slot.set(self.0));
    }
}
#[cfg(feature = "native-input-timing")]
fn event_age(timestamp: f64, uptime: f64) -> Option<Duration> {
    if !timestamp.is_finite() || !uptime.is_finite() || timestamp <= 0.0 || uptime < timestamp {
        return None;
    }
    Duration::try_from_secs_f64(uptime - timestamp).ok()
}
#[cfg(all(test, feature = "native-input-timing"))]
mod tests {
    use super::*;
    #[test]
    fn invalid_clocks_never_invent_input_latency() {
        for (event, now) in [
            (f64::NAN, 2.),
            (1., f64::INFINITY),
            (3., 2.),
            (-1., 2.),
            (0., 2.),
        ] {
            assert_eq!(event_age(event, now), None);
        }
        assert_eq!(event_age(1., 1.25), Some(Duration::from_millis(250)));
    }
    #[test]
    fn nested_dispatch_restores_and_unwinds_without_stale_correlation() {
        assert!(dispatch_age(Instant::now()).is_none());
        let outer = DispatchScope::from_age(Some(Duration::from_millis(10)));
        assert!(dispatch_age(Instant::now()).unwrap() >= Duration::from_millis(10));
        {
            let _inner = DispatchScope::from_age(None);
            assert!(dispatch_age(Instant::now()).is_none());
        }
        assert!(dispatch_age(Instant::now()).is_some());
        drop(outer);
        let _ = std::panic::catch_unwind(|| {
            let _guard = DispatchScope::from_age(Some(Duration::ZERO));
            panic!("test unwind");
        });
        assert!(dispatch_age(Instant::now()).is_none());
    }
}
