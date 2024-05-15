// =============================================================================
//        #######
//     ###       ###     F: heartbeat_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:44:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{Heartbeat, HeartbeatSource, StaticHeartbeatSource};
use appcore_core::NodeId;

#[test]
fn heartbeat_uses_node_id() {
    let hb = Heartbeat {
        node_id: NodeId::new("node-a".to_string()).unwrap(),
        timestamp_ms: 123,
    };

    assert_eq!(hb.node_id, NodeId::new("node-a".to_string()).unwrap());
    assert_eq!(hb.timestamp_ms, 123);
}

struct MockHeartbeatSource;

impl HeartbeatSource for MockHeartbeatSource {
    fn heartbeat(&self) -> Heartbeat {
        Heartbeat {
            node_id: NodeId::new("node-mock".to_string()).unwrap(),
            timestamp_ms: 99,
        }
    }
}

#[test]
fn mock_heartbeat_source_works() {
    let source = MockHeartbeatSource;
    let hb = source.heartbeat();

    assert_eq!(hb.node_id, NodeId::new("node-mock".to_string()).unwrap());
    assert_eq!(hb.timestamp_ms, 99);
}

#[test]
fn static_heartbeat_contains_node_id() {
    let source = StaticHeartbeatSource::new(NodeId::new("node-static".to_string()).unwrap(), 1234);
    let hb = source.heartbeat();
    assert_eq!(hb.node_id, NodeId::new("node-static".to_string()).unwrap());
    assert_eq!(hb.timestamp_ms, 1234);
}
