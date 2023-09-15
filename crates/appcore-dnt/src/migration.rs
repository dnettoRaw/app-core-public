// =============================================================================
//        #######
//     ###       ###     F: migration.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 12:07:11 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT key rotation and envelope migration operations.

use crate::{
    open, seal, DntCodec, DntKeyProvider, DntOpenOptions, DntResult, DntSealOptions, KeyId,
    OpenedDnt,
};
/// Opens and seals the same semantic payload under a new key identifier.
pub fn rekey<P, C>(
    input: &[u8],
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
    new_key_id: KeyId,
) -> DntResult<Vec<u8>>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    reseal(input, key_provider, codec, options, Some(new_key_id))
}

/// Migrates an envelope by opening and resealing it with the current writer.
pub fn migrate_envelope<P, C>(
    input: &[u8],
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<Vec<u8>>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    reseal(input, key_provider, codec, options, None)
}

fn reseal<P, C>(
    input: &[u8],
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
    new_key_id: Option<KeyId>,
) -> DntResult<Vec<u8>>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    let mut opened = open(input, key_provider, codec, options)?;
    let key_id = new_key_id.unwrap_or_else(|| opened.header.key_id.clone());
    let seal_options = options_from_opened(&opened, key_id, options.max_payload_bytes);
    let result = seal(&opened.payload, key_provider, codec, seal_options);
    opened.zeroize_plaintext();
    result
}

fn options_from_opened(
    opened: &OpenedDnt,
    key_id: KeyId,
    max_payload_bytes: Option<u64>,
) -> DntSealOptions {
    DntSealOptions {
        application_id: opened.header.application_id.clone(),
        tenant_id: opened.header.tenant_id.clone(),
        content_type: opened.header.content_type.clone(),
        schema_version: opened.header.schema_version,
        key_id,
        created_at_ms: opened.header.created_at_ms,
        public_metadata: opened.header.public_metadata.clone(),
        encrypted_metadata: opened.encrypted_metadata.clone(),
        flags: opened.header.flags,
        max_payload_bytes,
    }
}
