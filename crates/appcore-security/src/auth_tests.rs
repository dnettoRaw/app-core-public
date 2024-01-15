// =============================================================================
//        #######
//     ###       ###     F: auth_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:44:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{AuthContext, AuthDecision, Authenticator};
use appcore_core::{AppId, CommandName, NodeId};

struct MockAuthenticator;

impl Authenticator for MockAuthenticator {
    fn authenticate(&self, _context: &AuthContext) -> AuthDecision {
        AuthDecision::Allow
    }
}

#[test]
fn mock_authenticator_works() {
    let auth = MockAuthenticator;
    let context = AuthContext {
        app_id: AppId::new("example-app".to_string()).unwrap(),
        node_id: NodeId::new("node-a".to_string()).unwrap(),
        command_name: CommandName::new("runtime.start".to_string()).unwrap(),
    };
    assert_eq!(auth.authenticate(&context), AuthDecision::Allow);
}
