// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:38:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use crate::{
    dnt_compose_flags, dnt_user_flag, inspect_header, open, open_owned, read_verified, rekey, seal,
    verify, write_atomic, BytesCodec, CodecError, CodecId, ContentType, DntCodec, DntCompression,
    DntError, DntFlags, DntOpenOptions, DntSealOptions, KeyId, SecretKey, StaticDntKeyProvider,
    DNT_CONTENT_JSON, DNT_CONTENT_OCTET_STREAM, DNT_FLAG_PAYLOAD_DEFLATE, DNT_INTERNAL_FLAG_MASK,
    DNT_MAGIC, DNT_MAX_ENCRYPTED_METADATA_BYTES, DNT_USER_FLAG_MASK,
};
use appcore_contracts::ApplicationId;
use appcore_types::TenantId;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn app_id() -> ApplicationId {
    ApplicationId::new("app-a").unwrap()
}

fn tenant_id() -> TenantId {
    TenantId::new("tenant-a").unwrap()
}

fn key(byte: u8) -> SecretKey {
    SecretKey::new([byte; 32])
}

fn key_id(value: &str) -> KeyId {
    KeyId::new(value).unwrap()
}

fn provider() -> StaticDntKeyProvider {
    StaticDntKeyProvider::new()
        .with_key(key_id("key-a"), key(7))
        .with_key(key_id("key-b"), key(9))
}

fn seal_options(content_type: &str) -> DntSealOptions {
    DntSealOptions {
        application_id: app_id(),
        tenant_id: Some(tenant_id()),
        content_type: ContentType::new(content_type).unwrap(),
        schema_version: 3,
        key_id: key_id("key-a"),
        created_at_ms: 123,
        public_metadata: b"route=sync".to_vec(),
        encrypted_metadata: b"private".to_vec(),
        flags: 0,
        max_payload_bytes: Some(1024 * 1024),
    }
}

fn open_options(content_type: &str) -> DntOpenOptions {
    DntOpenOptions {
        application_id: app_id(),
        tenant_id: Some(tenant_id()),
        content_type: ContentType::new(content_type).unwrap(),
        max_payload_bytes: Some(1024 * 1024),
    }
}

#[test]
fn round_trip_binary_payload() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    let header = inspect_header(&sealed).unwrap();
    assert_eq!(header.application_id, app_id());
    assert_eq!(header.tenant_id, Some(tenant_id()));
    assert_eq!(header.content_type.as_str(), DNT_CONTENT_OCTET_STREAM);
    assert_eq!(header.public_metadata, b"route=sync");

    let opened = open(
        &sealed,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    assert_eq!(opened.payload, b"payload");
    assert_eq!(opened.encrypted_metadata, b"private");
}

