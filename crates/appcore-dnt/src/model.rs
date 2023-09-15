// =============================================================================
//        #######
//     ###       ###     F: model.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! High-level DNT seal, open, verify, rekey and migration operations.

use crate::cipher::{
    decrypt, decrypt_owned_in_place, encrypt, payload_digest, random_nonce, verify_payload_digest,
};
use crate::compression::{decode_payload, encode_payload};
use crate::flags::validate_flags;
use crate::header::{encode_header, HeaderParts};
use crate::model_types::{DntContext, DntOpenOptions, DntSealOptions, OpenedDnt, VerifiedDnt};
use crate::plaintext::{decode_plaintext_ranges, encode_plaintext};
use crate::{
    inspect_header, DntAlgorithm, DntCodec, DntError, DntHeader, DntKeyProvider, DntResult,
};
use std::ops::Range;
use zeroize::Zeroize;

/// Seals arbitrary bytes into a DNT envelope.
pub fn seal<P, C>(
    payload: &[u8],
    key_provider: &P,
    codec: &C,
    options: DntSealOptions,
) -> DntResult<Vec<u8>>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    enforce_max(payload.len() as u64, options.max_payload_bytes)?;
    validate_flags(options.flags)?;
    let encoded = codec.encode(payload)?;
    enforce_max(encoded.len() as u64, options.max_payload_bytes)?;
    let mut stored_payload = encode_payload(options.flags, encoded, options.max_payload_bytes)?;
    enforce_max(stored_payload.len() as u64, options.max_payload_bytes)?;
    let encrypted_metadata_length =
        u32::try_from(options.encrypted_metadata.len()).map_err(|_| DntError::PayloadTooLarge)?;
    let codec_id = codec.codec_id();
    let context = DntContext {
        application_id: options.application_id.clone(),
        tenant_id: options.tenant_id.clone(),
        content_type: options.content_type.clone(),
        codec_id: codec_id.clone(),
        schema_version: options.schema_version,
    };
    let key = key_provider.resolve_key(&options.key_id, &context)?;
    let nonce = random_nonce()?;
    let payload_hash = match payload_digest(&key, &stored_payload) {
        Ok(payload_hash) => payload_hash,
        Err(error) => {
            stored_payload.zeroize();
            return Err(error);
        }
    };
    let payload_length = match u64::try_from(stored_payload.len()) {
        Ok(payload_length) => payload_length,
        Err(_) => {
            stored_payload.zeroize();
            return Err(DntError::PayloadTooLarge);
        }
    };
    let header = match encode_header(HeaderParts {
        flags: options.flags,
        algorithm: DntAlgorithm::XChaCha20Poly1305,
        application_id: options.application_id,
        tenant_id: options.tenant_id,
        content_type: options.content_type,
        codec_id,
        key_id: options.key_id,
        schema_version: options.schema_version,
        created_at_ms: options.created_at_ms,
        payload_length,
        nonce,
        payload_hash,
        public_metadata: options.public_metadata,
        encrypted_metadata_length,
    }) {
        Ok(header) => header,
        Err(error) => {
            stored_payload.zeroize();
            return Err(error);
        }
    };
    let plaintext_result = encode_plaintext(&options.encrypted_metadata, &stored_payload);
    stored_payload.zeroize();
    let mut plaintext = plaintext_result?;
    let ciphertext_result = encrypt(&key, &nonce, &header, &plaintext);
    plaintext.zeroize();
    let ciphertext = ciphertext_result?;
    let mut output = Vec::with_capacity(header.len() + ciphertext.len());
    output.extend_from_slice(&header);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Opens, authenticates and decodes a complete DNT envelope.
pub fn open<P, C>(
    input: &[u8],
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<OpenedDnt>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    open_internal(input, key_provider, codec, options)
}

