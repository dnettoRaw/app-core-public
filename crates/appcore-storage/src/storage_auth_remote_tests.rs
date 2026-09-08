// =============================================================================
//        #######
//     ###       ###     F: storage_auth_remote_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/07 12:31:50 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/21 19:22:40 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use crate::storage::FileStorageProvider;
use appcore_security::HashTokenProvider;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn provider(secret: &[u8]) -> HashTokenProvider {
    HashTokenProvider::from_secret(secret.to_vec()).expect("provider")
}

fn auth_pair() -> (HashTokenProvider, HashTokenProvider) {
    (
        provider(b"transport-secret-1234567890"),
        provider(b"data-secret-123456789012345"),
    )
}

fn temp_paths(prefix: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!("appcore-remote-{prefix}-{}", std::process::id()));
    (base.join("storage"), base.join("backups"))
}

fn spawn_auth_server(max_requests: usize) -> String {
    let (transport, data) = auth_pair();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr").to_string();
    thread::spawn(move || {
        for stream in listener.incoming().take(max_requests).flatten() {
            handle_test_stream(stream, &transport, &data);
        }
    });
    address
}

fn handle_test_stream(
    mut stream: TcpStream,
    transport: &HashTokenProvider,
    data: &HashTokenProvider,
) {
    let body = read_request_body(&mut stream);
    let response = open_remote_request(&body, transport, now_ms())
        .and_then(|request| process_remote_request(&request, data))
        .and_then(|response| seal_remote_response(&response, transport));
    write_test_response(&mut stream, response);
}

fn read_request_body(stream: &mut TcpStream) -> String {
    let mut request = Vec::with_capacity(4096);
    let mut buf = [0u8; 4096];
    let header_end = loop {
        if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break position;
        }
        let read = stream.read(&mut buf).expect("read headers");
        assert!(read > 0, "request headers ended early");
        request.extend_from_slice(&buf[..read]);
        assert!(request.len() <= DEFAULT_AUTH_REMOTE_MAX_BYTES);
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let body_len = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let body_start = header_end + 4;
    let expected = body_start + body_len;
    assert!(expected <= DEFAULT_AUTH_REMOTE_MAX_HTTP_RESPONSE_BYTES);
    while request.len() < expected {
        let read = stream.read(&mut buf).expect("read body");
        assert!(read > 0, "request body ended early");
        request.extend_from_slice(&buf[..read]);
    }
    String::from_utf8(request[body_start..expected].to_vec()).expect("request UTF-8")
}

fn write_test_response(stream: &mut TcpStream, response: StorageResult<String>) {
    let (status, body) = match response {
        Ok(body) => ("200 OK", body),
        Err(_) => ("401 Unauthorized", String::new()),
    };
    let raw = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(raw.as_bytes());
}

#[test]
fn auth_remote_request_rejects_path_traversal_resource() {
    let result = make_auth_request("../private.txt", "seal", b"payload", now_ms());

    assert!(matches!(result, Err(StorageError::InvalidPath(_))));
}

#[test]
fn remote_auth_client_offline_returns_auth_unavailable() {
    let client = RemoteAuthStorageClient::new("127.0.0.1:9", provider(b"transport-secret-1234"))
        .with_timeout_ms(50);

    let result = client.seal_resource("private.bin", b"payload");

    assert!(matches!(result, Err(StorageError::AuthUnavailable(_))));
}

#[test]
fn secret_file_is_bounded_before_materialization() {
    let (storage, _) = temp_paths("secret-file-limit");
    std::fs::create_dir_all(&storage).unwrap();
    let path = storage.join("transport.secret");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(DEFAULT_AUTH_REMOTE_MAX_BYTES as u64 + 1)
        .unwrap();

    assert!(matches!(
        RemoteAuthStorageClient::from_secret_file("127.0.0.1:9", &path),
        Err(StorageError::SecurityFailed(_))
    ));
    let _ = std::fs::remove_dir_all(storage.parent().unwrap());
}