#[test]
fn empty_payload_round_trips() {
    let codec = BytesCodec;
    let sealed = seal(
        b"",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    let opened = open(
        &sealed,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    assert!(opened.payload.is_empty());
}

#[test]
fn registered_object_codec_round_trips() {
    struct ReverseCodec;

    impl DntCodec for ReverseCodec {
        fn codec_id(&self) -> CodecId {
            CodecId::new("reverse-v1").unwrap()
        }

        fn encode(&self, value: &[u8]) -> Result<Vec<u8>, CodecError> {
            Ok(value.iter().rev().copied().collect())
        }

        fn decode(&self, payload: &[u8]) -> Result<Vec<u8>, CodecError> {
            Ok(payload.iter().rev().copied().collect())
        }
    }

    let codec = ReverseCodec;
    let serialized_object = br#"{"schema":1,"enabled":true}"#;
    let sealed = seal(
        serialized_object,
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    let opened = open(
        &sealed,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    assert_eq!(opened.payload, serialized_object);
}

#[test]
fn open_owned_round_trips_with_same_validation() {
    let codec = BytesCodec;
    let sealed = seal(
        b"owned-payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    let opened = open_owned(
        sealed,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    assert_eq!(opened.payload, b"owned-payload");
    assert_eq!(opened.encrypted_metadata, b"private");
}

#[test]
fn round_trip_json_convention() {
    let codec = BytesCodec;
    let sealed = seal(
        br#"{"ok":true}"#,
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_JSON),
    )
    .unwrap();
    let opened = open(
        &sealed,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_JSON),
    )
    .unwrap();
    assert_eq!(opened.payload, br#"{"ok":true}"#);
}

#[test]
fn compact_payload_reduces_repetitive_data_and_round_trips() {
    let codec = BytesCodec;
    let payload =
        br#"{"kind":"snapshot","tenant":"tenant-a","value":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#
            .repeat(4096);
    let normal = seal(
        &payload,
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_JSON),
    )
    .unwrap();
    let mut options = seal_options(DNT_CONTENT_JSON);
    options.flags = DNT_FLAG_PAYLOAD_DEFLATE;
    let compact = seal(&payload, &provider(), &codec, options).unwrap();

    let header = inspect_header(&compact).unwrap();
    assert_eq!(header.compression(), DntCompression::Deflate);
    assert!(header.payload_length < payload.len() as u64);
    assert!(compact.len() < normal.len() / 10);

    let opened = open(
        &compact,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_JSON),
    )
    .unwrap();
    assert_eq!(opened.payload, payload);
    assert!(verify(
        &compact,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_JSON)
    )
    .is_ok());
}

#[test]
fn compact_payload_can_use_builder_style_option() {
    let codec = BytesCodec;
    let options = seal_options(DNT_CONTENT_OCTET_STREAM).compact_payload();
    let sealed = seal(b"payload payload payload", &provider(), &codec, options).unwrap();

    assert_eq!(
        inspect_header(&sealed).unwrap().compression(),
        DntCompression::Deflate
    );
    assert_eq!(
        open(
            &sealed,
            &provider(),
            &codec,
            &open_options(DNT_CONTENT_OCTET_STREAM)
        )
        .unwrap()
        .payload,
        b"payload payload payload"
    );
}

#[test]
fn user_flags_are_partitioned_validated_and_authenticated() {
    let codec = BytesCodec;
    let user_flag = dnt_user_flag(0).unwrap();
    let flags = dnt_compose_flags(DNT_FLAG_PAYLOAD_DEFLATE, user_flag).unwrap();
    let mut options = seal_options(DNT_CONTENT_OCTET_STREAM);
    options.flags = flags;
    let sealed = seal(b"payload payload payload", &provider(), &codec, options).unwrap();

    let header = inspect_header(&sealed).unwrap();
    assert_eq!(
        header.flags & DNT_INTERNAL_FLAG_MASK,
        DNT_FLAG_PAYLOAD_DEFLATE
    );
    assert_eq!(header.flags & DNT_USER_FLAG_MASK, user_flag);

    let mut tampered = sealed;
    tampered[14] ^= 1;
    assert_eq!(
        open(
            &tampered,
            &provider(),
            &codec,
            &open_options(DNT_CONTENT_OCTET_STREAM)
        ),
        Err(DntError::AuthenticationFailed)
    );
}

#[test]
fn user_flag_helpers_reject_impossible_bits() {
    assert_eq!(dnt_user_flag(16), Err(DntError::InvalidFlags));
    assert_eq!(
        dnt_compose_flags(0x0000_0002, 0),
        Err(DntError::InvalidFlags)
    );
    assert_eq!(
        dnt_compose_flags(0, 0x0000_0001),
        Err(DntError::InvalidFlags)
    );

    let flags = DntFlags::empty()
        .compact_payload()
        .with_user_flag(15)
        .unwrap();
    assert_eq!(flags.internal_bits(), DNT_FLAG_PAYLOAD_DEFLATE);
    assert_eq!(flags.user_bits(), dnt_user_flag(15).unwrap());
    assert_eq!(DntFlags::from_bits(flags.bits()).unwrap(), flags);
}

#[test]
fn unknown_internal_flags_are_rejected_before_opening() {
    let codec = BytesCodec;
    let mut options = seal_options(DNT_CONTENT_OCTET_STREAM);
    options.flags = 0x0000_0002;
    assert_eq!(
        seal(b"payload", &provider(), &codec, options),
        Err(DntError::InvalidFlags)
    );

    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    let mut unknown_internal = sealed;
    unknown_internal[17] ^= 0x02;
    assert_eq!(
        inspect_header(&unknown_internal),
        Err(DntError::InvalidFlags)
    );
}

#[test]
fn seal_options_can_add_user_flags_without_raw_shifts() {
    let codec = BytesCodec;
    let options = seal_options(DNT_CONTENT_OCTET_STREAM)
        .with_user_flag(2)
        .unwrap();
    let sealed = seal(b"payload", &provider(), &codec, options).unwrap();

    assert_eq!(
        inspect_header(&sealed).unwrap().flags & DNT_USER_FLAG_MASK,
        dnt_user_flag(2).unwrap()
    );
}

#[test]
fn verify_is_cryptographic() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    let verified = verify(
        &sealed,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    assert_eq!(
        verified.header.content_type.as_str(),
        DNT_CONTENT_OCTET_STREAM
    );
}

#[test]
fn verify_does_not_decode_payload() {
    struct DecodeFailsCodec;

    impl DntCodec for DecodeFailsCodec {
        fn codec_id(&self) -> CodecId {
            CodecId::new("bad-decode").unwrap()
        }

        fn encode(&self, value: &[u8]) -> Result<Vec<u8>, CodecError> {
            Ok(value.to_vec())
        }

        fn decode(&self, _payload: &[u8]) -> Result<Vec<u8>, CodecError> {
            Err(CodecError::DecodeFailed)
        }
    }

    let codec = DecodeFailsCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    assert!(verify(
        &sealed,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM)
    )
    .is_ok());
    assert_eq!(
        open(
            &sealed,
            &provider(),
            &codec,
            &open_options(DNT_CONTENT_OCTET_STREAM)
        ),
        Err(DntError::CodecFailed)
    );
}

