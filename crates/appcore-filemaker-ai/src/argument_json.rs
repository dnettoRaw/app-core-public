// =============================================================================
//        #######
//     ###       ###     F: argument_json.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Parses bounded tool arguments around cooperative cancellation checkpoints.

use appcore_filemaker::OperationControl;
use serde_json::Value;

use crate::{error::json_error, BridgeResult};

const CONTROLLED_JSON_BUFFER_BYTES: usize = 16 * 1024;

pub(crate) fn parse(arguments_json: &str, control: &OperationControl) -> BridgeResult<Value> {
    control.cancellation().check()?;
    for _ in arguments_json
        .as_bytes()
        .chunks(CONTROLLED_JSON_BUFFER_BYTES)
    {
        control.cancellation().check()?;
    }
    let parsed = serde_json::from_str(arguments_json);
    control.cancellation().check()?;
    parsed.map_err(json_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore_filemaker::{CancellationToken, ErrorCode};

    #[test]
    fn parser_preserves_small_and_large_json() {
        let control = OperationControl::default();
        assert_eq!(parse(r#"{"id":"box"}"#, &control).unwrap()["id"], "box");

        let payload = "x".repeat(CONTROLLED_JSON_BUFFER_BYTES + 1);
        let source = serde_json::json!({"payload": payload}).to_string();
        assert_eq!(parse(&source, &control).unwrap()["payload"], payload);
    }

    #[test]
    fn parser_rejects_cancelled_large_json() {
        let cancellation = CancellationToken::default();
        let control = OperationControl::new(cancellation.clone());
        cancellation.cancel();
        assert!(matches!(
            parse(
                &serde_json::json!({"payload": "x".repeat(20_000)}).to_string(),
                &control
            ),
            Err(crate::BridgeError::Core(error)) if error.code() == ErrorCode::Cancelled
        ));
    }
}
