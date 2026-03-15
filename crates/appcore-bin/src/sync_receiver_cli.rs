// =============================================================================
//        #######
//     ###       ###     F: sync_receiver_cli.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/06 20:53:23 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/26 10:16:57 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Local follower receiver used by the sync CLI/runtime host.

use crate::bootstrap::{now_ms, BootstrapError};
use crate::server::RuntimeServer;
use crate::sync_cli::{discovered_sync_peers, push_sync_to_peer_addresses};
use appcore_security::{CommandTokenValidator, HashTokenProvider, TokenClaims};
use appcore_supervisor::ManagedService;
use appcore_sync::{decode_sync_envelope, SyncReceiverState};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

const MAX_SYNC_REQUEST_BODY_BYTES: usize = 1024 * 1024;

pub(crate) fn sync_service_if_enabled(
    server: &RuntimeServer,
) -> Result<Option<Arc<dyn ManagedService>>, BootstrapError> {
    if !server.app.config.sync_enabled {
        return Ok(None);
    }
    if server.app.config.sync_role == "leader" {
        return sync_push_service_if_enabled(server);
    }
    if server.app.config.sync_role != "follower" {
        return Ok(None);
    }
    let Some(replication_log) = &server.app.replication_log else {
        return Ok(None);
    };
    let Some(checkpoint_store) = &server.app.checkpoint_store else {
        return Err(BootstrapError::Runtime(
            "missing sync checkpoint store".to_string(),
        ));
    };
    let state = SyncReceiverState::new(Arc::clone(replication_log), checkpoint_store.clone())
        .with_local_identity(server.app.core_identity.clone());
    let auth = build_receiver_auth(server)?;
    let host = server.app.config.sync_bind_host.clone();
    let port = server.app.config.sync_bind_port;
    let descriptor = crate::runtime_services::service_descriptor(
        crate::runtime_services::SYNC_SERVICE,
        appcore_supervisor::ManagedResource::Sync,
        &[crate::runtime_services::SECURITY_SERVICE],
    )?;
    Ok(Some(Arc::new(
        appcore_supervisor::ManagedThreadService::new(descriptor, move |shutdown| {
            let host = host.clone();
            let state = state.clone();
            let auth = auth.clone();
            thread::Builder::new()
                .name("appcore-sync".to_string())
                .spawn(move || {
                    run_sync_receiver(host, port, shutdown, state, auth)
                        .map_err(|error| error.to_string())
                })
                .map_err(|error| error.to_string())
        }),
    )))
}

fn sync_push_service_if_enabled(
    server: &RuntimeServer,
) -> Result<Option<Arc<dyn ManagedService>>, BootstrapError> {
    if server.app.replication_log.is_none() {
        return Ok(None);
    }
    let config = server.app.config.clone();
    let replication_log = server.app.replication_log.clone();
    let security_provider = server.app.security_provider.clone();
    let peer_directory = Arc::clone(&server.app.peer_directory);
    let interval_ms = config.sync_push_every_ticks.max(1).saturating_mul(1_000);
    let descriptor = crate::runtime_services::service_descriptor(
        crate::runtime_services::SYNC_SERVICE,
        appcore_supervisor::ManagedResource::Sync,
        &[
            crate::runtime_services::SECURITY_SERVICE,
            crate::runtime_services::CONTROL_PLANE_SERVICE,
        ],
    )?;
    Ok(Some(Arc::new(
        appcore_supervisor::ManagedThreadService::new(descriptor, move |shutdown| {
            let config = config.clone();
            let replication_log = replication_log.clone();
            let security_provider = security_provider.clone();
            let peer_directory = Arc::clone(&peer_directory);
            thread::Builder::new()
                .name("appcore-sync-push".to_string())
                .spawn(move || {
                    run_sync_push_loop(
                        config,
                        replication_log,
                        security_provider,
                        peer_directory,
                        interval_ms,
                        shutdown,
                    );
                    Ok(())
                })
                .map_err(|error| error.to_string())
        }),
    )))
}

fn run_sync_push_loop(
    config: crate::runtime_config::RuntimeConfig,
    replication_log: Option<Arc<parking_lot::Mutex<Box<dyn appcore_sync::ReplicationLog + Send>>>>,
    security_provider: HashTokenProvider,
    peer_directory: Arc<parking_lot::Mutex<Option<appcore_control_plane::PeerDirectory>>>,
    interval_ms: u64,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::SeqCst) {
        if let Ok(peers) = discovered_sync_peers(peer_directory.lock().as_ref()) {
            let _ = push_sync_to_peer_addresses(
                &config,
                replication_log.as_ref(),
                Some(&security_provider),
                peers,
            );
        }
        sleep_or_shutdown(&shutdown, interval_ms);
    }
}