#[test]
fn public_payload_digest_does_not_expose_plaintext_sha256() {
    let codec = BytesCodec;
    let sealed = seal(
        b"low-entropy-secret",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    let header = inspect_header(&sealed).unwrap();
    let raw_sha: [u8; 32] = Sha256::digest(b"low-entropy-secret").into();

    assert_ne!(header.payload_hash, raw_sha);
}

#[test]
fn wrong_application_is_rejected_before_opening() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    let options = DntOpenOptions {
        application_id: ApplicationId::new("app-b").unwrap(),
        tenant_id: Some(tenant_id()),
        content_type: ContentType::new(DNT_CONTENT_OCTET_STREAM).unwrap(),
        max_payload_bytes: None,
    };

    assert_eq!(
        open(&sealed, &provider(), &codec, &options),
        Err(DntError::ContextMismatch)
    );
}

#[test]
fn wrong_tenant_is_rejected() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    let options = DntOpenOptions {
        application_id: app_id(),
        tenant_id: Some(TenantId::new("tenant-b").unwrap()),
        content_type: ContentType::new(DNT_CONTENT_OCTET_STREAM).unwrap(),
        max_payload_bytes: None,
    };

    assert_eq!(
        open(&sealed, &provider(), &codec, &options),
        Err(DntError::ContextMismatch)
    );
}

#[test]
fn wrong_content_type_is_rejected() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    assert_eq!(
        open(
            &sealed,
            &provider(),
            &codec,
            &open_options(DNT_CONTENT_JSON)
        ),
        Err(DntError::ContextMismatch)
    );
}

#[test]
fn wrong_key_fails_authentication() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    let wrong = StaticDntKeyProvider::new().with_key(key_id("key-a"), key(1));

    assert_eq!(
        open(
            &sealed,
            &wrong,
            &codec,
            &open_options(DNT_CONTENT_OCTET_STREAM)
        ),
        Err(DntError::AuthenticationFailed)
    );
}

#[test]
fn invalid_magic_and_future_version_are_distinct() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    let mut invalid_magic = sealed.clone();
    invalid_magic[0] ^= 1;
    assert_eq!(inspect_header(&invalid_magic), Err(DntError::InvalidFormat));

    let mut future = sealed;
    future[8..10].copy_from_slice(&99u16.to_be_bytes());
    assert_eq!(inspect_header(&future), Err(DntError::UnsupportedVersion));
}

#[test]
fn truncation_is_rejected() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    assert_eq!(inspect_header(&sealed[..6]), Err(DntError::InvalidFormat));
    assert_eq!(
        open(
            &sealed[..sealed.len() - 4],
            &provider(),
            &codec,
            &open_options(DNT_CONTENT_OCTET_STREAM),
        ),
        Err(DntError::AuthenticationFailed)
    );
}