/// Opens an owned envelope, allowing in-place decryption of the file buffer.
///
/// Prefer this API after reading a complete DNT file into a `Vec<u8>`.
/// It preserves the same authentication and context checks as [`open`] while
/// avoiding an extra plaintext allocation in the read path.
pub fn open_owned<P, C>(
    input: Vec<u8>,
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<OpenedDnt>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    open_owned_internal(input, key_provider, codec, options)
}

/// Cryptographically verifies a DNT envelope and discards plaintext.
pub fn verify<P, C>(
    input: &[u8],
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<VerifiedDnt>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    let mut authenticated = authenticate_internal(input, key_provider, codec, options)?;
    if authenticated.header.compression().is_compacted() {
        let encoded_result = decode_payload(
            authenticated.header.flags,
            authenticated.stored_payload(),
            options.max_payload_bytes,
        );
        authenticated.zeroize_buffer();
        match encoded_result {
            Ok(mut encoded_payload) => encoded_payload.zeroize(),
            Err(error) => {
                authenticated.encrypted_metadata.zeroize();
                return Err(error);
            }
        }
    } else {
        authenticated.zeroize_buffer();
    }
    authenticated.encrypted_metadata.zeroize();
    Ok(VerifiedDnt {
        header: authenticated.header,
    })
}

struct AuthenticatedDnt {
    header: DntHeader,
    encrypted_metadata: Vec<u8>,
    buffer: Vec<u8>,
    payload_range: Range<usize>,
}

impl AuthenticatedDnt {
    fn stored_payload(&self) -> &[u8] {
        &self.buffer[self.payload_range.clone()]
    }

    fn take_payload_buffer(&mut self) -> Vec<u8> {
        let payload_start = self.payload_range.start;
        let payload_end = self.payload_range.end;
        let payload_len = payload_end - payload_start;
        if payload_start != 0 {
            self.buffer.copy_within(payload_start..payload_end, 0);
        }
        self.buffer[payload_len..payload_end].zeroize();
        self.buffer.truncate(payload_len);
        std::mem::take(&mut self.buffer)
    }

    fn zeroize_buffer(&mut self) {
        self.buffer.zeroize();
    }
}

fn open_internal<P, C>(
    input: &[u8],
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<OpenedDnt>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    let authenticated = authenticate_internal(input, key_provider, codec, options)?;
    open_authenticated(authenticated, codec, options)
}

fn open_owned_internal<P, C>(
    input: Vec<u8>,
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<OpenedDnt>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    let authenticated = authenticate_owned_internal(input, key_provider, codec, options)?;
    open_authenticated(authenticated, codec, options)
}

