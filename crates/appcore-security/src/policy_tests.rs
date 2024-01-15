// =============================================================================
//        #######
//     ###       ###     F: policy_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:44:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{PolicyCheck, PolicyDecision};
use appcore_core::CommandName;

struct MockPolicy {
    name: CommandName,
}

impl PolicyCheck for MockPolicy {
    fn command_name(&self) -> &CommandName {
        &self.name
    }

    fn evaluate(&self) -> PolicyDecision {
        PolicyDecision::Allow
    }
}

#[test]
fn mock_policy_works() {
    let policy = MockPolicy {
        name: CommandName::new("runtime.start".to_string()).unwrap(),
    };
    assert_eq!(
        policy.command_name(),
        &CommandName::new("runtime.start".to_string()).unwrap()
    );
    assert_eq!(policy.evaluate(), PolicyDecision::Allow);
}
