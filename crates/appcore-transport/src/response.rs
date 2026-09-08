// =============================================================================
//        #######
//     ###       ###     F: response.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Defines bounded response contracts and behavior for this crate.

use crate::{HttpResponse, TransportError, TransportResult};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::borrow::Cow;
use std::io::{Read, Write};

struct ParsedResponseHead {
    body_start: usize,
    status_code: u16,
    headers: Vec<(String, String)>,
}

/// Parses, bounds, de-chunks, and optionally decompresses an HTTP response.
pub fn parse_response(
    raw: &[u8],
    max_header_bytes: usize,
    max_response_bytes: usize,
) -> TransportResult<HttpResponse> {
    let head = parse_response_head(raw, max_header_bytes)?;
    let body = parse_response_body(&raw[head.body_start..], &head.headers, max_response_bytes)?;
    Ok(HttpResponse {
        status_code: head.status_code,
        headers: head.headers,
        body,
    })
}

/// Parses an owned HTTP frame and reuses its allocation for identity bodies.
///
/// Both fixed and chunked identity bodies are compacted in place. Compressed
/// bodies still require their bounded decompressed output.
pub fn parse_response_owned(
    mut raw: Vec<u8>,
    max_header_bytes: usize,
    max_response_bytes: usize,
) -> TransportResult<HttpResponse> {
    let head = parse_response_head(&raw, max_header_bytes)?;
    let chunked = header_value(&head.headers, "transfer-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"));
    let compressed = header_value(&head.headers, "content-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("gzip"));
    let body = if chunked {
        let body_len = decode_chunked_in_place(&mut raw, head.body_start, max_response_bytes)?;
        if compressed {
            decode_gzip_limited(&raw[..body_len], max_response_bytes)?
        } else {
            raw.truncate(body_len);
            raw
        }
    } else if !compressed {
        let encoded = &raw[head.body_start..];
        validate_identity_body(&head.headers, encoded, max_response_bytes)?;
        let body_len = encoded.len();
        raw.copy_within(head.body_start.., 0);
        raw.truncate(body_len);
        raw
    } else {
        parse_response_body(&raw[head.body_start..], &head.headers, max_response_bytes)?
    };
    Ok(HttpResponse {
        status_code: head.status_code,
        headers: head.headers,
        body,
    })
}

fn parse_response_head(raw: &[u8], max_header_bytes: usize) -> TransportResult<ParsedResponseHead> {
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
    Ok(ParsedResponseHead {
        body_start: header_end + 4,
        status_code,
        headers,
    })
}

fn parse_response_body(
    encoded: &[u8],
    headers: &[(String, String)],
    max_response_bytes: usize,
) -> TransportResult<Vec<u8>> {
    let transfer = header_value(headers, "transfer-encoding");
    let body = if transfer.is_some_and(|value| value.eq_ignore_ascii_case("chunked")) {
        Cow::Owned(decode_chunked(encoded, max_response_bytes)?)
    } else {
        validate_identity_body(headers, encoded, max_response_bytes)?;
        Cow::Borrowed(encoded)
    };
    let body = if header_value(headers, "content-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("gzip"))
    {
        decode_gzip_limited(&body, max_response_bytes)?
    } else {
        body.into_owned()
    };
    Ok(body)
}

fn validate_identity_body(
    headers: &[(String, String)],
    encoded: &[u8],
    max_response_bytes: usize,
) -> TransportResult<()> {
    validate_content_length(headers, encoded)?;
    if encoded.len() > max_response_bytes {
        return Err(TransportError::ResponseTooLarge {
            max: max_response_bytes,
        });
    }
    Ok(())
}

/// Gzip-compresses bytes only when the result is smaller.
/// Stops retaining output before it can reach the input length. This bounds
/// requested output capacity, not the codec's internal workspace or process RSS.
pub fn encode_gzip_if_smaller(input: &[u8]) -> TransportResult<Option<Vec<u8>>> {
    if input.is_empty() {
        return Ok(None);
    }
    let mut output = GzipCandidate {
        bytes: Vec::new(),
        max: input.len() - 1,
        exceeded: false,
    };
    let result = {
        let mut encoder = GzEncoder::new(&mut output, Compression::fast());
        encoder.write_all(input).and_then(|_| encoder.try_finish())
    };
    if output.exceeded {
        return Ok(None);
    }
    result.map_err(|error| TransportError::Io(error.to_string()))?;
    Ok(Some(output.bytes))
}

struct GzipCandidate {
    bytes: Vec<u8>,
    max: usize,
    exceeded: bool,
}

impl Write for GzipCandidate {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.exceeded || bytes.len() > self.max.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::from(std::io::ErrorKind::WriteZero));
        }
        let needed = self.bytes.len() + bytes.len();
        if needed > self.bytes.capacity() {
            let capacity = needed
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.max);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(std::io::Error::other)?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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

fn decode_chunked_in_place(
    raw: &mut [u8],
    body_start: usize,
    max_bytes: usize,
) -> TransportResult<usize> {
    let mut cursor = body_start;
    let mut written = 0usize;
    loop {
        let line_end = raw
            .get(cursor..)
            .and_then(|remaining| remaining.windows(2).position(|window| window == b"\r\n"))
            .map(|offset| cursor + offset)
            .ok_or(TransportError::TruncatedResponse)?;
        let size_text = std::str::from_utf8(&raw[cursor..line_end])
            .map_err(|_| TransportError::InvalidResponse("invalid chunk size".to_string()))?;
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or_default(), 16)
            .map_err(|_| TransportError::InvalidResponse("invalid chunk size".to_string()))?;
        cursor = line_end + 2;
        if size == 0 {
            return Ok(written);
        }
        let chunk_end = cursor
            .checked_add(size)
            .ok_or_else(|| TransportError::InvalidResponse("chunk overflow".to_string()))?;
        if chunk_end.saturating_add(2) > raw.len()
            || raw.get(chunk_end..chunk_end + 2) != Some(b"\r\n")
        {
            return Err(TransportError::TruncatedResponse);
        }
        if written.saturating_add(size) > max_bytes {
            return Err(TransportError::ResponseTooLarge { max: max_bytes });
        }
        raw.copy_within(cursor..chunk_end, written);
        written += size;
        cursor = chunk_end + 2;
    }
}

