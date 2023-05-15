// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;

#[test]
fn leadership_is_always_scoped_to_a_service() {
    let service = ServiceId::new("document.extract").unwrap();
    let leadership =
        LeadershipRequirement::new(service.clone(), LeadershipMode::Required, 30_000).unwrap();
    let scheduling = SchedulingProfile::new(10, 5, 4, WorkloadClass::Compute).unwrap();
    let profile = CoreProfile::new(
        CoreRole::Compute,
        service,
        [CapabilityId::new("document.extract").unwrap()],
        leadership,
        ResourceProfile::new(Some(8), Some(16_000_000_000), 1),
        scheduling,
    );
    assert!(profile.is_ok());
}

#[test]
fn leadership_rejects_a_different_profile_service() {
    let leadership = LeadershipRequirement::new(
        ServiceId::new("storage.query").unwrap(),
        LeadershipMode::Required,
        30_000,
    )
    .unwrap();
    let scheduling = SchedulingProfile::new(1, 0, 1, WorkloadClass::General).unwrap();
    let result = CoreProfile::new(
        CoreRole::GeneralPurpose,
        ServiceId::new("document.extract").unwrap(),
        [],
        leadership,
        ResourceProfile::default(),
        scheduling,
    );
    assert!(result.is_err());
}