fn open_authenticated<C>(
    mut authenticated: AuthenticatedDnt,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<OpenedDnt>
where
    C: DntCodec,
{
    let payload_result = if authenticated.header.compression().is_compacted() {
        let encoded_result = decode_payload(
            authenticated.header.flags,
            authenticated.stored_payload(),
            options.max_payload_bytes,
        );
        authenticated.zeroize_buffer();
        let encoded_payload = match encoded_result {
            Ok(encoded_payload) => encoded_payload,
            Err(error) => {
                authenticated.encrypted_metadata.zeroize();
                return Err(error);
            }
        };
        codec.decode_owned(encoded_payload)
    } else {
        codec.decode_owned(authenticated.take_payload_buffer())
    };
    let mut payload = match payload_result {
        Ok(payload) => payload,
        Err(error) => {
            authenticated.encrypted_metadata.zeroize();
            return Err(error.into());
        }
    };
    if let Err(error) = enforce_max(payload.len() as u64, options.max_payload_bytes) {
        payload.zeroize();
        authenticated.encrypted_metadata.zeroize();
        return Err(error);
    }
    let opened = OpenedDnt {
        header: authenticated.header,
        payload,
        encrypted_metadata: authenticated.encrypted_metadata,
    };
    Ok(opened)
}

fn authenticate_internal<P, C>(
    input: &[u8],
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<AuthenticatedDnt>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    let header = inspect_header(input)?;
    validate_header_context(&header, codec, options)?;
    enforce_max(header.payload_length, options.max_payload_bytes)?;
    let header_len = header.header_length as usize;
    let ciphertext = input.get(header_len..).ok_or(DntError::InvalidFormat)?;
    if ciphertext.is_empty() {
        return Err(DntError::InvalidFormat);
    }
    let context = DntContext::from_header(&header);
    let key = key_provider.resolve_key(&header.key_id, &context)?;
    let mut plaintext = decrypt(&key, header.nonce(), &input[..header_len], ciphertext)?;
    let (encrypted_metadata_range, payload_start) =
        match decode_plaintext_ranges(&plaintext, &header) {
            Ok(ranges) => ranges,
            Err(error) => {
                plaintext.zeroize();
                return Err(error);
            }
        };
    let digest_matches =
        match verify_payload_digest(&key, &plaintext[payload_start..], &header.payload_hash) {
            Ok(matches) => matches,
            Err(error) => {
                plaintext.zeroize();
                return Err(error);
            }
        };
    if !digest_matches {
        plaintext.zeroize();
        return Err(DntError::AuthenticationFailed);
    }
    let encrypted_metadata = plaintext[encrypted_metadata_range].to_vec();
    let payload_range = payload_start..plaintext.len();
    let authenticated = AuthenticatedDnt {
        header,
        encrypted_metadata,
        buffer: plaintext,
        payload_range,
    };
    Ok(authenticated)
}

fn authenticate_owned_internal<P, C>(
    mut input: Vec<u8>,
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<AuthenticatedDnt>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    let header = inspect_header(&input)?;
    validate_header_context(&header, codec, options)?;
    enforce_max(header.payload_length, options.max_payload_bytes)?;
    let header_len = header.header_length as usize;
    if input.len() <= header_len {
        return Err(DntError::InvalidFormat);
    }
    let context = DntContext::from_header(&header);
    let key = key_provider.resolve_key(&header.key_id, &context)?;
    let plaintext_range = match decrypt_owned_in_place(&key, header.nonce(), header_len, &mut input)
    {
        Ok(range) => range,
        Err(error) => {
            input.zeroize();
            return Err(error);
        }
    };
    let (encrypted_metadata_range, payload_start) =
        match decode_plaintext_ranges(&input[plaintext_range.clone()], &header) {
            Ok(ranges) => ranges,
            Err(error) => {
                input.zeroize();
                return Err(error);
            }
        };
    let payload_start = plaintext_range.start + payload_start;
    let encrypted_metadata_range = plaintext_range.start + encrypted_metadata_range.start
        ..plaintext_range.start + encrypted_metadata_range.end;
    let digest_matches = match verify_payload_digest(
        &key,
        &input[payload_start..plaintext_range.end],
        &header.payload_hash,
    ) {
        Ok(matches) => matches,
        Err(error) => {
            input.zeroize();
            return Err(error);
        }
    };
    if !digest_matches {
        input.zeroize();
        return Err(DntError::AuthenticationFailed);
    }
    let encrypted_metadata = input[encrypted_metadata_range].to_vec();
    let authenticated = AuthenticatedDnt {
        header,
        encrypted_metadata,
        buffer: input,
        payload_range: payload_start..plaintext_range.end,
    };
    Ok(authenticated)
}

fn validate_header_context<C>(
    header: &DntHeader,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<()>
where
    C: DntCodec,
{
    if header.application_id != options.application_id
        || header.tenant_id != options.tenant_id
        || header.content_type != options.content_type
    {
        return Err(DntError::ContextMismatch);
    }
    if !codec.matches_codec_id(&header.codec_id) {
        return Err(DntError::CodecUnavailable);
    }
    Ok(())
}

fn enforce_max(actual: u64, max: Option<u64>) -> DntResult<()> {
    if max.is_some_and(|max| actual > max) {
        return Err(DntError::PayloadTooLarge);
    }
    Ok(())
}
