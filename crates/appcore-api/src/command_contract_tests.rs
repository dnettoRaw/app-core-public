// =============================================================================
//        #######
//     ###       ###     F: command_contract_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{CommandRequest, CommandRequestValidationError, CommandResponse, CommandResponseEvent};
use appcore_core::{AppId, NodeId};

#[test]
fn request_validation_rules() {
    let valid = CommandRequest {
        command_name: "runtime.ping".to_string(),
        command_id: "cmd-1".to_string(),
        idempotency_key: Some("idemp-1".to_string()),
        payload: "ok".to_string(),
    };
    assert!(valid.validate(65_536).is_ok());

    let empty_name = CommandRequest {
        command_name: "".to_string(),
        ..valid.clone()
    };
    assert_eq!(
        empty_name.validate(65_536),
        Err(CommandRequestValidationError::EmptyCommandName)
    );

    let empty_id = CommandRequest {
        command_id: "".to_string(),
        ..valid.clone()
    };
    assert_eq!(
        empty_id.validate(65_536),
        Err(CommandRequestValidationError::EmptyCommandId)
    );

    let invalid_name = CommandRequest {
        command_name: "runtime ping".to_string(),
        ..valid.clone()
    };
    assert_eq!(
        invalid_name.validate(65_536),
        Err(CommandRequestValidationError::InvalidCommandName)
    );

    let invalid_key = CommandRequest {
        idempotency_key: Some("".to_string()),
        ..valid.clone()
    };
    assert_eq!(
        invalid_key.validate(65_536),
        Err(CommandRequestValidationError::InvalidIdempotencyKey)
    );

    let missing_key = CommandRequest {
        command_name: "runtime.update".to_string(),
        idempotency_key: None,
        ..valid.clone()
    };
    assert_eq!(
        missing_key.validate(65_536),
        Err(CommandRequestValidationError::MissingIdempotencyKey)
    );

    let too_large = CommandRequest {
        payload: "x".repeat(9),
        ..valid
    };
    assert_eq!(
        too_large.validate(8),
        Err(CommandRequestValidationError::PayloadTooLarge)
    );
}

#[test]
fn payload_bytes_exposes_utf8_bytes() {
    let req = CommandRequest {
        command_name: "runtime.ping".to_string(),
        command_id: "cmd-1".to_string(),
        idempotency_key: None,
        payload: "hello".to_string(),
    };
    assert_eq!(req.payload_bytes(), b"hello");
}

#[test]
fn response_helpers_work() {
    let accepted = CommandResponse::accepted(vec![CommandResponseEvent {
        event_name: "runtime.pong".to_string(),
        event_id: "evt-1".to_string(),
    }]);
    assert!(accepted.accepted);
    assert_eq!(accepted.events.len(), 1);

    let rejected = CommandResponse::rejected("nope");
    assert!(!rejected.accepted);
    assert_eq!(rejected.message.as_deref(), Some("nope"));
}

#[test]
fn command_v1_golden_fixtures_are_stable() {
    let request: CommandRequest =
        serde_json::from_str(include_str!("fixtures/command-request-v1.json")).unwrap();
    let request_round_trip = serde_json::to_value(&request).unwrap();
    let request_golden: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/command-request-v1.json")).unwrap();
    assert_eq!(request_round_trip, request_golden);

    let response: CommandResponse =
        serde_json::from_str(include_str!("fixtures/command-response-v1.json")).unwrap();
    let response_round_trip = serde_json::to_value(&response).unwrap();
    let response_golden: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/command-response-v1.json")).unwrap();
    assert_eq!(response_round_trip, response_golden);
}

#[test]
fn to_envelope_maps_fields() {
    let req = CommandRequest {
        command_name: "runtime.ping".to_string(),
        command_id: "cmd-1".to_string(),
        idempotency_key: Some("idemp-1".to_string()),
        payload: "hello".to_string(),
    };
    let env = req.to_envelope(
        AppId::new("minimal-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        42,
        65_536,
    );
    assert!(env.is_ok());
    let env = match env {
        Ok(env) => env,
        Err(_) => return,
    };
    assert_eq!(env.command_name.as_str(), "runtime.ping");
    assert_eq!(env.command_id, "cmd-1");
    assert_eq!(env.idempotency_key.as_deref(), Some("idemp-1"));
    assert_eq!(env.payload, b"hello".to_vec());
}

#[test]
fn to_envelope_rejects_large_payload() {
    let req = CommandRequest {
        command_name: "runtime.ping".to_string(),
        command_id: "cmd-1".to_string(),
        idempotency_key: Some("idemp-1".to_string()),
        payload: "123456789".to_string(),
    };
    let env = req.to_envelope(
        AppId::new("minimal-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        42,
        8,
    );
    assert!(env.is_err());
}

#[test]
fn to_envelope_rejects_command_id_with_tabs_or_newlines() {
    for bad_id in &["cmd\t1", "cmd\n1", "cmd\r1"] {
        let req = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: bad_id.to_string(),
            idempotency_key: None,
            payload: "hello".to_string(),
        };
        let env = req.to_envelope(
            AppId::new("minimal-app".to_string()).unwrap(),
            NodeId::new("node-a".to_string()).unwrap(),
            42,
            65_536,
        );
        assert!(env.is_err(), "should reject command_id: {:?}", bad_id);
    }
}
