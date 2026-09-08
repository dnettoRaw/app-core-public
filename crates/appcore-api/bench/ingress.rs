// =============================================================================
//        #######
//     ###       ###     F: ingress.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Measures bounded HTTP body admission through real loopback sockets.

use appcore_api::{HttpApiConfig, RuntimeHttpHost};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

const INFLIGHT_REQUESTS: usize = 16;
const BODY_BYTES: usize = 512 * 1_024;
const IO_DEADLINE: Duration = Duration::from_secs(5);

pub(super) fn benchmark(case: &str) -> Result<(), String> {
    let iterations = super::iterations(1);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let started = super::benchmark_started();
    for _ in 0..iterations {
        runtime.block_on(run_once())?;
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-api::{case} iterations={iterations} bodies={INFLIGHT_REQUESTS} body_bytes={BODY_BYTES} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

async fn run_once() -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| error.to_string())?;
    let address = listener.local_addr().map_err(|error| error.to_string())?;
    let server = tokio::spawn(async move { axum::serve(listener, host().router()).await });
    let partial_body = vec![b'x'; BODY_BYTES - 1];
    let mut held = Vec::with_capacity(INFLIGHT_REQUESTS);
    for _ in 0..INFLIGHT_REQUESTS {
        let client = TcpStream::connect(address)
            .await
            .map_err(|error| error.to_string())?;
        write_all(&client, request_head(BODY_BYTES).as_bytes()).await?;
        write_all(&client, &partial_body).await?;
        held.push(client);
    }

    // Let the server drain the sockets and retain all sixteen admitted bodies.
    tokio::time::sleep(Duration::from_millis(25)).await;
    require_status(address, complete_request(), 503).await?;
    require_status(address, health_request(), 200).await?;

    drop(held);
    let released = wait_until_released(address).await;
    server.abort();
    let _ = server.await;
    released
}

fn host() -> RuntimeHttpHost {
    let config = HttpApiConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        max_payload_bytes: BODY_BYTES,
        ..HttpApiConfig::default()
    };
    RuntimeHttpHost::new(config, super::static_info_fixture())
}

async fn wait_until_released(address: SocketAddr) -> Result<(), String> {
    let deadline = Instant::now() + IO_DEADLINE;
    loop {
        let status = request_status(address, complete_request()).await?;
        if status != 503 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("HTTP ingress slots were not released after cancellation".to_string());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn require_status(
    address: SocketAddr,
    request: &'static [u8],
    expected: u16,
) -> Result<(), String> {
    let status = request_status(address, request).await?;
    if status == expected {
        Ok(())
    } else {
        Err(format!("expected HTTP {expected}, received HTTP {status}"))
    }
}

async fn request_status(address: SocketAddr, request: &[u8]) -> Result<u16, String> {
    let client = TcpStream::connect(address)
        .await
        .map_err(|error| error.to_string())?;
    write_all(&client, request).await?;
    let mut response = [0_u8; 512];
    let read = tokio::time::timeout(IO_DEADLINE, read_some(&client, &mut response))
        .await
        .map_err(|_| "HTTP benchmark response timed out".to_string())??;
    parse_status(&response[..read])
}

async fn write_all(stream: &TcpStream, mut bytes: &[u8]) -> Result<(), String> {
    tokio::time::timeout(IO_DEADLINE, async {
        while !bytes.is_empty() {
            stream.writable().await.map_err(|error| error.to_string())?;
            match stream.try_write(bytes) {
                Ok(0) => return Err("HTTP benchmark socket closed while writing".to_string()),
                Ok(written) => bytes = &bytes[written..],
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| "HTTP benchmark write timed out".to_string())?
}

async fn read_some(stream: &TcpStream, bytes: &mut [u8]) -> Result<usize, String> {
    loop {
        stream.readable().await.map_err(|error| error.to_string())?;
        match stream.try_read(bytes) {
            Ok(0) => return Err("HTTP benchmark socket closed without a response".to_string()),
            Ok(read) => return Ok(read),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn parse_status(response: &[u8]) -> Result<u16, String> {
    let response = std::str::from_utf8(response).map_err(|error| error.to_string())?;
    response
        .split_ascii_whitespace()
        .nth(1)
        .ok_or_else(|| "HTTP benchmark response omitted status".to_string())?
        .parse()
        .map_err(|error| format!("invalid HTTP benchmark status: {error}"))
}

fn request_head(content_length: usize) -> String {
    format!(
        "POST /v1/command HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {content_length}\r\n\r\n"
    )
}

fn complete_request() -> &'static [u8] {
    b"POST /v1/command HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
}

fn health_request() -> &'static [u8] {
    b"GET /v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
}
