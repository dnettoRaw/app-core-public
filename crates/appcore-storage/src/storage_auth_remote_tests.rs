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
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).expect("read");
    let text = String::from_utf8_lossy(&buf[..n]);
    text.split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default()
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
fn wrong_transport_secret_is_rejected() {
    let address = spawn_auth_server(1);
    let wrong = provider(b"wrong-transport-secret-123");
    let client = RemoteAuthStorageClient::new(address, wrong);

    let result = client.seal_resource("private.bin", b"payload");

    assert!(matches!(result, Err(StorageError::SecurityFailed(_))));
}
