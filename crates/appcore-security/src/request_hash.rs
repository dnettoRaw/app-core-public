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
use std::io::{self, Write};

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

/// Borrowed canonical payload used during request-bound token verification.
#[derive(Debug, Clone, Copy)]
pub enum RequestPayloadRef<'a> {
    /// An already canonical text payload.
    Text(&'a str),
    /// A structured payload serialized canonically by `serde_json`.
    Json(&'a serde_json::Value),
}

/// Borrowed request details that avoid copying an in-flight payload.
#[derive(Debug, Clone, Copy)]
pub struct RequestValidationDetailsRef<'a> {
    /// Request purpose.
    pub purpose: &'a str,
    /// Command or query name.
    pub name: &'a str,
    /// Request identity.
    pub id: &'a str,
    /// Optional idempotency key.
    pub idempotency_key: Option<&'a str>,
    /// Canonical text or structured JSON payload.
    pub payload: RequestPayloadRef<'a>,
    /// Optional authenticated subject.
    pub subject: Option<&'a str>,
    /// Optional target audience.
    pub audience: Option<&'a str>,
}

impl RequestValidationDetailsRef<'_> {
    /// Materializes the current owned contract for compatible verifiers.
    pub fn to_owned(self) -> Result<RequestValidationDetails, serde_json::Error> {
        let payload = match self.payload {
            RequestPayloadRef::Text(payload) => payload.to_string(),
            RequestPayloadRef::Json(payload) => serde_json::to_string(payload)?,
        };
        Ok(RequestValidationDetails {
            purpose: self.purpose.to_string(),
            name: self.name.to_string(),
            id: self.id.to_string(),
            idempotency_key: self.idempotency_key.map(str::to_string),
            payload,
            subject: self.subject.map(str::to_string),
            audience: self.audience.map(str::to_string),
        })
    }
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

/// Computes the same deterministic V2 hash without materializing borrowed JSON.
pub fn compute_borrowed_request_hash(
    details: &RequestValidationDetailsRef<'_>,
) -> Result<String, serde_json::Error> {
    let mut hasher = Sha256::new();
    hasher.update(REQUEST_HASH_DOMAIN_V2);
    update_required(&mut hasher, 1, details.purpose);
    update_required(&mut hasher, 2, details.name);
    update_required(&mut hasher, 3, details.id);
    update_optional(&mut hasher, 4, details.idempotency_key);
    update_payload(&mut hasher, 5, details.payload)?;
    update_optional(&mut hasher, 6, details.subject);
    update_optional(&mut hasher, 7, details.audience);

    let digest = hasher.finalize();
    let mut output = String::with_capacity(REQUEST_HASH_PREFIX_V2.len() + digest.len() * 2);
    output.push_str(REQUEST_HASH_PREFIX_V2);
    push_hex(&mut output, &digest);
    Ok(output)
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

fn update_payload(
    hasher: &mut Sha256,
    tag: u8,
    payload: RequestPayloadRef<'_>,
) -> Result<(), serde_json::Error> {
    match payload {
        RequestPayloadRef::Text(payload) => update_required(hasher, tag, payload),
        RequestPayloadRef::Json(payload) => {
            let mut counter = JsonByteCounter::default();
            serde_json::to_writer(&mut counter, payload)?;
            hasher.update([tag]);
            hasher.update(counter.bytes.to_be_bytes());
            serde_json::to_writer(JsonHashWriter(hasher), payload)?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct JsonByteCounter {
    bytes: u64,
}

impl Write for JsonByteCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let length = u64::try_from(bytes.len())
            .map_err(|_| io::Error::other("request payload length overflowed"))?;
        self.bytes = self
            .bytes
            .checked_add(length)
            .ok_or_else(|| io::Error::other("request payload length overflowed"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct JsonHashWriter<'a>(&'a mut Sha256);

impl Write for JsonHashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn push_hex(output: &mut String, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compute_borrowed_request_hash, compute_request_hash, RequestPayloadRef,
        RequestValidationDetails, RequestValidationDetailsRef,
    };
    use serde_json::json;

    #[test]
    fn borrowed_text_hash_matches_owned_contract() {
        let owned = owned_details("hello");
        let borrowed = borrowed_details(RequestPayloadRef::Text("hello"));

        assert_eq!(
            compute_borrowed_request_hash(&borrowed).unwrap(),
            compute_request_hash(&owned)
        );
    }

    #[test]
    fn borrowed_json_hash_matches_canonical_owned_contract() {
        let payload = json!({"text": "é日本語العربية", "values": [1, 2, 3]});
        let encoded = serde_json::to_string(&payload).unwrap();
        let owned = owned_details(&encoded);
        let borrowed = borrowed_details(RequestPayloadRef::Json(&payload));

        assert_eq!(
            compute_borrowed_request_hash(&borrowed).unwrap(),
            compute_request_hash(&owned)
        );
        assert_eq!(borrowed.to_owned().unwrap().payload, encoded);
    }

    fn owned_details(payload: &str) -> RequestValidationDetails {
        RequestValidationDetails {
            purpose: "query".to_string(),
            name: "runtime.status".to_string(),
            id: "query-1".to_string(),
            idempotency_key: Some("request-1".to_string()),
            payload: payload.to_string(),
            subject: Some("subject-1".to_string()),
            audience: Some("runtime".to_string()),
        }
    }

    fn borrowed_details(payload: RequestPayloadRef<'_>) -> RequestValidationDetailsRef<'_> {
        RequestValidationDetailsRef {
            purpose: "query",
            name: "runtime.status",
            id: "query-1",
            idempotency_key: Some("request-1"),
            payload,
            subject: Some("subject-1"),
            audience: Some("runtime"),
        }
    }
}
