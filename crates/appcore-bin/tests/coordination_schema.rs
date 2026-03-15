// =============================================================================
//        #######
//     ###       ###     F: coordination_schema.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 13:45:20 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use std::collections::BTreeSet;

const SCHEMA: &str = include_str!("../../appcore-control-plane/schema/appcore-coordination-v1.sql");
const SCHEMA_V2: &str =
    include_str!("../../appcore-control-plane/schema/appcore-coordination-v2.sql");

#[test]
fn coordination_migration_is_transactional_idempotent_and_infrastructure_only() {
    let normalized = SCHEMA.trim();
    assert!(normalized.starts_with("BEGIN;"));
    assert!(normalized.ends_with("COMMIT;"));
    assert!(!normalized.contains("DROP SCHEMA"));

    let tables = normalized
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("CREATE TABLE IF NOT EXISTS appcore.")
                .and_then(|tail| tail.split_whitespace().next())
                .map(|name| name.trim_end_matches("(").to_string())
        })
        .collect::<BTreeSet<_>>();
    let expected = [
        "audit",
        "capabilities",
        "jobs",
        "leases",
        "runtime_instances",
        "runtime_versions",
        "schema_migrations",
        "tenants",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<BTreeSet<_>>();
    assert_eq!(tables, expected);

    assert!(normalized.contains("ON CONFLICT (version) DO NOTHING;"));
    assert!(normalized.contains("VALUES (1, 'appcore-coordination-v1')"));
    for forbidden in [
        "appcore.customers",
        "appcore.products",
        "appcore.orders",
        "appcore.inventory",
        "appcore.payments",
        "password text",
        "private_key text",
        "bearer_token text",
    ] {
        assert!(!normalized.to_ascii_lowercase().contains(forbidden));
    }
}

#[test]
fn coordination_migration_checksum_is_reviewed() {
    use sha2::{Digest, Sha256};

    let checksum = Sha256::digest(SCHEMA.as_bytes());
    let actual = checksum
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        actual,
        "160e623c9a4344cd3290ed7f740fb854e6ed41b9c52fbd42d8cb339c26728335"
    );
}

#[test]
fn capability_class_migration_is_additive_and_bounded() {
    let normalized = SCHEMA_V2.trim();
    assert!(normalized.starts_with("BEGIN;"));
    assert!(normalized.ends_with("COMMIT;"));
    assert!(normalized.contains("ADD COLUMN IF NOT EXISTS capability_class"));
    assert!(normalized.contains("'infrastructure', 'functional'"));
    assert!(normalized.contains("VALUES (2, 'appcore-coordination-v2')"));
    assert!(!normalized.contains("DROP "));
}

#[test]
fn capability_class_migration_checksum_is_reviewed() {
    use sha2::{Digest, Sha256};

    let checksum = Sha256::digest(SCHEMA_V2.as_bytes());
    let actual = checksum
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        actual,
        "7b403c308e5afbfd2a0d77ccc054d4b276c928c4d52152776ff7d2f65f8d178b"
    );
}
