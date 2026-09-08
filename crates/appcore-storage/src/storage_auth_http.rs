// =============================================================================
//        #######
//     ###       ###     F: storage_auth_http.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Bounded HTTP encoding and response parsing for remote auth-storage.

use super::storage_auth_remote::{AUTH_REMOTE_ENDPOINT, DEFAULT_AUTH_REMOTE_MAX_BYTES};
use super::{StorageError, StorageResult};
use std::io::Read;
use std::net::TcpStream;

const AUTH_REMOTE_MAX_HEADER_BYTES: usize = 64 * 1024;

pub(super) fn http_request_head(host: &str, body_len: usize) -> String {
    format!(
        "POST {AUTH_REMOTE_ENDPOINT} HTTP/1.1\r\nHost: {host}\r\nContent-Type: text/plain\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n"
    )
}

pub(super) fn read_http_response(
    stream: &mut TcpStream,
    max_bytes: usize,
) -> StorageResult<String> {
    let raw = read_limited(stream, max_bytes)?;
    let text = String::from_utf8(raw)
        .map_err(|_| StorageError::SecurityFailed("auth response".to_string()))?;
    parse_http_response(text)
}

pub(super) fn read_limited(stream: &mut impl Read, max_bytes: usize) -> StorageResult<Vec<u8>> {
    let mut out = Vec::with_capacity(4096.min(max_bytes));
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if n > max_bytes.saturating_sub(out.len()) {
                    return Err(StorageError::SecurityFailed(
                        "auth response too large".into(),
                    ));
                }
                out.extend_from_slice(&buf[..n]);
            }
            Err(_) => return Err(StorageError::AuthUnavailable("auth-server".to_string())),
        }
    }
    Ok(out)
}

pub(super) fn parse_http_response(mut text: String) -> StorageResult<String> {
    let header_end = text
        .find("\r\n\r\n")
        .ok_or_else(|| StorageError::SecurityFailed("malformed auth response".to_string()))?;
    if header_end > AUTH_REMOTE_MAX_HEADER_BYTES {
        return Err(StorageError::SecurityFailed(
            "auth response headers too large".to_string(),
        ));
    }
    let body_start = header_end + 4;
    let head = &text[..header_end];
    let status = http_status(head)?;
    let body_len = text.len() - body_start;
    if body_len > DEFAULT_AUTH_REMOTE_MAX_BYTES
        || declared_content_length(head)?.is_some_and(|length| length != body_len)
    {
        return Err(StorageError::SecurityFailed(
            "invalid auth response length".to_string(),
        ));
    }
    if (200..300).contains(&status) {
        text.drain(..body_start);
        return Ok(text);
    }
    if status == 503 {
        return Err(StorageError::AuthUnavailable("auth-server".to_string()));
    }
    Err(StorageError::SecurityFailed(format!(
        "auth status {status}"
    )))
}

fn declared_content_length(headers: &str) -> StorageResult<Option<usize>> {
    let mut declared = None;
    for line in headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if !name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        let value = value
            .trim()
            .parse::<usize>()
            .map_err(|_| StorageError::SecurityFailed("invalid content-length".to_string()))?;
        if declared.replace(value).is_some() {
            return Err(StorageError::SecurityFailed(
                "multiple content-length headers".to_string(),
            ));
        }
    }
    Ok(declared)
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
