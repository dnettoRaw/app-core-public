// =============================================================================
//        #######
//     ###       ###     F: client.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{
    parse_response, CancellationToken, HttpClientConfig, HttpRequest, HttpResponse, HttpScheme,
    HttpTarget, TransportError, TransportResult,
};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Sends one bounded blocking HTTP request.
pub fn send(
    target: &HttpTarget,
    request: &HttpRequest,
    config: HttpClientConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<HttpResponse> {
    check_cancelled(cancellation)?;
    if request.body().len() > config.max_request_bytes {
        return Err(TransportError::RequestTooLarge {
            max: config.max_request_bytes,
        });
    }
    let stream = connect(target, config.timeout_ms)?;
    check_cancelled(cancellation)?;
    match target.scheme() {
        HttpScheme::Http => exchange(stream, target, request, config, cancellation),
        HttpScheme::Https => {
            let connector = native_tls::TlsConnector::new()
                .map_err(|error| TransportError::Tls(error.to_string()))?;
            let stream = connector
                .connect(target.host(), stream)
                .map_err(|error| TransportError::Tls(error.to_string()))?;
            exchange(stream, target, request, config, cancellation)
        }
    }
}

fn connect(target: &HttpTarget, timeout_ms: u64) -> TransportResult<TcpStream> {
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let mut addresses = (target.host(), target.port())
        .to_socket_addrs()
        .map_err(|error| TransportError::Dns(error.to_string()))?;
    let address = addresses
        .next()
        .ok_or_else(|| TransportError::Dns("host resolved to no addresses".to_string()))?;
    let stream = TcpStream::connect_timeout(&address, timeout).map_err(map_io_error)?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|_| stream.set_write_timeout(Some(timeout)))
        .map_err(map_io_error)?;
    Ok(stream)
}

fn exchange<S>(
    mut stream: S,
    target: &HttpTarget,
    request: &HttpRequest,
    config: HttpClientConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<HttpResponse>
where
    S: Read + Write,
{
    let head = encode_request_head(target, request)?;
    stream.write_all(&head).map_err(map_io_error)?;
    stream.write_all(request.body()).map_err(map_io_error)?;
    check_cancelled(cancellation)?;
    let raw = read_bounded(&mut stream, config, cancellation)?;
    parse_response(&raw, config.max_header_bytes, config.max_response_bytes)
}

fn encode_request_head(target: &HttpTarget, request: &HttpRequest) -> TransportResult<Vec<u8>> {
    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\n",
        request.method(),
        target.path(),
        target.authority()
    );
    for header in request.headers() {
        if matches!(
            header.name().to_ascii_lowercase().as_str(),
            "host" | "content-length" | "connection"
        ) {
            return Err(TransportError::InvalidRequest(format!(
                "reserved request header: {}",
                header.name()
            )));
        }
        head.push_str(header.name());
        head.push_str(": ");
        head.push_str(header.value());
        head.push_str("\r\n");
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        request.body().len()
    ));
    Ok(head.into_bytes())
}

fn read_bounded<S>(
    stream: &mut S,
    config: HttpClientConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<Vec<u8>>
where
    S: Read,
{
    let max_raw = config
        .max_header_bytes
        .saturating_add(config.max_response_bytes)
        .saturating_add(1);
    let mut raw = Vec::new();
    loop {
        check_cancelled(cancellation)?;
        let mut chunk = [0u8; 8_192];
        let read = stream.read(&mut chunk).map_err(map_io_error)?;
        if read == 0 {
            break;
        }
        if raw.len().saturating_add(read) > max_raw {
            return Err(TransportError::ResponseTooLarge {
                max: config.max_response_bytes,
            });
        }
        raw.extend_from_slice(&chunk[..read]);
    }
    if raw.is_empty() {
        return Err(TransportError::InvalidResponse(
            "empty response".to_string(),
        ));
    }
    Ok(raw)
}

fn check_cancelled(cancellation: Option<&CancellationToken>) -> TransportResult<()> {
    if cancellation.is_some_and(CancellationToken::is_cancelled) {
        Err(TransportError::Cancelled)
    } else {
        Ok(())
    }
}

fn map_io_error(error: std::io::Error) -> TransportError {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => TransportError::Timeout,
        std::io::ErrorKind::ConnectionRefused => TransportError::ConnectionRefused,
        _ => TransportError::Io(error.to_string()),
    }
}
