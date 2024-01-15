// =============================================================================
//        #######
//     ###       ###     F: secret_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{
    format_secret_material, new_rotated_secret, parse_secret_material, FileSecretResolver,
    PeerCredential, SecretBytes, SecretFormatError, SecretResolver, SecretStore,
    SecuritySecretMaterial, SecuritySecretMetadata, SecuritySecretRef, SecuritySecretStatus,
};
use crate::token::{SecurityError, SecurityResult};
use std::collections::HashMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

struct MockSecretStore {
    data: HashMap<String, Vec<u8>>,
}

impl MockSecretStore {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
}

impl SecretStore for MockSecretStore {
    fn put(&mut self, data: Vec<u8>) -> SecurityResult<SecuritySecretRef> {
        let id = format!("secret-{}", self.data.len() + 1);
        self.data.insert(id.clone(), data);
        Ok(SecuritySecretRef(id))
    }

    fn get(&self, reference: &SecuritySecretRef) -> SecurityResult<Vec<u8>> {
        let Some(value) = self.data.get(&reference.0) else {
            return Err(SecurityError::InvalidToken);
        };
        Ok(value.clone())
    }
}

#[test]
fn mock_secret_store_put_get_works() {
    let mut store = MockSecretStore::new();
    let reference = store.put(vec![1, 2, 3]);
    assert!(reference.is_ok());
    let reference = match reference {
        Ok(reference) => reference,
        Err(_) => return,
    };
    let value = store.get(&reference);
    assert!(value.is_ok());
    assert_eq!(value.ok(), Some(vec![1, 2, 3]));
}

#[test]
fn raw_secret_material_is_rejected_with_upgrade_wall() {
    assert_eq!(
        parse_secret_material(b"0123456789abcdef"),
        Err(SecretFormatError::InvalidFormat(
            "NO MORE SUPPORTED PLEASE UPDATE"
        ))
    );
}

#[test]
fn parse_structured_secret_works() {
    let input = "key_id=k1\ncreated_at_ms=1\nexpires_at_ms=none\nstatus=active\nsecret=hex:30313233343536373839616263646566\n";
    let parsed = parse_secret_material(input.as_bytes());
    assert!(parsed.is_ok());
    let parsed = match parsed {
        Ok(value) => value,
        Err(_) => return,
    };
    assert_eq!(parsed.metadata.key_id, "k1");
    assert_eq!(parsed.metadata.status, SecuritySecretStatus::Active);
    assert_eq!(parsed.secret, b"0123456789abcdef".to_vec());
}

#[test]
fn parse_structured_secret_rejects_short_material() {
    let input = "key_id=k1\ncreated_at_ms=1\nexpires_at_ms=none\nstatus=active\nsecret=short\n";
    assert_eq!(
        parse_secret_material(input.as_bytes()),
        Err(SecretFormatError::InvalidSecret)
    );
}

#[test]
fn format_roundtrip_works() {
    let material = SecuritySecretMaterial {
        secret: b"0123456789abcdef".to_vec(),
        metadata: SecuritySecretMetadata {
            key_id: "k1".to_string(),
            created_at_ms: 10,
            expires_at_ms: Some(20),
            status: SecuritySecretStatus::Deprecated,
        },
    };
    let text = format_secret_material(&material);
    let parsed = parse_secret_material(text.as_bytes());
    assert!(parsed.is_ok());
    assert_eq!(parsed.ok(), Some(material));
}

#[test]
fn rotated_secret_is_active() {
    let material = new_rotated_secret(None).expect("rotated secret");
    assert_eq!(material.metadata.status, SecuritySecretStatus::Active);
    assert_eq!(material.secret.len(), 32);
}

#[test]
fn rotated_secrets_are_different() {
    let first = new_rotated_secret(None).expect("first rotated secret");
    let second = new_rotated_secret(None).expect("second rotated secret");
    assert_ne!(first.secret, second.secret);
}

#[test]
fn secret_bytes_debug_is_redacted() {
    let secret = SecretBytes::new(b"super-secret-value".to_vec());
    let rendered = format!("{secret:?}");

    assert!(rendered.contains("REDACTED"));
    assert!(!rendered.contains("super-secret-value"));
}

#[test]
fn peer_credential_debug_is_redacted() {
    let credential = PeerCredential {
        key_id: "peer-key".to_string(),
        secret: SecretBytes::new(b"peer-secret-value".to_vec()),
    };
    let rendered = format!("{credential:?}");

    assert!(rendered.contains("REDACTED"));
    assert!(!rendered.contains("peer-secret-value"));
}

#[test]
fn file_secret_resolver_rejects_parent_escape() {
    let root = std::env::temp_dir().join(format!(
        "appcore-secret-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    ));
    assert!(fs::create_dir_all(&root).is_ok());
    let resolver = FileSecretResolver::new(&root);

    assert_eq!(
        resolver.resolve(&SecuritySecretRef("../secret".to_string())),
        Err(SecurityError::InvalidSecretRef)
    );
    let _ = fs::remove_dir_all(root);
}
