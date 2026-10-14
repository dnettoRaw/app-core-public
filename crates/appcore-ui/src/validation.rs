// =============================================================================
//        #######
//     ###       ###     F: validation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Bound encoded input before decode, then validate operation semantics before admission.
use crate::{UiActionV1, UiError, UiLimits, UiRequestV1, UiResult, UI_PROTOCOL_V1};
/// Decodes only UI V1 after checking the complete input budget.
pub fn decode_ui_request_v1(bytes: &[u8], limits: UiLimits) -> UiResult<UiRequestV1> {
    let limits = limits.validate()?;
    if bytes.len() > limits.max_frame_bytes {
        return Err(UiError::PayloadTooLarge);
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| UiError::InvalidFrame)?;
    if value.get("version").and_then(serde_json::Value::as_u64) != Some(u64::from(UI_PROTOCOL_V1)) {
        return Err(UiError::UnsupportedVersion);
    }
    let request: UiRequestV1 = serde_json::from_value(value).map_err(|_| UiError::InvalidFrame)?;
    request.validate(limits)?;
    Ok(request)
}
impl UiRequestV1 {
    /// Validates a frame built directly in Rust as well as decoded wire input.
    pub fn validate(&self, limits: UiLimits) -> UiResult<()> {
        let limits = limits.validate()?;
        if self.version != UI_PROTOCOL_V1 {
            return Err(UiError::UnsupportedVersion);
        }
        validate_name(&self.surface_id)?;
        validate_name(&self.request_id)?;
        if self.timeout_ms == 0 || self.timeout_ms > limits.max_timeout_ms {
            return Err(UiError::InvalidRequest);
        }
        match &self.action {
            UiActionV1::Query { query } => {
                query
                    .validate(limits.max_payload_bytes)
                    .map_err(|error| match error {
                        appcore_api::QueryRequestValidationError::PayloadTooLarge => {
                            UiError::PayloadTooLarge
                        }
                        _ => UiError::InvalidRequest,
                    })?;
                if query.query_id != self.request_id {
                    return Err(UiError::InvalidRequest);
                }
            }
            UiActionV1::Command { command } => {
                if command.idempotency_key.is_none() {
                    return Err(UiError::IdempotencyRequired);
                }
                command
                    .validate(limits.max_payload_bytes)
                    .map_err(|error| match error {
                        appcore_api::CommandRequestValidationError::PayloadTooLarge => {
                            UiError::PayloadTooLarge
                        }
                        _ => UiError::InvalidRequest,
                    })?;
                if command.command_id != self.request_id {
                    return Err(UiError::InvalidRequest);
                }
            }
            UiActionV1::Subscribe { capability, topic } => {
                validate_name(capability)?;
                validate_name(topic)?;
            }
            UiActionV1::Poll { subscription_id } | UiActionV1::Unsubscribe { subscription_id } => {
                validate_name(subscription_id)?;
            }
            UiActionV1::Cancel { target_request_id } => {
                validate_name(target_request_id)?;
            }
            UiActionV1::Close => {}
        }
        crate::json_budget::json_bytes(self, limits.max_frame_bytes)?;
        Ok(())
    }
}
pub(crate) fn validate_name(value: &str) -> UiResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(UiError::InvalidRequest);
    }
    Ok(())
}