#[test]
fn tampering_each_region_is_rejected() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    let header = inspect_header(&sealed).unwrap();
    let header_len = header.header_length as usize;
    let positions = [
        14usize,
        header_len.saturating_sub(1),
        header_len,
        sealed.len().saturating_sub(1),
    ];

    for position in positions {
        let mut tampered = sealed.clone();
        tampered[position] ^= 1;
        assert!(open(
            &tampered,
            &provider(),
            &codec,
            &open_options(DNT_CONTENT_OCTET_STREAM),
        )
        .is_err());
    }
}

#[test]
fn every_envelope_byte_is_integrity_sensitive_and_concatenation_fails() {
    let codec = BytesCodec;
    let sealed = seal(
        b"byte-wise tamper coverage",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();

    for position in 0..sealed.len() {
        let mut tampered = sealed.clone();
        tampered[position] ^= 1;
        assert!(open(
            &tampered,
            &provider(),
            &codec,
            &open_options(DNT_CONTENT_OCTET_STREAM),
        )
        .is_err());
    }

    let mut concatenated = sealed;
    concatenated.extend_from_slice(b"trailing-data");
    assert!(open(
        &concatenated,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM),
    )
    .is_err());
}

#[test]
fn generated_round_trip_rekey_and_tamper_invariants_hold() {
    let codec = BytesCodec;
    let lengths = [0, 1, 2, 15, 16, 17, 63, 64, 255, 256, 1_023, 4_096, 65_535];
    let mut state = 0x5a17_d3c4_91ef_280bu64;

    for length in lengths {
        let mut payload = Vec::with_capacity(length);
        for _ in 0..length {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            payload.push(state as u8);
        }
        for compact in [false, true] {
            let options = if compact {
                seal_options(DNT_CONTENT_OCTET_STREAM).compact_payload()
            } else {
                seal_options(DNT_CONTENT_OCTET_STREAM)
            };
            let sealed = seal(&payload, &provider(), &codec, options).unwrap();
            let opened = open_owned(
                sealed.clone(),
                &provider(),
                &codec,
                &open_options(DNT_CONTENT_OCTET_STREAM),
            )
            .unwrap();
            assert_eq!(opened.payload, payload);

            let rotated = rekey(
                &sealed,
                &provider(),
                &codec,
                &open_options(DNT_CONTENT_OCTET_STREAM),
                key_id("key-b"),
            )
            .unwrap();
            let reopened = open_owned(
                rotated,
                &provider(),
                &codec,
                &open_options(DNT_CONTENT_OCTET_STREAM),
            )
            .unwrap();
            assert_eq!(reopened.payload, payload);

            let mut tampered = sealed;
            let position = (state as usize) % tampered.len();
            tampered[position] ^= 0x80;
            assert!(open_owned(
                tampered,
                &provider(),
                &codec,
                &open_options(DNT_CONTENT_OCTET_STREAM),
            )
            .is_err());
        }
    }
}

#[test]
fn generated_arbitrary_inputs_fail_without_panicking_or_unbounded_open() {
    let codec = BytesCodec;
    let options = open_options(DNT_CONTENT_OCTET_STREAM);
    let keys = provider();
    let mut state = 0xd1b5_4a32_d192_ed03u64;

    for case in 0..512usize {
        let length = case.saturating_mul(37) % 4_097;
        let mut input = Vec::with_capacity(length);
        for _ in 0..length {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            input.push(state as u8);
        }
        let _ = inspect_header(&input);
        let _ = open_owned(input, &keys, &codec, &options);
    }
}

#[test]
fn manually_invalid_header_flags_cannot_trigger_a_panic() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    let mut header = inspect_header(&sealed).unwrap();
    header.flags = 0x0000_0002;

    assert_eq!(header.compression(), DntCompression::None);
}