fn sleep_or_shutdown(shutdown: &AtomicBool, interval_ms: u64) {
    let mut slept = 0u64;
    let interval = interval_ms.max(1_000);
    while slept < interval && !shutdown.load(Ordering::SeqCst) {
        let step = (interval - slept).min(100);
        thread::sleep(std::time::Duration::from_millis(step));
        slept = slept.saturating_add(step);
    }
}

#[derive(Clone)]
struct ReceiverAuth {
    required: bool,
    validator: Option<(HashTokenProvider, TokenClaims)>,
}

fn build_receiver_auth(server: &RuntimeServer) -> Result<ReceiverAuth, BootstrapError> {
    let required = server.app.config.sync_require_token;
    let validator = if required {
        Some(build_sync_validator(server)?)
    } else {
        None
    };
    Ok(ReceiverAuth {
        required,
        validator,
    })
}

fn build_sync_validator(
    server: &RuntimeServer,
) -> Result<(HashTokenProvider, TokenClaims), BootstrapError> {
    let provider = server.app.security_provider.clone();
    let claims = TokenClaims {
        issuer: server.app.config.token_issuer.clone(),
        audience: server.app.config.token_audience.clone(),
        salt: "sync".to_string(),
        ttl_ms: 60_000,
    };
    Ok((provider, claims))
}

fn run_sync_receiver(
    host: String,
    port: u16,
    shutdown: Arc<AtomicBool>,
    state: SyncReceiverState,
    auth: ReceiverAuth,
) -> Result<(), BootstrapError> {
    let listener = bind_sync_listener(&host, port)?;
    while !shutdown.load(Ordering::SeqCst) {
        accept_sync_client(&listener, &state, &auth)?;
    }
    Ok(())
}

fn bind_sync_listener(host: &str, port: u16) -> Result<TcpListener, BootstrapError> {
    let listener = TcpListener::bind((host, port))
        .map_err(|_| BootstrapError::Runtime("failed to bind sync receiver".to_string()))?;
    listener
        .set_nonblocking(true)
        .map_err(|_| BootstrapError::Runtime("failed to set sync receiver mode".to_string()))?;
    Ok(listener)
}

fn accept_sync_client(
    listener: &TcpListener,
    state: &SyncReceiverState,
    auth: &ReceiverAuth,
) -> Result<(), BootstrapError> {
    match listener.accept() {
        Ok((stream, _)) => {
            let _ = handle_sync_client(stream, state, auth);
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
            thread::sleep(std::time::Duration::from_millis(100));
            Ok(())
        }
        Err(_) => Err(BootstrapError::Runtime(
            "sync receiver accept failed".to_string(),
        )),
    }
}

fn handle_sync_client(
    mut stream: TcpStream,
    state: &SyncReceiverState,
    auth: &ReceiverAuth,
) -> Result<(), BootstrapError> {
    if configure_stream_timeouts(&mut stream).is_err() {
        return Ok(());
    }
    let (headers, body) = match read_http_request_parts(&mut stream) {
        Ok(parts) => parts,
        Err(_) => return write_empty_result(&mut stream, "400 Bad Request"),
    };
    match classify_sync_route(&headers) {
        SyncRoute::Current => {}
        SyncRoute::Removed => {
            return write_text_result(
                &mut stream,
                "426 Upgrade Required",
                "NO MORE SUPPORTED PLEASE UPDATE",
            )
        }
        SyncRoute::Unknown => return write_empty_result(&mut stream, "404 Not Found"),
    }
    if auth.required && authenticate_sync_request(&headers, auth.validator.as_ref()).is_err() {
        return write_empty_result(&mut stream, "401 Unauthorized");
    }
    apply_sync_body(&mut stream, state, &body)
}

fn configure_stream_timeouts(stream: &mut TcpStream) -> Result<(), std::io::Error> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
    Ok(())
}

