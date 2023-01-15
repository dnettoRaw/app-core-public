// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

fn serve(response: Vec<u8>, delay: Duration) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let thread = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request);
        thread::sleep(delay);
        let _ = stream.write_all(&response);
    });
    (format!("http://{address}"), thread)
}

fn request() -> HttpRequest {
    HttpRequest::new("GET", Vec::new()).unwrap()
}

#[test]
fn debug_redacts_headers_and_omits_request_and_response_bodies() {
    let marker = "secret-marker-must-not-appear";
    let request = HttpRequest::new("POST", marker.as_bytes().to_vec())
        .unwrap()
        .with_header(HttpHeader::new("Authorization", marker).unwrap());
    let response = HttpResponse {
        status_code: 200,
        headers: vec![("set-cookie".to_string(), marker.to_string())],
        body: marker.as_bytes().to_vec(),
    };

    let request_debug = format!("{request:?}");
    let response_debug = format!("{response:?}");
    assert!(!request_debug.contains(marker));
    assert!(!response_debug.contains(marker));
    assert!(request_debug.contains("body_bytes"));
    assert!(response_debug.contains("body_bytes"));
}

#[test]
fn parses_chunked_and_gzip_with_bounds() {
    let compressed = encode_gzip_if_smaller(&vec![b'a'; 1_024]).unwrap().unwrap();
    let raw = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Encoding: gzip\r\n\r\n{:x}\r\n",
        compressed.len()
    )
    .into_bytes();
    let mut raw = [raw, compressed, b"\r\n0\r\n\r\n".to_vec()].concat();
    let response = parse_response(&raw, 1_024, 2_048).unwrap();
    assert_eq!(response.body, vec![b'a'; 1_024]);
    raw.clear();
}

#[test]
fn rejects_truncated_oversized_and_malformed_gzip_responses() {
    let truncated = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nshort";
    assert_eq!(
        parse_response(truncated, 1_024, 1_024),
        Err(TransportError::TruncatedResponse)
    );
    let oversized = b"HTTP/1.1 200 OK\r\n\r\n0123456789";
    assert!(matches!(
        parse_response(oversized, 1_024, 4),
        Err(TransportError::ResponseTooLarge { .. })
    ));
    let gzip = b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\n\r\nnot-gzip";
    assert!(matches!(
        parse_response(gzip, 1_024, 1_024),
        Err(TransportError::InvalidResponse(_))
    ));
}

#[test]
fn classifies_timeout_connection_refusal_and_cancellation() {
    let (url, server) = serve(
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
        Duration::from_millis(100),
    );
    let target = HttpTarget::parse(&url, "/").unwrap();
    let config = HttpClientConfig {
        timeout_ms: 10,
        ..HttpClientConfig::default()
    };
    assert_eq!(
        send(&target, &request(), config, None),
        Err(TransportError::Timeout)
    );
    server.join().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let target = HttpTarget::parse(&format!("http://{address}"), "/").unwrap();
    assert_eq!(
        send(&target, &request(), config, None),
        Err(TransportError::ConnectionRefused)
    );

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert_eq!(
        send(&target, &request(), config, Some(&cancellation)),
        Err(TransportError::Cancelled)
    );
}

#[test]
fn redacts_sensitive_headers_and_rejects_dns_failure() {
    let header = HttpHeader::sensitive("Authorization", "Bearer do-not-log").unwrap();
    assert!(!format!("{header:?}").contains("do-not-log"));
    let target = HttpTarget::parse("http://invalid.invalid:80", "/").unwrap();
    assert!(matches!(
        send(
            &target,
            &request(),
            HttpClientConfig {
                timeout_ms: 10,
                ..HttpClientConfig::default()
            },
            None
        ),
        Err(TransportError::Dns(_))
    ));
}