#[test]
fn encrypted_metadata_and_file_reads_are_bounded() {
    let codec = BytesCodec;
    let mut oversized_metadata = seal_options(DNT_CONTENT_OCTET_STREAM);
    oversized_metadata.encrypted_metadata = vec![0; DNT_MAX_ENCRYPTED_METADATA_BYTES + 1];
    assert_eq!(
        seal(b"payload", &provider(), &codec, oversized_metadata),
        Err(DntError::PayloadTooLarge)
    );

    let path = temp_path("bounded-read.dnt");
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    std::fs::write(&path, sealed).unwrap();
    let mut unbounded = open_options(DNT_CONTENT_OCTET_STREAM);
    unbounded.max_payload_bytes = None;
    assert_eq!(
        read_verified(&path, &provider(), &codec, &unbounded),
        Err(DntError::PayloadTooLarge)
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn nonce_tamper_fails_authentication() {
    let codec = BytesCodec;
    let mut sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    let nonce_offset = DNT_MAGIC.len() + 2 + 4 + 4 + 2 + 4 + 8 + 8;
    sealed[nonce_offset] ^= 1;

    assert_eq!(
        open(
            &sealed,
            &provider(),
            &codec,
            &open_options(DNT_CONTENT_OCTET_STREAM)
        ),
        Err(DntError::AuthenticationFailed)
    );
}

#[test]
fn rekey_rotates_key_id_and_preserves_payload() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    let rotated = rekey(
        &sealed,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM),
        key_id("key-b"),
    )
    .unwrap();

    let header = inspect_header(&rotated).unwrap();
    assert_eq!(header.key_id.as_str(), "key-b");
    let opened = open(
        &rotated,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    assert_eq!(opened.payload, b"payload");
}

#[test]
fn rekey_preserves_compact_payload_flag() {
    let codec = BytesCodec;
    let payload = b"repeat-repeat-repeat-repeat-repeat-repeat-repeat-repeat";
    let sealed = seal(
        payload,
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM).compact_payload(),
    )
    .unwrap();
    let rotated = rekey(
        &sealed,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM),
        key_id("key-b"),
    )
    .unwrap();

    let header = inspect_header(&rotated).unwrap();
    assert_eq!(header.key_id.as_str(), "key-b");
    assert_eq!(header.compression(), DntCompression::Deflate);
    assert_eq!(
        open(
            &rotated,
            &provider(),
            &codec,
            &open_options(DNT_CONTENT_OCTET_STREAM),
        )
        .unwrap()
        .payload,
        payload
    );
}

#[test]
fn max_payload_is_enforced() {
    let codec = BytesCodec;
    let mut options = seal_options(DNT_CONTENT_OCTET_STREAM);
    options.max_payload_bytes = Some(3);

    assert_eq!(
        seal(b"payload", &provider(), &codec, options),
        Err(DntError::PayloadTooLarge)
    );
}

#[test]
fn compact_payload_expansion_limit_is_enforced_on_open_and_verify() {
    let codec = BytesCodec;
    let payload = vec![b'a'; 2048];
    let sealed = seal(
        &payload,
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM).compact_payload(),
    )
    .unwrap();
    let options = DntOpenOptions {
        application_id: app_id(),
        tenant_id: Some(tenant_id()),
        content_type: ContentType::new(DNT_CONTENT_OCTET_STREAM).unwrap(),
        max_payload_bytes: Some(128),
    };

    assert_eq!(
        open(&sealed, &provider(), &codec, &options),
        Err(DntError::PayloadTooLarge)
    );
    assert_eq!(
        verify(&sealed, &provider(), &codec, &options),
        Err(DntError::PayloadTooLarge)
    );
}

#[test]
fn compact_payload_requires_open_payload_bound() {
    let codec = BytesCodec;
    let sealed = seal(
        b"payload payload payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM).compact_payload(),
    )
    .unwrap();
    let options = DntOpenOptions {
        application_id: app_id(),
        tenant_id: Some(tenant_id()),
        content_type: ContentType::new(DNT_CONTENT_OCTET_STREAM).unwrap(),
        max_payload_bytes: None,
    };

    assert_eq!(
        open(&sealed, &provider(), &codec, &options),
        Err(DntError::PayloadTooLarge)
    );
}

#[test]
fn atomic_write_and_read_verified_round_trip() {
    let codec = BytesCodec;
    let path = temp_path("atomic.dnt");
    let result = write_atomic(
        &path,
        b"payload",
        &provider(),
        &codec,
        seal_options(DNT_CONTENT_OCTET_STREAM),
        &open_options(DNT_CONTENT_OCTET_STREAM),
    );
    assert_eq!(result, Ok(()));

    let opened = read_verified(
        &path,
        &provider(),
        &codec,
        &open_options(DNT_CONTENT_OCTET_STREAM),
    )
    .unwrap();
    assert_eq!(opened.payload, b"payload");
    let _ = std::fs::remove_file(path);
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("appcore-dnt-{}-{name}", std::process::id()))
}