fn apply_sync_body(
    stream: &mut TcpStream,
    state: &SyncReceiverState,
    body: &str,
) -> Result<(), BootstrapError> {
    let envelope = match decode_sync_envelope(body) {
        Ok(envelope) => envelope,
        Err(_) => return write_empty_result(stream, "400 Bad Request"),
    };
    let ack = match state.apply_sync_envelope(&envelope) {
        Ok(ack) => ack,
        Err(_) => return write_empty_result(stream, "400 Bad Request"),
    };
    write_sync_ack_response(stream, &ack);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncRoute {
    Current,
    Removed,
    Unknown,
}

fn classify_sync_route(headers: &str) -> SyncRoute {
    let first_line = headers.lines().next().unwrap_or("");
    let mut path_parts = first_line.split_whitespace();
    let method = path_parts.next().unwrap_or("");
    let path = path_parts.next().unwrap_or("");
    match (method, path) {
        ("POST", "/v1/sync/events") => SyncRoute::Current,
        ("POST", "/sync/events") => SyncRoute::Removed,
        _ => SyncRoute::Unknown,
    }
}

fn authenticate_sync_request(
    headers: &str,
    sync_validator: Option<&(HashTokenProvider, TokenClaims)>,
) -> Result<(), BootstrapError> {
    let token = extract_bearer_token(headers)
        .ok_or_else(|| BootstrapError::Runtime("missing token".to_string()))?;
    let (provider, claims) =
        sync_validator.ok_or_else(|| BootstrapError::Runtime("missing validator".to_string()))?;
    CommandTokenValidator::new(provider, claims.clone())
        .validate_for_purpose(token.as_str(), "sync", None, now_ms())
        .map_err(|_| BootstrapError::Runtime("invalid token".to_string()))
}

fn write_sync_ack_response(stream: &mut TcpStream, ack: &appcore_sync::SyncReceiveAck) {
    let response_body = format!(
        "{{\"accepted\":{},\"received\":{},\"skipped\":{},\"last_sequence\":{}}}",
        ack.accepted, ack.received, ack.skipped, ack.last_sequence
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn read_http_request_parts(stream: &mut TcpStream) -> Result<(String, String), BootstrapError> {
    let mut buffer = Vec::new();
    let mut headers_end = None;
    let mut content_length = 0usize;
    loop {
        read_request_chunk(stream, &mut buffer)?;
        update_content_length(&buffer, &mut headers_end, &mut content_length)?;
        if let Some(parts) = try_split_request(&buffer, headers_end, content_length)? {
            return Ok(parts);
        }
    }
}

fn read_request_chunk(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> Result<(), BootstrapError> {
    let mut chunk = [0u8; 512];
    let read = stream
        .read(&mut chunk)
        .map_err(|_| BootstrapError::Runtime("failed to read sync request".to_string()))?;
    if read == 0 {
        return Err(BootstrapError::Runtime(
            "incomplete sync request".to_string(),
        ));
    }
    buffer.extend_from_slice(&chunk[..read]);
    Ok(())
}

fn update_content_length(
    buffer: &[u8],
    headers_end: &mut Option<usize>,
    content_length: &mut usize,
) -> Result<(), BootstrapError> {
    if headers_end.is_some() {
        return Ok(());
    }
    *headers_end = find_headers_end(buffer);
    if let Some(end) = *headers_end {
        *content_length = parse_content_length(&buffer[..end])?;
        reject_large_body(*content_length)?;
    }
    Ok(())
}

fn try_split_request(
    buffer: &[u8],
    headers_end: Option<usize>,
    content_length: usize,
) -> Result<Option<(String, String)>, BootstrapError> {
    let Some(end) = headers_end else {
        return Ok(None);
    };
    if buffer.len().saturating_sub(end) < content_length {
        return Ok(None);
    }
    Ok(Some(split_request(buffer, end, content_length)?))
}

fn split_request(
    buffer: &[u8],
    headers_end: usize,
    content_length: usize,
) -> Result<(String, String), BootstrapError> {
    let body = &buffer[headers_end..headers_end + content_length];
    let headers = String::from_utf8(buffer[..headers_end].to_vec())
        .map_err(|_| BootstrapError::Runtime("invalid sync request headers".to_string()))?;
    let body = String::from_utf8(body.to_vec())
        .map_err(|_| BootstrapError::Runtime("invalid sync request body".to_string()))?;
    Ok((headers, body))
}

fn reject_large_body(content_length: usize) -> Result<(), BootstrapError> {
    if content_length <= MAX_SYNC_REQUEST_BODY_BYTES {
        return Ok(());
    }
    Err(BootstrapError::Runtime(
        "sync request body too large".to_string(),
    ))
}

fn write_empty_result(stream: &mut TcpStream, status: &str) -> Result<(), BootstrapError> {
    write_empty_http_response(stream, status);
    Ok(())
}

fn write_text_result(
    stream: &mut TcpStream,
    status: &str,
    body: &str,
) -> Result<(), BootstrapError> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    Ok(())
}

fn write_empty_http_response(stream: &mut TcpStream, status: &str) {
    let response = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n");
    let _ = stream.write_all(response.as_bytes());
}

fn extract_bearer_token(headers: &str) -> Option<String> {
    headers.lines().find_map(|line| {
        line.strip_prefix("Authorization: Bearer ")
            .map(|value| value.trim().to_string())
    })
}

fn find_headers_end(buffer: &[u8]) -> Option<usize> {
    let marker = b"\r\n\r\n";
    buffer
        .windows(marker.len())
        .position(|window| window == marker)
        .map(|position| position + marker.len())
}

fn parse_content_length(headers: &[u8]) -> Result<usize, BootstrapError> {
    let as_text = String::from_utf8(headers.to_vec())
        .map_err(|_| BootstrapError::Runtime("invalid sync headers".to_string()))?;
    as_text
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length:"))
        .ok_or_else(|| BootstrapError::Runtime("missing sync content-length".to_string()))?
        .trim()
        .parse::<usize>()
        .map_err(|_| BootstrapError::Runtime("invalid sync content-length".to_string()))
}

#[cfg(test)]
#[path = "sync_cli_tests.rs"]
mod tests;
