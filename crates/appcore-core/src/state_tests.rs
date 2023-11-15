// =============================================================================
//        #######
//     ###       ###     F: state_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:44:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{StateMachine, StateRegistry, StateTransition};
use crate::ids::{EventName, StateName};

#[test]
fn register_state() {
    let mut registry = StateRegistry::new();
    let name = StateName::new("Running".to_string()).unwrap();

    let result = registry.register(name.clone());

    assert!(result.is_ok());
    assert!(registry.contains(&name));
    assert_eq!(registry.len(), 1);
}

#[test]
fn state_machine_new_sets_initial_state() {
    let machine = StateMachine::new(StateName::new("Booting".to_string()).unwrap());
    assert_eq!(
        machine.current(),
        &StateName::new("Booting".to_string()).unwrap()
    );
}

#[test]
fn add_transition_adds_transition() {
    let mut machine = StateMachine::new(StateName::new("Booting".to_string()).unwrap());
    let transition = StateTransition {
        from: StateName::new("Booting".to_string()).unwrap(),
        event: EventName::new("RuntimeStarted".to_string()).unwrap(),
        to: StateName::new("Running".to_string()).unwrap(),
    };

    let result = machine.add_transition(transition.clone());
    assert!(result.is_ok());
    assert_eq!(machine.transitions(), &[transition]);
}

#[test]
fn add_transition_rejects_duplicate() {
    let mut machine = StateMachine::new(StateName::new("Booting".to_string()).unwrap());
    let first = StateTransition {
        from: StateName::new("Booting".to_string()).unwrap(),
        event: EventName::new("RuntimeStarted".to_string()).unwrap(),
        to: StateName::new("Running".to_string()).unwrap(),
    };
    let second = StateTransition {
        from: StateName::new("Booting".to_string()).unwrap(),
        event: EventName::new("RuntimeStarted".to_string()).unwrap(),
        to: StateName::new("Degraded".to_string()).unwrap(),
    };

    assert!(machine.add_transition(first).is_ok());
    let duplicate_result = machine.add_transition(second);
    assert!(duplicate_result.is_err());
}

#[test]
fn can_apply_true_when_transition_exists() {
    let mut machine = StateMachine::new(StateName::new("Booting".to_string()).unwrap());
    assert!(machine
        .add_transition(StateTransition {
            from: StateName::new("Booting".to_string()).unwrap(),
            event: EventName::new("RuntimeStarted".to_string()).unwrap(),
            to: StateName::new("Running".to_string()).unwrap(),
        })
        .is_ok());

    assert!(machine.can_apply(&EventName::new("RuntimeStarted".to_string()).unwrap()));
}

#[test]
fn can_apply_false_when_transition_missing() {
    let machine = StateMachine::new(StateName::new("Booting".to_string()).unwrap());
    assert!(!machine.can_apply(&EventName::new("RuntimeStarted".to_string()).unwrap()));
}

#[test]
fn apply_changes_state() {
    let mut machine = StateMachine::new(StateName::new("Booting".to_string()).unwrap());
    assert!(machine
        .add_transition(StateTransition {
            from: StateName::new("Booting".to_string()).unwrap(),
            event: EventName::new("RuntimeStarted".to_string()).unwrap(),
            to: StateName::new("Running".to_string()).unwrap(),
        })
        .is_ok());

    let result = machine.apply(&EventName::new("RuntimeStarted".to_string()).unwrap());
    assert!(result.is_ok());
    assert_eq!(
        machine.current(),
        &StateName::new("Running".to_string()).unwrap()
    );
}

#[test]
fn apply_rejects_invalid_transition() {
    let mut machine = StateMachine::new(StateName::new("Booting".to_string()).unwrap());
    let result = machine.apply(&EventName::new("RuntimeStarted".to_string()).unwrap());
    assert!(result.is_err());
}

#[test]
fn transitions_exposes_list() {
    let mut machine = StateMachine::new(StateName::new("Booting".to_string()).unwrap());
    let transition = StateTransition {
        from: StateName::new("Booting".to_string()).unwrap(),
        event: EventName::new("RuntimeStarted".to_string()).unwrap(),
        to: StateName::new("Running".to_string()).unwrap(),
    };
    assert!(machine.add_transition(transition.clone()).is_ok());
    assert_eq!(machine.transitions(), &[transition]);
}