#[test]
fn remote_auth_roundtrip_keeps_plaintext_off_disk() {
    let address = spawn_auth_server(2);
    let client = RemoteAuthStorageClient::new(address, auth_pair().0);
    let (storage, backups) = temp_paths("remote-auth-roundtrip");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());

    let write = provider.write_remote_auth_required_bytes(
        "secure/runtime-record.bin",
        b"classified payload",
        Some(&client),
    );
    let raw = std::fs::read(storage.join("secure/runtime-record.bin")).expect("raw file");
    let read = provider.read_remote_auth_required_bytes("secure/runtime-record.bin", Some(&client));

    assert!(write.is_ok());
    assert_ne!(raw, b"classified payload".to_vec());
    assert!(!String::from_utf8_lossy(&raw).contains("classified payload"));
    assert_eq!(read.ok(), Some(b"classified payload".to_vec()));
    let _ = std::fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn default_max_plaintext_roundtrip_fits_wire_limit() {
    let (transport, data) = auth_pair();
    let plaintext = vec![0x5a; DEFAULT_AUTH_REMOTE_MAX_PLAINTEXT_BYTES];
    let timestamp = now_ms();
    let seal_request = make_auth_request("private.bin", "seal", &plaintext, timestamp).unwrap();
    let seal_token = seal_remote_request(&seal_request, &transport).unwrap();
    let opened_seal = open_remote_request(&seal_token, &transport, timestamp).unwrap();
    let seal_response = process_remote_request(&opened_seal, &data).unwrap();
    let seal_response_token = seal_remote_response(&seal_response, &transport).unwrap();
    let sealed = open_remote_response(
        &seal_response_token,
        &transport,
        &seal_request.nonce,
        timestamp,
    )
    .unwrap();
    let open_request = make_auth_request("private.bin", "open", &sealed, timestamp).unwrap();
    let open_token = seal_remote_request(&open_request, &transport).unwrap();
    let opened_open = open_remote_request(&open_token, &transport, timestamp).unwrap();
    let open_response = process_remote_request(&opened_open, &data).unwrap();
    let open_response_token = seal_remote_response(&open_response, &transport).unwrap();
    let recovered = open_remote_response(
        &open_response_token,
        &transport,
        &open_request.nonce,
        timestamp,
    )
    .unwrap();

    assert!(sealed.len() <= DEFAULT_AUTH_REMOTE_MAX_SEALED_BYTES);
    assert!(seal_token.len() <= DEFAULT_AUTH_REMOTE_MAX_BYTES);
    assert!(seal_response_token.len() <= DEFAULT_AUTH_REMOTE_MAX_BYTES);
    assert!(open_token.len() <= DEFAULT_AUTH_REMOTE_MAX_BYTES);
    assert!(open_response_token.len() <= DEFAULT_AUTH_REMOTE_MAX_BYTES);
    assert_eq!(recovered, plaintext);
}

#[test]
fn operation_payload_limits_fail_before_hex_encoding() {
    let oversized_plaintext = vec![0_u8; DEFAULT_AUTH_REMOTE_MAX_PLAINTEXT_BYTES + 1];
    assert!(matches!(
        make_auth_request("private.bin", "seal", &oversized_plaintext, now_ms()),
        Err(StorageError::SecurityFailed(_))
    ));

    let oversized_sealed = vec![0_u8; DEFAULT_AUTH_REMOTE_MAX_SEALED_BYTES + 1];
    assert!(matches!(
        make_auth_request("private.bin", "open", &oversized_sealed, now_ms()),
        Err(StorageError::SecurityFailed(_))
    ));
}

#[test]
fn response_and_http_limits_fail_closed() {
    let response = AuthRemoteResponse {
        schema: AUTH_REMOTE_SCHEMA.to_string(),
        status: "ok".to_string(),
        nonce: "nonce".to_string(),
        expires_at_ms: now_ms().saturating_add(DEFAULT_AUTH_REMOTE_TTL_MS),
        payload_hex: "aa".repeat(DEFAULT_AUTH_REMOTE_MAX_SEALED_BYTES + 1),
    };
    assert!(matches!(
        seal_remote_response(&response, &auth_pair().0),
        Err(StorageError::SecurityFailed(_))
    ));

    let mut oversized = std::io::Cursor::new(vec![0_u8; 17]);
    assert!(matches!(
        read_limited(&mut oversized, 16),
        Err(StorageError::SecurityFailed(_))
    ));

    let malformed = "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\ntoken".to_string();
    assert!(matches!(
        parse_http_response(malformed),
        Err(StorageError::SecurityFailed(_))
    ));
}

#[test]
fn wrong_transport_secret_is_rejected() {
    let address = spawn_auth_server(1);
    let wrong = provider(b"wrong-transport-secret-123");
    let client = RemoteAuthStorageClient::new(address, wrong);

    let result = client.seal_resource("private.bin", b"payload");

    assert!(matches!(result, Err(StorageError::SecurityFailed(_))));
}
