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

fn read_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let header_end = loop {
        if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        let mut chunk = [0u8; 1024];
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0);
        request.extend_from_slice(&chunk[..read]);
    };
    let head = std::str::from_utf8(&request[..header_end]).unwrap();
    let body_bytes = head
        .split("\r\n")
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(str::trim)
                .map(str::parse::<usize>)
        })
        .transpose()
        .unwrap()
        .unwrap_or(0);
    while request.len() < header_end + body_bytes {
        let mut chunk = [0u8; 1024];
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0);
        request.extend_from_slice(&chunk[..read]);
    }
    request
}

fn exchange_config() -> HttpExchangeConfig {
    HttpExchangeConfig {
        timeouts: HttpTimeouts::uniform(1_000),
        ..HttpExchangeConfig::default()
    }
}

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

#[test]
fn reusable_client_uses_one_connection_for_multiple_framed_responses() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let responses: [&[u8]; 2] = [
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\na\r\n0\r\nX-Test: complete\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: keep-alive\r\n\r\nb",
        ];
        for response in responses {
            let request = read_request(&mut stream);
            assert!(String::from_utf8_lossy(&request).contains("Connection: keep-alive"));
            stream.write_all(response).unwrap();
        }
    });
    let target = HttpTarget::parse(&format!("http://{address}"), "/").unwrap();
    let client = HttpClient::new(HttpPoolConfig::default()).unwrap();
    assert_eq!(
        client
            .send(&target, &request(), exchange_config(), None)
            .unwrap()
            .body,
        b"a"
    );
    assert_eq!(
        client
            .send(&target, &request(), exchange_config(), None)
            .unwrap()
            .body,
        b"b"
    );
    server.join().unwrap();
}

#[test]
fn compatibility_send_preserves_one_shot_connection_close() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert!(String::from_utf8_lossy(&request).contains("Connection: close"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    let target = HttpTarget::parse(&format!("http://{address}"), "/").unwrap();
    assert!(send(
        &target,
        &request(),
        HttpClientConfig {
            max_request_bytes: 0,
            ..HttpClientConfig::default()
        },
        None,
    )
    .is_ok());
    server.join().unwrap();
}

#[test]
fn reusable_client_applies_independent_read_timeout() {
    let (url, server) = serve(
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
        Duration::from_millis(100),
    );
    let target = HttpTarget::parse(&url, "/").unwrap();
    let client = HttpClient::new(HttpPoolConfig::default()).unwrap();
    let config = HttpExchangeConfig {
        timeouts: HttpTimeouts {
            connect_ms: 500,
            read_ms: 10,
            write_ms: 500,
        },
        ..HttpExchangeConfig::default()
    };
    assert_eq!(
        client.send(&target, &request(), config, None),
        Err(TransportError::Timeout)
    );
    server.join().unwrap();
}

#[test]
fn connection_close_and_truncation_never_return_a_socket_to_the_pool() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut truncated, _) = listener.accept().unwrap();
        let _ = read_request(&mut truncated);
        truncated
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nno")
            .unwrap();
        drop(truncated);

        let (mut closed, _) = listener.accept().unwrap();
        let _ = read_request(&mut closed);
        closed
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
        drop(closed);

        let (mut healthy, _) = listener.accept().unwrap();
        let _ = read_request(&mut healthy);
        healthy
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .unwrap();
    });
    let target = HttpTarget::parse(&format!("http://{address}"), "/").unwrap();
    let client = HttpClient::new(HttpPoolConfig::default()).unwrap();
    assert_eq!(
        client.send(&target, &request(), exchange_config(), None),
        Err(TransportError::TruncatedResponse)
    );
    assert_eq!(
        client
            .send(&target, &request(), exchange_config(), None)
            .unwrap()
            .body,
        b"ok"
    );
    assert_eq!(
        client
            .send(&target, &request(), exchange_config(), None)
            .unwrap()
            .body,
        b"ok"
    );
    server.join().unwrap();
}

#[test]
fn per_origin_admission_is_bounded_and_cancellable() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (accepted_tx, accepted_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_request(&mut stream);
        accepted_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    let target = HttpTarget::parse(&format!("http://{address}"), "/").unwrap();
    let client = HttpClient::new(HttpPoolConfig {
        max_connections_per_origin: 1,
        max_idle_per_origin: 1,
        max_origins: 1,
        ..HttpPoolConfig::default()
    })
    .unwrap();
    let first_client = client.clone();
    let first_target = target.clone();
    let first = thread::spawn(move || {
        first_client.send(&first_target, &request(), exchange_config(), None)
    });
    accepted_rx.recv().unwrap();
    let saturated = HttpExchangeConfig {
        timeouts: HttpTimeouts {
            connect_ms: 20,
            read_ms: 1_000,
            write_ms: 1_000,
        },
        ..HttpExchangeConfig::default()
    };
    assert_eq!(
        client.send(&target, &request(), saturated, None),
        Err(TransportError::Timeout)
    );
    let other_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let other_target = HttpTarget::parse(
        &format!("http://{}", other_listener.local_addr().unwrap()),
        "/",
    )
    .unwrap();
    assert_eq!(
        client.send(&other_target, &request(), saturated, None),
        Err(TransportError::Timeout)
    );
    drop(other_listener);
    let cancellation = CancellationToken::new();
    let cancel_from_thread = cancellation.clone();
    let canceller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(10));
        cancel_from_thread.cancel();
    });
    assert_eq!(
        client.send(&target, &request(), exchange_config(), Some(&cancellation)),
        Err(TransportError::Cancelled)
    );
    canceller.join().unwrap();
    release_tx.send(()).unwrap();
    assert!(first.join().unwrap().is_ok());
    server.join().unwrap();
}

#[test]
fn idle_connections_expire_before_the_next_exchange() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let _ = read_request(&mut first);
        first
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        let (mut second, _) = listener.accept().unwrap();
        assert_eq!(first.read(&mut [0u8; 1]).unwrap(), 0);
        let _ = read_request(&mut second);
        second
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    let target = HttpTarget::parse(&format!("http://{address}"), "/").unwrap();
    let client = HttpClient::new(HttpPoolConfig {
        idle_timeout_ms: 10,
        ..HttpPoolConfig::default()
    })
    .unwrap();
    client
        .send(&target, &request(), exchange_config(), None)
        .unwrap();
    thread::sleep(Duration::from_millis(20));
    client
        .send(&target, &request(), exchange_config(), None)
        .unwrap();
    server.join().unwrap();
}

#[test]
fn dropping_the_client_closes_retained_idle_connections() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        assert_eq!(stream.read(&mut [0u8; 1]).unwrap(), 0);
    });
    let target = HttpTarget::parse(&format!("http://{address}"), "/").unwrap();
    let client = HttpClient::default();
    client
        .send(&target, &request(), exchange_config(), None)
        .unwrap();
    drop(client);
    server.join().unwrap();
}
