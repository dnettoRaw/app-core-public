// =============================================================================
//        #######
//     ###       ###     F: wire.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0
// =============================================================================

//! HTTP/1.1 request encoding and complete response-frame reads.

use crate::connection::map_io_error;
use crate::{
    CancellationToken, HttpExchangeConfig, HttpRequest, HttpTarget, TransportError, TransportResult,
};
use std::io::{Read, Write};

pub(crate) struct RawResponse {
    pub(crate) bytes: Vec<u8>,
    pub(crate) reusable: bool,
}

pub(crate) fn exchange(
    stream: &mut (impl Read + Write),
    target: &HttpTarget,
    request: &HttpRequest,
    config: HttpExchangeConfig,
    cancellation: Option<&CancellationToken>,
    keep_alive: bool,
) -> TransportResult<RawResponse> {
    check_cancelled(cancellation)?;
    let head = encode_request_head(target, request, keep_alive)?;
    stream.write_all(&head).map_err(map_io_error)?;
    stream.write_all(request.body()).map_err(map_io_error)?;
    stream.flush().map_err(map_io_error)?;
    check_cancelled(cancellation)?;
    read_response(stream, request.method(), config, cancellation)
}

fn encode_request_head(
    target: &HttpTarget,
    request: &HttpRequest,
    keep_alive: bool,
) -> TransportResult<Vec<u8>> {
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
    let connection = if keep_alive { "keep-alive" } else { "close" };
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: {connection}\r\n\r\n",
        request.body().len()
    ));
    Ok(head.into_bytes())
}

fn read_response(
    stream: &mut impl Read,
    method: &str,
    config: HttpExchangeConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<RawResponse> {
    let mut raw = Vec::new();
    let header_end = read_headers(stream, &mut raw, config, cancellation)?;
    let head = std::str::from_utf8(&raw[..header_end])
        .map_err(|_| TransportError::InvalidResponse("headers are not UTF-8".to_string()))?;
    let framing = parse_framing(head, method)?;
    let (frame_end, explicitly_framed) = match framing.body {
        BodyFraming::None => (header_end, true),
        BodyFraming::Length(length) => {
            let end = header_end
                .checked_add(length)
                .ok_or_else(|| TransportError::InvalidResponse("body overflow".to_string()))?;
            read_until(stream, &mut raw, end, config, cancellation)?;
            (end, true)
        }
        BodyFraming::Chunked => {
            let end = read_chunked_end(stream, &mut raw, header_end, config, cancellation)?;
            (end, true)
        }
        BodyFraming::Close => {
            read_to_close(stream, &mut raw, config, cancellation)?;
            (raw.len(), false)
        }
    };
    if raw.len() > frame_end {
        return Err(TransportError::InvalidResponse(
            "response exceeds its HTTP frame".to_string(),
        ));
    }
    raw.truncate(frame_end);
    Ok(RawResponse {
        bytes: raw,
        reusable: framing.reusable && explicitly_framed,
    })
}

fn read_headers(
    stream: &mut impl Read,
    raw: &mut Vec<u8>,
    config: HttpExchangeConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<usize> {
    loop {
        if let Some(position) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            let end = position + 4;
            if end.saturating_sub(4) > config.max_header_bytes {
                return Err(TransportError::InvalidResponse(
                    "headers exceed configured limit".to_string(),
                ));
            }
            return Ok(end);
        }
        if raw.len() > config.max_header_bytes.saturating_add(3) {
            return Err(TransportError::InvalidResponse(
                "headers exceed configured limit".to_string(),
            ));
        }
        read_more(stream, raw, config, cancellation)?;
    }
}

struct ResponseFraming {
    body: BodyFraming,
    reusable: bool,
}

enum BodyFraming {
    None,
    Length(usize),
    Chunked,
    Close,
}

fn parse_framing(head: &str, method: &str) -> TransportResult<ResponseFraming> {
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .ok_or_else(|| TransportError::InvalidResponse("missing status line".to_string()))?;
    let mut status_parts = status.split_whitespace();
    let version = status_parts.next().unwrap_or_default();
    let code = status_parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| TransportError::InvalidResponse("invalid status code".to_string()))?;
    if (100..200).contains(&code) {
        return Err(TransportError::InvalidResponse(
            "informational responses are unsupported".to_string(),
        ));
    }
    let mut content_length = None;
    let mut transfer_encoding = None;
    let mut connection_tokens = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or_else(|| {
            TransportError::InvalidResponse("malformed response header".to_string())
        })?;
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => {
                let length = value.trim().parse::<usize>().map_err(|_| {
                    TransportError::InvalidResponse("invalid content length".to_string())
                })?;
                if content_length.replace(length).is_some() {
                    return Err(TransportError::InvalidResponse(
                        "duplicate content length".to_string(),
                    ));
                }
            }
            "transfer-encoding" => {
                if transfer_encoding
                    .replace(value.trim().to_string())
                    .is_some()
                {
                    return Err(TransportError::InvalidResponse(
                        "duplicate transfer encoding".to_string(),
                    ));
                }
            }
            "connection" => connection_tokens.extend(
                value
                    .split(',')
                    .map(|token| token.trim().to_ascii_lowercase()),
            ),
            _ => {}
        }
    }
    let chunked = match transfer_encoding.as_deref() {
        None => false,
        Some(value) if value.eq_ignore_ascii_case("chunked") => true,
        Some(_) => {
            return Err(TransportError::InvalidResponse(
                "unsupported transfer encoding".to_string(),
            ))
        }
    };
    if chunked && content_length.is_some() {
        return Err(TransportError::InvalidResponse(
            "ambiguous response framing".to_string(),
        ));
    }
    let body = if method == "HEAD" || matches!(code, 204 | 304) {
        BodyFraming::None
    } else if chunked {
        BodyFraming::Chunked
    } else if let Some(length) = content_length {
        BodyFraming::Length(length)
    } else {
        BodyFraming::Close
    };
    let close = connection_tokens.iter().any(|token| token == "close");
    let keep_alive = connection_tokens.iter().any(|token| token == "keep-alive");
    Ok(ResponseFraming {
        body,
        reusable: !close && (version == "HTTP/1.1" || keep_alive),
    })
}

