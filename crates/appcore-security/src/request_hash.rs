// =============================================================================
//        #######
//     ###       ###     F: request_hash.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Canonical request framing for request-bound Runtime credentials.

use sha2::{Digest, Sha256};

const REQUEST_HASH_DOMAIN_V2: &[u8] = b"appcore.request-hash.v2\0";
const REQUEST_HASH_PREFIX_V2: &str = "v2:";

/// Details of an incoming query or command request used to verify its integrity.
#[derive(Debug, Clone)]
pub struct RequestValidationDetails {
    /// Request purpose.
    pub purpose: String,
    /// Command or query name.
    pub name: String,
    /// Request identity.
    pub id: String,
    /// Optional idempotency key.
    pub idempotency_key: Option<String>,
    /// Canonical serialized payload.
    pub payload: String,
    /// Optional authenticated subject.
    pub subject: Option<String>,
    /// Optional target audience.
    pub audience: Option<String>,
}

/// Computes the deterministic V2 SHA-256 hash of a canonically framed request.
pub fn compute_request_hash(details: &RequestValidationDetails) -> String {
    let mut hasher = Sha256::new();
    hasher.update(REQUEST_HASH_DOMAIN_V2);
    update_required(&mut hasher, 1, &details.purpose);
    update_required(&mut hasher, 2, &details.name);
    update_required(&mut hasher, 3, &details.id);
    update_optional(&mut hasher, 4, details.idempotency_key.as_deref());
    update_required(&mut hasher, 5, &details.payload);
    update_optional(&mut hasher, 6, details.subject.as_deref());
    update_optional(&mut hasher, 7, details.audience.as_deref());

    let digest = hasher.finalize();
    let mut output = String::with_capacity(REQUEST_HASH_PREFIX_V2.len() + digest.len() * 2);
    output.push_str(REQUEST_HASH_PREFIX_V2);
    push_hex(&mut output, &digest);
    output
}

fn update_required(hasher: &mut Sha256, tag: u8, value: &str) {
    hasher.update([tag]);
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn update_optional(hasher: &mut Sha256, tag: u8, value: Option<&str>) {
    hasher.update([tag]);
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        None => hasher.update([0]),
    }
}

fn push_hex(output: &mut String, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
}
