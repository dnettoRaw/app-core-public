// =============================================================================
//        #######
//     ###       ###     F: response.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{HttpResponse, TransportError, TransportResult};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};

/// Parses, bounds, de-chunks, and optionally decompresses an HTTP response.
pub fn parse_response(
    raw: &[u8],
    max_header_bytes: usize,
    max_response_bytes: usize,
) -> TransportResult<HttpResponse> {
    let header_end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| TransportError::InvalidResponse("missing headers".to_string()))?;
    if header_end > max_header_bytes {
        return Err(TransportError::InvalidResponse(
            "headers exceed configured limit".to_string(),
        ));
    }
    let header_text = std::str::from_utf8(&raw[..header_end])
        .map_err(|_| TransportError::InvalidResponse("headers are not UTF-8".to_string()))?;
    let (status_code, headers) = parse_headers(header_text)?;
    let encoded = &raw[header_end + 4..];
    let transfer = header_value(&headers, "transfer-encoding");
    let mut body = if transfer.is_some_and(|value| value.eq_ignore_ascii_case("chunked")) {
        decode_chunked(encoded, max_response_bytes)?
    } else {
        validate_content_length(&headers, encoded)?;
        if encoded.len() > max_response_bytes {
            return Err(TransportError::ResponseTooLarge {
                max: max_response_bytes,
            });
        }
        encoded.to_vec()
    };
    if header_value(&headers, "content-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("gzip"))
    {
        body = decode_gzip_limited(&body, max_response_bytes)?;
    }
    Ok(HttpResponse {
        status_code,
        headers,
        body,
    })
}

/// Gzip-compresses bytes only when the result is smaller.
pub fn encode_gzip_if_smaller(input: &[u8]) -> TransportResult<Option<Vec<u8>>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(input)
        .map_err(|error| TransportError::Io(error.to_string()))?;
    let encoded = encoder
        .finish()
        .map_err(|error| TransportError::Io(error.to_string()))?;
    Ok((encoded.len() < input.len()).then_some(encoded))
}

/// Decodes gzip while enforcing the maximum decompressed size.
pub fn decode_gzip_limited(input: &[u8], max_bytes: usize) -> TransportResult<Vec<u8>> {
    let mut decoder = GzDecoder::new(input);
    let mut output = Vec::new();
    decoder
        .by_ref()
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut output)
        .map_err(|error| {
            TransportError::InvalidResponse(format!("malformed gzip body: {error}"))
        })?;
    if output.len() > max_bytes {
        return Err(TransportError::ResponseTooLarge { max: max_bytes });
    }
    Ok(output)
}

fn parse_headers(header_text: &str) -> TransportResult<(u16, Vec<(String, String)>)> {
    let mut lines = header_text.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| TransportError::InvalidResponse("missing status line".to_string()))?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| TransportError::InvalidResponse("missing status code".to_string()))?
        .parse::<u16>()
        .map_err(|_| TransportError::InvalidResponse("invalid status code".to_string()))?;
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(|| {
            TransportError::InvalidResponse("malformed response header".to_string())
        })?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    Ok((status_code, headers))
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.as_str())
}

fn validate_content_length(headers: &[(String, String)], encoded: &[u8]) -> TransportResult<()> {
    let Some(value) = header_value(headers, "content-length") else {
        return Ok(());
    };
    let expected = value
        .parse::<usize>()
        .map_err(|_| TransportError::InvalidResponse("invalid content length".to_string()))?;
    if encoded.len() < expected {
        return Err(TransportError::TruncatedResponse);
    }
    if encoded.len() > expected {
        return Err(TransportError::InvalidResponse(
            "response exceeds declared content length".to_string(),
        ));
    }
    Ok(())
}

fn decode_chunked(input: &[u8], max_bytes: usize) -> TransportResult<Vec<u8>> {
    let mut cursor = 0usize;
    let mut output = Vec::new();
    loop {
        let line_end = input
            .get(cursor..)
            .and_then(|remaining| remaining.windows(2).position(|window| window == b"\r\n"))
            .map(|offset| cursor + offset)
            .ok_or(TransportError::TruncatedResponse)?;
        let size_text = std::str::from_utf8(&input[cursor..line_end])
            .map_err(|_| TransportError::InvalidResponse("invalid chunk size".to_string()))?;
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or_default(), 16)
            .map_err(|_| TransportError::InvalidResponse("invalid chunk size".to_string()))?;
        cursor = line_end + 2;
        if size == 0 {
            return Ok(output);
        }
        let chunk_end = cursor
            .checked_add(size)
            .ok_or_else(|| TransportError::InvalidResponse("chunk overflow".to_string()))?;
        if chunk_end.saturating_add(2) > input.len()
            || input.get(chunk_end..chunk_end + 2) != Some(b"\r\n")
        {
            return Err(TransportError::TruncatedResponse);
        }
        if output.len().saturating_add(size) > max_bytes {
            return Err(TransportError::ResponseTooLarge { max: max_bytes });
        }
        output.extend_from_slice(&input[cursor..chunk_end]);
        cursor = chunk_end + 2;
    }
}
