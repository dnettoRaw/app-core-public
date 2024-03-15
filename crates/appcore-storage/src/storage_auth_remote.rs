// =============================================================================
//        #######
//     ###       ###     F: storage_auth_remote.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/07 12:31:50 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Remote auth-storage protocol shared by the runtime host and the auth-server.

use super::{StorageError, StorageResult};
use appcore_security::{parse_secret_material, HashTokenProvider, TokenClaims, TokenProvider};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Component, Path};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

/// Auth-storage protocol schema.
pub const AUTH_REMOTE_SCHEMA: &str = "appcore.auth-remote.v1";
/// Auth-storage HTTP endpoint.
pub const AUTH_REMOTE_ENDPOINT: &str = "/auth/storage";
/// Default maximum request or response payload.
pub const DEFAULT_AUTH_REMOTE_MAX_BYTES: usize = 1024 * 1024;
/// Default sealed request lifetime.
pub const DEFAULT_AUTH_REMOTE_TTL_MS: u64 = 10_000;
/// Default network deadline.
pub const DEFAULT_AUTH_REMOTE_TIMEOUT_MS: u64 = 5_000;

/// Sealed remote storage operation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthRemoteRequest {
    /// Protocol schema.
    pub schema: String,
    /// Relative storage resource.
    pub resource: String,
    /// `seal` or `open` operation.
    pub operation: String,
    /// Single-request correlation value.
    pub nonce: String,
    /// Issue timestamp in Unix milliseconds.
    pub issued_at_ms: u64,
    /// Expiry timestamp in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Hex-encoded opaque payload.
    pub payload_hex: String,
}

/// Sealed remote storage operation response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthRemoteResponse {
    /// Protocol schema.
    pub schema: String,
    /// Controlled response status.
    pub status: String,
    /// Request nonce echoed by the service.
    pub nonce: String,
    /// Expiry timestamp in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Hex-encoded opaque payload.
    pub payload_hex: String,
}

/// Bounded synchronous client for the dedicated auth-storage service.
#[derive(Debug, Clone)]
pub struct RemoteAuthStorageClient {
    address: String,
    provider: HashTokenProvider,
    timeout_ms: u64,
    max_response_bytes: usize,
}

impl RemoteAuthStorageClient {
    /// Creates a client with default deadline and response bound.
    pub fn new(address: impl Into<String>, provider: HashTokenProvider) -> Self {
        Self {
            address: address.into(),
            provider,
            timeout_ms: DEFAULT_AUTH_REMOTE_TIMEOUT_MS,
            max_response_bytes: DEFAULT_AUTH_REMOTE_MAX_BYTES,
        }
    }

    /// Creates a client from deployment-owned structured secret material.
    pub fn from_secret_file(address: impl Into<String>, path: &Path) -> StorageResult<Self> {
        let raw = std::fs::read(path)
            .map_err(|_| StorageError::AuthUnavailable(path.display().to_string()))?;
        let material = parse_secret_material(&raw)
            .map_err(|_| StorageError::SecurityFailed(path.display().to_string()))?;
        let provider = HashTokenProvider::from_secret(material.secret.clone())
            .map_err(|_| StorageError::SecurityFailed(path.display().to_string()))?;
        Ok(Self::new(address, provider))
    }

    /// Overrides the network deadline in milliseconds.
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Requests authenticated sealing of one storage resource.
    pub fn seal_resource(&self, resource: &str, bytes: &[u8]) -> StorageResult<Vec<u8>> {
        self.call(resource, "seal", bytes)
    }

    /// Requests authenticated opening of one storage resource.
    pub fn open_resource(&self, resource: &str, sealed: &[u8]) -> StorageResult<Vec<u8>> {
        self.call(resource, "open", sealed)
    }

    fn call(&self, resource: &str, operation: &str, payload: &[u8]) -> StorageResult<Vec<u8>> {
        let now = now_ms();
        let request = make_auth_request(resource, operation, payload, now)?;
        let nonce = request.nonce.clone();
        let token = seal_remote_request(&request, &self.provider)?;
        let response = self.post_token(&token)?;
        open_remote_response(&response, &self.provider, &nonce, now_ms())
    }

    fn post_token(&self, token: &str) -> StorageResult<String> {
        let mut stream = self.connect()?;
        let request = http_request(&self.address, token);
        stream
            .write_all(request.as_bytes())
            .map_err(|_| StorageError::AuthUnavailable(self.address.clone()))?;
        read_http_response(&mut stream, self.max_response_bytes)
    }

    fn connect(&self) -> StorageResult<TcpStream> {
        let timeout = Duration::from_millis(self.timeout_ms);
        let address = self
            .address
            .to_socket_addrs()
            .map_err(|_| StorageError::AuthUnavailable(self.address.clone()))?
            .next()
            .ok_or_else(|| StorageError::AuthUnavailable(self.address.clone()))?;
        let stream = TcpStream::connect_timeout(&address, timeout)
            .map_err(|_| StorageError::AuthUnavailable(self.address.clone()))?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|_| StorageError::AuthUnavailable(self.address.clone()))?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|_| StorageError::AuthUnavailable(self.address.clone()))?;
        Ok(stream)
    }
}