fn read_chunked_end(
    stream: &mut impl Read,
    raw: &mut Vec<u8>,
    header_end: usize,
    config: HttpExchangeConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<usize> {
    loop {
        if let Some(length) = chunked_wire_len(&raw[header_end..])? {
            return Ok(header_end.saturating_add(length));
        }
        read_more(stream, raw, config, cancellation)?;
    }
}

fn chunked_wire_len(input: &[u8]) -> TransportResult<Option<usize>> {
    let mut cursor = 0usize;
    loop {
        let Some(offset) = input
            .get(cursor..)
            .and_then(|remaining| remaining.windows(2).position(|window| window == b"\r\n"))
        else {
            return Ok(None);
        };
        let line_end = cursor + offset;
        let size_text = std::str::from_utf8(&input[cursor..line_end])
            .map_err(|_| TransportError::InvalidResponse("invalid chunk size".to_string()))?;
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or_default(), 16)
            .map_err(|_| TransportError::InvalidResponse("invalid chunk size".to_string()))?;
        cursor = line_end + 2;
        if size == 0 {
            if input.get(cursor..cursor.saturating_add(2)) == Some(b"\r\n") {
                return Ok(Some(cursor + 2));
            }
            return Ok(input.get(cursor..).and_then(|trailers| {
                trailers
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|offset| cursor + offset + 4)
            }));
        }
        let chunk_end = cursor
            .checked_add(size)
            .ok_or_else(|| TransportError::InvalidResponse("chunk overflow".to_string()))?;
        if input.len() < chunk_end.saturating_add(2) {
            return Ok(None);
        }
        if input.get(chunk_end..chunk_end + 2) != Some(b"\r\n") {
            return Err(TransportError::InvalidResponse(
                "invalid chunk terminator".to_string(),
            ));
        }
        cursor = chunk_end + 2;
    }
}

fn read_until(
    stream: &mut impl Read,
    raw: &mut Vec<u8>,
    end: usize,
    config: HttpExchangeConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<()> {
    if end > maximum_wire_bytes(config) {
        return Err(TransportError::ResponseTooLarge {
            max: config.max_response_bytes,
        });
    }
    while raw.len() < end {
        read_more(stream, raw, config, cancellation)?;
    }
    Ok(())
}

fn read_to_close(
    stream: &mut impl Read,
    raw: &mut Vec<u8>,
    config: HttpExchangeConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<()> {
    while read_chunk(stream, raw, config, cancellation)? != 0 {}
    Ok(())
}

fn read_more(
    stream: &mut impl Read,
    raw: &mut Vec<u8>,
    config: HttpExchangeConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<()> {
    if read_chunk(stream, raw, config, cancellation)? == 0 {
        return if raw.is_empty() {
            Err(TransportError::InvalidResponse(
                "empty response".to_string(),
            ))
        } else {
            Err(TransportError::TruncatedResponse)
        };
    }
    Ok(())
}

fn read_chunk(
    stream: &mut impl Read,
    raw: &mut Vec<u8>,
    config: HttpExchangeConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<usize> {
    check_cancelled(cancellation)?;
    let mut chunk = [0u8; 8_192];
    let read = stream.read(&mut chunk).map_err(map_io_error)?;
    if raw.len().saturating_add(read) > maximum_wire_bytes(config) {
        return Err(TransportError::ResponseTooLarge {
            max: config.max_response_bytes,
        });
    }
    raw.extend_from_slice(&chunk[..read]);
    Ok(read)
}

fn maximum_wire_bytes(config: HttpExchangeConfig) -> usize {
    config
        .max_header_bytes
        .saturating_add(config.max_response_bytes.saturating_mul(8))
        .saturating_add(8_192)
}

fn check_cancelled(cancellation: Option<&CancellationToken>) -> TransportResult<()> {
    if cancellation.is_some_and(CancellationToken::is_cancelled) {
        Err(TransportError::Cancelled)
    } else {
        Ok(())
    }
}