#[cfg(test)]
mod gzip_budget_tests {
    use super::*;

    #[test]
    fn owned_identity_response_reuses_the_raw_allocation() {
        let mut raw = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nbody".to_vec();
        raw.reserve_exact(32);
        let pointer = raw.as_ptr();
        let capacity = raw.capacity();

        let response = parse_response_owned(raw, 1024, 4).unwrap();

        assert_eq!(response.body, b"body");
        assert_eq!(response.body.as_ptr(), pointer);
        assert_eq!(response.body.capacity(), capacity);
    }

    #[test]
    fn owned_chunked_identity_response_reuses_the_raw_allocation() {
        let mut raw =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nbo\r\n2\r\ndy\r\n0\r\n\r\n"
                .to_vec();
        raw.reserve_exact(32);
        let pointer = raw.as_ptr();
        let capacity = raw.capacity();
        let response = parse_response_owned(raw, 1_024, 4).unwrap();
        assert_eq!(response.body, b"body");
        assert_eq!(response.body.as_ptr(), pointer);
        assert_eq!(response.body.capacity(), capacity);
    }

    #[test]
    fn owned_compressed_response_preserves_borrowed_parser_semantics() {
        let payload = vec![b'a'; 1_024];
        let encoded = encode_gzip_if_smaller(&payload).unwrap().unwrap();
        let mut raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
            encoded.len()
        )
        .into_bytes();
        raw.extend_from_slice(&encoded);

        let borrowed = parse_response(&raw, 1_024, payload.len()).unwrap();
        let owned = parse_response_owned(raw, 1_024, payload.len()).unwrap();

        assert_eq!(owned, borrowed);
    }

    #[test]
    fn owned_chunked_gzip_preserves_borrowed_parser_semantics() {
        let payload = vec![b'a'; 1_024];
        let encoded = encode_gzip_if_smaller(&payload).unwrap().unwrap();
        let mut raw = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Encoding: gzip\r\n\r\n{:x}\r\n",
            encoded.len()
        )
        .into_bytes();
        raw.extend_from_slice(&encoded);
        raw.extend_from_slice(b"\r\n0\r\n\r\n");

        let borrowed = parse_response(&raw, 1_024, payload.len()).unwrap();
        let owned = parse_response_owned(raw, 1_024, payload.len()).unwrap();

        assert_eq!(owned, borrowed);
    }

    #[test]
    fn owned_chunked_rejects_truncation_and_decoded_limit() {
        let truncated =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nbody\r".to_vec();
        assert_eq!(
            parse_response_owned(truncated, 1_024, 4),
            Err(TransportError::TruncatedResponse)
        );
        let oversized =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nbody\r\n0\r\n\r\n".to_vec();
        assert_eq!(
            parse_response_owned(oversized, 1_024, 3),
            Err(TransportError::ResponseTooLarge { max: 3 })
        );
    }

    #[test]
    fn non_chunked_gzip_preserves_limits_framing_and_owned_result() {
        let payload = vec![b'a'; 1_024];
        let encoded = encode_gzip_if_smaller(&payload).unwrap().unwrap();
        let mut raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
            encoded.len()
        )
        .into_bytes();
        raw.extend_from_slice(&encoded);
        assert_eq!(
            parse_response(&raw, 1_024, 1_023),
            Err(TransportError::ResponseTooLarge { max: 1_023 })
        );
        assert_eq!(
            parse_response(&raw, 1_024, encoded.len() - 1),
            Err(TransportError::ResponseTooLarge {
                max: encoded.len() - 1
            })
        );
        let response = parse_response(&raw, 1_024, 1_024).unwrap();
        raw.pop();
        assert_eq!(
            parse_response(&raw, 1_024, 1_024),
            Err(TransportError::TruncatedResponse)
        );
        raw.clear();
        assert_eq!(response.body, payload);
    }

    #[test]
    fn candidate_rejects_before_growth_and_stays_rejected() {
        let mut candidate = GzipCandidate {
            bytes: Vec::new(),
            max: 8,
            exceeded: false,
        };
        candidate.write_all(b"1234").unwrap();
        let capacity = candidate.bytes.capacity();
        assert!(candidate.write_all(b"56789").is_err());
        assert_eq!(candidate.bytes, b"1234");
        assert_eq!(candidate.bytes.capacity(), capacity);
        assert!(candidate.write_all(b"5").is_err());
    }

    #[test]
    fn bounded_candidate_matches_complete_encoder_decision_and_bytes() {
        let mut state = 0x1234_5678_u32;
        let random: Vec<u8> = (0..65_536)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        for len in (0..256).chain([1_024, 16_384, 65_536]) {
            for input in [&random[..len], &vec![b'a'; len][..]] {
                let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
                encoder.write_all(input).unwrap();
                let full = encoder.finish().unwrap();
                let expected = (full.len() < input.len()).then_some(full);
                let actual = encode_gzip_if_smaller(input).unwrap();
                assert_eq!(actual, expected, "input length {len}");
                if let Some(encoded) = actual {
                    assert_eq!(decode_gzip_limited(&encoded, len).unwrap(), input);
                }
            }
        }
    }
}