/// Builds a validated short-lived remote auth request.
pub fn make_auth_request(
    resource: &str,
    operation: &str,
    payload: &[u8],
    now_ms: u64,
) -> StorageResult<AuthRemoteRequest> {
    validate_resource(resource)?;
    validate_operation(operation)?;
    Ok(AuthRemoteRequest {
        schema: AUTH_REMOTE_SCHEMA.to_string(),
        resource: resource.to_string(),
        operation: operation.to_string(),
        nonce: make_nonce(now_ms),
        issued_at_ms: now_ms,
        expires_at_ms: now_ms.saturating_add(DEFAULT_AUTH_REMOTE_TTL_MS),
        payload_hex: encode_hex(payload),
    })
}

/// Seals a remote auth request for transport.
pub fn seal_remote_request<P: TokenProvider>(
    request: &AuthRemoteRequest,
    provider: &P,
) -> StorageResult<String> {
    let payload = serde_json::to_vec(request)
        .map_err(|_| StorageError::SecurityFailed(request.resource.clone()))?;
    let token = provider
        .seal(&payload, &transport_claims(DEFAULT_AUTH_REMOTE_TTL_MS))
        .map_err(|_| StorageError::SecurityFailed(request.resource.clone()))?;
    String::from_utf8(token).map_err(|_| StorageError::SecurityFailed(request.resource.clone()))
}

/// Opens and validates a remote auth request.
pub fn open_remote_request<P: TokenProvider>(
    token: &str,
    provider: &P,
    now_ms: u64,
) -> StorageResult<AuthRemoteRequest> {
    let bytes = provider
        .open(
            token.as_bytes(),
            &transport_claims(DEFAULT_AUTH_REMOTE_TTL_MS),
        )
        .map_err(|_| StorageError::SecurityFailed("auth transport".to_string()))?;
    let request = serde_json::from_slice::<AuthRemoteRequest>(&bytes)
        .map_err(|_| StorageError::SecurityFailed("auth request".to_string()))?;
    validate_request(&request, now_ms, DEFAULT_AUTH_REMOTE_MAX_BYTES)?;
    Ok(request)
}

/// Seals a remote auth response for transport.
pub fn seal_remote_response<P: TokenProvider>(
    response: &AuthRemoteResponse,
    provider: &P,
) -> StorageResult<String> {
    let payload = serde_json::to_vec(response)
        .map_err(|_| StorageError::SecurityFailed(response.nonce.clone()))?;
    let token = provider
        .seal(&payload, &transport_claims(DEFAULT_AUTH_REMOTE_TTL_MS))
        .map_err(|_| StorageError::SecurityFailed(response.nonce.clone()))?;
    String::from_utf8(token).map_err(|_| StorageError::SecurityFailed(response.nonce.clone()))
}

/// Opens and validates a remote auth response and request nonce.
pub fn open_remote_response<P: TokenProvider>(
    token: &str,
    provider: &P,
    expected_nonce: &str,
    now_ms: u64,
) -> StorageResult<Vec<u8>> {
    let bytes = provider
        .open(
            token.as_bytes(),
            &transport_claims(DEFAULT_AUTH_REMOTE_TTL_MS),
        )
        .map_err(|_| StorageError::SecurityFailed("auth response".to_string()))?;
    let response = serde_json::from_slice::<AuthRemoteResponse>(&bytes)
        .map_err(|_| StorageError::SecurityFailed("auth response".to_string()))?;
    validate_response(&response, expected_nonce, now_ms)?;
    decode_hex(&response.payload_hex).ok_or(StorageError::SecurityFailed(response.nonce))
}

/// Executes a validated seal or open request against the data key.
pub fn process_remote_request<P: TokenProvider>(
    request: &AuthRemoteRequest,
    data_provider: &P,
) -> StorageResult<AuthRemoteResponse> {
    let payload = decode_hex(&request.payload_hex)
        .ok_or_else(|| StorageError::SecurityFailed(request.resource.clone()))?;
    let output = match request.operation.as_str() {
        "seal" => data_provider.seal(&payload, &data_claims()),
        "open" => data_provider.open(&payload, &data_claims()),
        _ => return Err(StorageError::SecurityFailed(request.resource.clone())),
    };
    let payload_hex = output
        .map(|bytes| encode_hex(&bytes))
        .map_err(|_| StorageError::SecurityFailed(request.resource.clone()))?;
    Ok(AuthRemoteResponse {
        schema: AUTH_REMOTE_SCHEMA.to_string(),
        status: "ok".to_string(),
        nonce: request.nonce.clone(),
        expires_at_ms: request.expires_at_ms,
        payload_hex,
    })
}

/// Validates a relative auth-storage resource name.
pub fn validate_auth_resource(resource: &str) -> StorageResult<()> {
    validate_resource(resource)
}

