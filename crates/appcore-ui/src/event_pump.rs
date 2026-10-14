// =============================================================================
//        #######
//     ###       ###     F: event_pump.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Bounded event draining: wait only for the first event, then release queued input promptly.
use std::time::{Duration, Instant};
const MAX_EVENTS: usize = 64;
const DRAIN_BUDGET: Duration = Duration::from_millis(2);
pub(crate) fn drain<T>(
    wait: Duration,
    mut next: impl FnMut(Duration) -> Option<T>,
    mut dispatch: impl FnMut(T) -> bool,
) -> usize {
    let Some(first) = next(wait) else {
        return 0;
    };
    let started = Instant::now();
    let mut count = 1;
    if !dispatch(first) {
        return count;
    }
    while count < MAX_EVENTS && started.elapsed() < DRAIN_BUDGET {
        let Some(event) = next(Duration::ZERO) else {
            break;
        };
        count += 1;
        if !dispatch(event) {
            break;
        }
    }
    count
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque};
    #[test]
    fn queued_mouse_down_up_and_next_click_are_drained_without_waiting_again() {
        let events = RefCell::new(VecDeque::from(["down", "up", "down", "up"]));
        let mut waits = vec![];
        let mut delivered = vec![];
        assert_eq!(
            drain(
                Duration::from_millis(16),
                |wait| {
                    waits.push(wait);
                    events.borrow_mut().pop_front()
                },
                |event| {
                    delivered.push(event);
                    true
                }
            ),
            4
        );
        assert_eq!(delivered, vec!["down", "up", "down", "up"]);
        assert_eq!(waits[0], Duration::from_millis(16));
        assert!(waits[1..].iter().all(Duration::is_zero));
    }
    #[test]
    fn continuously_ready_events_are_bounded_and_close_stops_the_batch() {
        let count = drain(Duration::ZERO, |_| Some(()), |_| true);
        assert!(count > 0 && count <= 64);
        assert_eq!(drain(Duration::ZERO, |_| Some(()), |_| false), 1);
        assert_eq!(drain(Duration::ZERO, |_| None::<()>, |_| true), 0);
    }
}
