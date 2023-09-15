// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT authenticated encrypted binary container.
//!
//! DNT is payload-agnostic. The file extension is never trusted as format
//! identity; callers inspect and verify the authenticated binary header.

#![deny(missing_docs)]

mod cipher;
mod codec;
mod compression;
mod crypto;
mod error;
mod flags;
mod header;
mod ids;
mod io;
mod migration;
mod model;
mod model_types;
mod plaintext;

pub use codec::{BytesCodec, DntCodec, IdentityJsonCodec};
pub use compression::DntCompression;
pub use crypto::{DntKeyProvider, SecretKey, StaticDntKeyProvider};
pub use error::{CodecError, DntError, DntKeyError, DntResult};
pub use flags::{
    dnt_compose_flags, dnt_user_flag, DntFlags, DNT_FLAG_PAYLOAD_DEFLATE, DNT_INTERNAL_FLAG_MASK,
    DNT_USER_FLAG_COUNT, DNT_USER_FLAG_MASK, DNT_USER_FLAG_OFFSET,
};
pub use header::{
    inspect_header, DntAlgorithm, DntHeader, DNT_CONTENT_BACKUP, DNT_CONTENT_JSON,
    DNT_CONTENT_OCTET_STREAM, DNT_CONTENT_SECRET, DNT_CONTENT_SNAPSHOT, DNT_CONTENT_SYNC_EVENT,
    DNT_ENVELOPE_VERSION_V1, DNT_MAGIC, DNT_MAX_ENCRYPTED_METADATA_BYTES, DNT_MAX_HEADER_BYTES,
};
pub use ids::{CodecId, ContentType, KeyId};
pub use io::{read_verified, write_atomic};
pub use migration::{migrate_envelope, rekey};
pub use model::{open, open_owned, seal, verify};
pub use model_types::{DntContext, DntOpenOptions, DntSealOptions, OpenedDnt, VerifiedDnt};

#[cfg(test)]
mod tests;
