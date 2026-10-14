// =============================================================================
//        #######
//     ###       ###     F: subscriptions.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Authenticated ephemeral notifications with count/byte backpressure and bounded pages.
use crate::surface_state::{
    remove_subscription, Owner, QueuedEvent, Subscription, MAX_SAFE_SEQUENCE,
};
use crate::{
    UiCaller, UiConnectionState, UiError, UiEventV1, UiReplyV1, UiRequestV1, UiResult, UiSurface,
};
use appcore_capabilities::CapabilityRequest;
use appcore_core::{CapabilityMode, CapabilityName};
use serde_json::{json, Value};
use std::{collections::VecDeque, sync::Arc};
/// Delivery report with no event payload or subscriber identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiPublishReport {
    /// Subscriber queues that accepted the event.
    pub delivered: usize,
    /// Subscriber queues that rejected the event under backpressure.
    pub dropped: usize,
}
impl UiSurface {
    pub(crate) fn subscribe(
        &self,
        caller: &UiCaller,
        frame: &UiRequestV1,
        capability: &str,
        topic: &str,
    ) -> UiResult<Value> {
        if self
            .topics
            .get(topic)
            .is_none_or(|declared| declared != capability)
        {
            return Err(UiError::Forbidden);
        }
        let epoch = self.state.lock().epoch;
        let request = read_request(frame, capability)?;
        let grant = self.authorize(caller, &request)?;
        let mut state = self.state.lock();
        if state.epoch != epoch {
            return Err(UiError::Cancelled);
        }
        if state.connection == UiConnectionState::Closed {
            return Err(UiError::Closed);
        }
        if state.connection == UiConnectionState::Offline {
            return Err(UiError::Unavailable);
        }
        if state.subscriptions.len() >= self.limits.max_subscriptions {
            return Err(UiError::Busy);
        }
        state.next_subscription = next_sequence(state.next_subscription)?;
        let id = format!("{}-{}", self.generation, state.next_subscription);
        state.subscriptions.insert(
            id.clone(),
            Subscription {
                capability: capability.to_string(),
                topic: topic.to_string(),
                owner: Owner::from(&grant),
                events: VecDeque::new(),
                dropped: 0,
            },
        );
        Ok(json!({"subscription_id":id}))
    }
    pub(crate) fn poll(&self, caller: &UiCaller, frame: &UiRequestV1, id: &str) -> UiResult<Value> {
        let (capability, owner) = {
            let state = self.state.lock();
            let sub = state
                .subscriptions
                .get(id)
                .ok_or(UiError::SubscriptionNotFound)?;
            (sub.capability.clone(), sub.owner.clone())
        };
        let request = read_request(frame, &capability)?;
        let grant = match self.authorize(caller, &request) {
            Ok(grant) => grant,
            Err(error) => {
                remove_subscription(&mut self.state.lock(), id);
                return Err(error);
            }
        };
        if owner != Owner::from(&grant) {
            remove_subscription(&mut self.state.lock(), id);
            return Err(UiError::Forbidden);
        }
        let mut state = self.state.lock();
        let connection = state.connection;
        let sub = state
            .subscriptions
            .get_mut(id)
            .ok_or(UiError::SubscriptionNotFound)?;
        let empty = json!({"subscription_id":id,"events":[],"dropped":sub.dropped});
        let overhead = crate::json_budget::json_bytes(
            &UiReplyV1::new(&self.id, &frame.request_id, connection, Ok(empty)),
            self.limits.max_frame_bytes,
        )?;
        let mut size = overhead;
        let mut count = 0usize;
        for event in &sub.events {
            let added = event.bytes + usize::from(count > 0);
            if size.saturating_add(added) > self.limits.max_frame_bytes {
                break;
            }
            size += added;
            count += 1;
        }
        let events = sub
            .events
            .iter()
            .take(count)
            .map(|event| event.event.as_ref())
            .collect::<Vec<_>>();
        let value = json!({"subscription_id":id,"events":events,"dropped":sub.dropped});
        let consumed = sub
            .events
            .iter()
            .take(count)
            .map(|event| event.bytes)
            .sum::<usize>();
        sub.events.drain(..count);
        sub.dropped = 0;
        state.queued_bytes = state.queued_bytes.saturating_sub(consumed);
        Ok(value)
    }
    /// Publishes a bounded notification from a trusted deployment producer.
    ///
    /// Full queues reject this event and increment visible counters. Consumers must
    /// rebuild state with queries after loss; no durable event delivery is implied.
    pub fn publish(&self, topic: &str, payload: Value) -> UiResult<UiPublishReport> {
        if !self.topics.contains_key(topic) {
            return Err(UiError::Forbidden);
        }
        crate::json_budget::json_bytes(&payload, self.limits.max_payload_bytes)?;
        let mut state = self.state.lock();
        if state.connection == UiConnectionState::Closed {
            return Err(UiError::Closed);
        }
        let sequence = next_sequence(state.next_event)?;
        let event = Arc::new(UiEventV1 {
            sequence,
            topic: topic.to_string(),
            payload,
        });
        let bytes = crate::json_budget::json_bytes(
            event.as_ref(),
            self.limits.max_frame_bytes.saturating_sub(1024),
        )?;
        state.next_event = sequence;
        let mut queued_bytes = state.queued_bytes;
        let mut report = UiPublishReport {
            delivered: 0,
            dropped: 0,
        };
        for sub in state
            .subscriptions
            .values_mut()
            .filter(|sub| sub.topic == topic)
        {
            if sub.events.len() >= self.limits.max_events_per_subscription
                || queued_bytes.saturating_add(bytes) > self.limits.max_queued_event_bytes
            {
                sub.dropped = sub.dropped.saturating_add(1).min(MAX_SAFE_SEQUENCE);
                report.dropped += 1;
            } else {
                sub.events.push_back(QueuedEvent {
                    event: Arc::clone(&event),
                    bytes,
                });
                queued_bytes += bytes;
                report.delivered += 1;
            }
        }
        state.queued_bytes = queued_bytes;
        state.dropped_events = state.dropped_events.saturating_add(report.dropped as u64);
        Ok(report)
    }
}
fn read_request(frame: &UiRequestV1, capability: &str) -> UiResult<CapabilityRequest> {
    Ok(CapabilityRequest {
        request_id: frame.request_id.clone(),
        capability: CapabilityName::new(capability.to_string())
            .map_err(|_| UiError::InvalidRequest)?,
        mode: CapabilityMode::Query,
        payload: b"null".to_vec(),
        idempotency_key: None,
        trace: None,
    })
}
fn next_sequence(previous: u64) -> UiResult<u64> {
    previous
        .checked_add(1)
        .filter(|sequence| *sequence <= MAX_SAFE_SEQUENCE)
        .ok_or(UiError::SequenceExhausted)
}