/// Returns isolated claims for auth-service transport.
pub fn transport_claims(ttl_ms: u64) -> TokenClaims {
    TokenClaims {
        issuer: "appcore-runtime-host".to_string(),
        audience: "appcore-auth-server".to_string(),
        salt: "auth-remote-transport-v1".to_string(),
        ttl_ms,
    }
}

/// Returns isolated claims for stored data encryption.
pub fn data_claims() -> TokenClaims {
    TokenClaims {
        issuer: "appcore-auth-server".to_string(),
        audience: "appcore-auth-storage".to_string(),
        salt: "auth-remote-data-v1".to_string(),
        ttl_ms: 0,
    }
}

/// Returns current Unix time in milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn validate_request(
    request: &AuthRemoteRequest,
    now_ms: u64,
    max_payload_bytes: usize,
) -> StorageResult<()> {
    if request.schema != AUTH_REMOTE_SCHEMA || request.expires_at_ms <= now_ms {
        return Err(StorageError::SecurityFailed(request.resource.clone()));
    }
    if request.nonce.is_empty() || request.issued_at_ms >= request.expires_at_ms {
        return Err(StorageError::SecurityFailed(request.resource.clone()));
    }
    validate_resource(&request.resource)?;
    validate_operation(&request.operation)?;
    if request.payload_hex.len() / 2 > max_payload_bytes {
        return Err(StorageError::SecurityFailed(request.resource.clone()));
    }
    Ok(())
}

fn http_request(host: &str, body: &str) -> String {
    format!(
        "POST {AUTH_REMOTE_ENDPOINT} HTTP/1.1\r\nHost: {host}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn read_http_response(stream: &mut TcpStream, max_bytes: usize) -> StorageResult<String> {
    let raw = read_limited(stream, max_bytes)?;
    let text = String::from_utf8(raw)
        .map_err(|_| StorageError::SecurityFailed("auth response".to_string()))?;
    parse_http_response(&text)
}

fn read_limited(stream: &mut TcpStream, max_bytes: usize) -> StorageResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if out.len() > max_bytes {
                    return Err(StorageError::SecurityFailed(
                        "auth response too large".into(),
                    ));
                }
            }
            Err(_) => return Err(StorageError::AuthUnavailable("auth-server".to_string())),
        }
    }
    Ok(out)
}

fn parse_http_response(text: &str) -> StorageResult<String> {
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| StorageError::SecurityFailed("malformed auth response".to_string()))?;
    let status = http_status(head)?;
    if (200..300).contains(&status) {
        return Ok(body.to_string());
    }
    if status == 503 {
        return Err(StorageError::AuthUnavailable("auth-server".to_string()));
    }
    Err(StorageError::SecurityFailed(format!(
        "auth status {status}"
    )))
}

fn http_status(head: &str) -> StorageResult<u16> {
    let line = head
        .lines()
        .next()
        .ok_or_else(|| StorageError::SecurityFailed("missing auth status".to_string()))?;
    line.split_whitespace()
        .nth(1)
        .ok_or_else(|| StorageError::SecurityFailed("missing auth status".to_string()))?
        .parse::<u16>()
        .map_err(|_| StorageError::SecurityFailed("invalid auth status".to_string()))
}

fn validate_response(
    response: &AuthRemoteResponse,
    expected_nonce: &str,
    now_ms: u64,
) -> StorageResult<()> {
    if response.schema != AUTH_REMOTE_SCHEMA || response.status != "ok" {
        return Err(StorageError::SecurityFailed("auth response".to_string()));
    }
    if response.nonce != expected_nonce || response.expires_at_ms <= now_ms {
        return Err(StorageError::SecurityFailed("auth response".to_string()));
    }
    Ok(())
}

fn validate_operation(operation: &str) -> StorageResult<()> {
    if operation == "seal" || operation == "open" {
        return Ok(());
    }
    Err(StorageError::SecurityFailed(operation.to_string()))
}

fn validate_resource(resource: &str) -> StorageResult<()> {
    let path = Path::new(resource);
    if resource.is_empty() || resource.len() > 256 || path.is_absolute() {
        return Err(StorageError::InvalidPath(resource.to_string()));
    }
    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(StorageError::InvalidPath(resource.to_string()));
        }
    }
    Ok(())
}

fn make_nonce(now_ms: u64) -> String {
    format!("{now_ms}-{}-{}", std::process::id(), nonce_counter())
}

fn nonce_counter() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local nonce reuse
    static NONCE_COUNTER: AtomicUsize = AtomicUsize::new(0);
    NONCE_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[allow(unknown_lints)]
#[allow(clippy::manual_is_multiple_of)]
fn decode_hex(input: &str) -> Option<Vec<u8>> {
    if input.len() % 2 != 0 {
        return None;
    }
    let mut output = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        output.push((hex_value(bytes[i])? << 4) | hex_value(bytes[i + 1])?);
    }
    Some(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[path = "storage_auth_remote_tests.rs"]
mod tests;
