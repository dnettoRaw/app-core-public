// =============================================================================
//        #######
//     ###       ###     F: cipher.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 10:49:08 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT AEAD and keyed digest helpers.

use crate::{DntError, DntResult};
use chacha20poly1305::aead::{Aead, AeadInPlace, KeyInit, Payload};
use chacha20poly1305::{Key, Tag, XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::ops::Range;
use zeroize::Zeroize;

type HmacSha256 = Hmac<Sha256>;
const AEAD_TAG_BYTES: usize = 16;

pub(crate) fn encrypt(
    key: &crate::SecretKey,
    nonce: &[u8; 24],
    aad: &[u8],
    plaintext: &[u8],
) -> DntResult<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose_key()));
    cipher
        .encrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| DntError::AuthenticationFailed)
}

pub(crate) fn decrypt(
    key: &crate::SecretKey,
    nonce: &[u8; 24],
    aad: &[u8],
    ciphertext: &[u8],
) -> DntResult<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose_key()));
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| DntError::AuthenticationFailed)
}

pub(crate) fn decrypt_owned_in_place(
    key: &crate::SecretKey,
    nonce: &[u8; 24],
    header_len: usize,
    input: &mut [u8],
) -> DntResult<Range<usize>> {
    if input.len() < header_len {
        return Err(DntError::InvalidFormat);
    }
    let (aad, ciphertext_with_tag) = input.split_at_mut(header_len);
    if ciphertext_with_tag.len() < AEAD_TAG_BYTES {
        return Err(DntError::InvalidFormat);
    }
    let tag_start = ciphertext_with_tag.len() - AEAD_TAG_BYTES;
    let (ciphertext, tag) = ciphertext_with_tag.split_at_mut(tag_start);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose_key()));
    cipher
        .decrypt_in_place_detached(
            XNonce::from_slice(nonce),
            aad,
            ciphertext,
            Tag::from_slice(tag),
        )
        .map_err(|_| DntError::AuthenticationFailed)?;
    Ok(header_len..header_len + tag_start)
}

pub(crate) fn payload_digest(key: &crate::SecretKey, payload: &[u8]) -> DntResult<[u8; 32]> {
    let mut digest_key = derive_payload_digest_key(key)?;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&digest_key)
        .map_err(|_| DntError::AuthenticationFailed)?;
    mac.update(b"appcore-dnt-payload-digest-v1");
    let payload_len = u64::try_from(payload.len()).map_err(|_| DntError::PayloadTooLarge)?;
    mac.update(&payload_len.to_be_bytes());
    mac.update(payload);
    let digest = mac.finalize().into_bytes().into();
    digest_key.zeroize();
    Ok(digest)
}

pub(crate) fn verify_payload_digest(
    key: &crate::SecretKey,
    payload: &[u8],
    expected: &[u8; 32],
) -> DntResult<bool> {
    let mut digest_key = derive_payload_digest_key(key)?;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&digest_key)
        .map_err(|_| DntError::AuthenticationFailed)?;
    mac.update(b"appcore-dnt-payload-digest-v1");
    let payload_len = u64::try_from(payload.len()).map_err(|_| DntError::PayloadTooLarge)?;
    mac.update(&payload_len.to_be_bytes());
    mac.update(payload);
    let result = mac.verify_slice(expected).is_ok();
    digest_key.zeroize();
    Ok(result)
}

pub(crate) fn random_nonce() -> DntResult<[u8; 24]> {
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut nonce).map_err(|_| DntError::Io)?;
    Ok(nonce)
}

fn derive_payload_digest_key(key: &crate::SecretKey) -> DntResult<[u8; 32]> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key.expose_key())
        .map_err(|_| DntError::AuthenticationFailed)?;
    mac.update(b"appcore-dnt-payload-digest-key-v1");
    Ok(mac.finalize().into_bytes().into())
}
