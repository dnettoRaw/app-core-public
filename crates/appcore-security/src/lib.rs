// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 00:04:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Security contracts for auth, policy, token trust, and secret boundaries.

#![deny(missing_docs)]

pub mod auth;
pub mod dnt;
pub mod hashtoken;
pub mod policy;
pub mod redaction;
mod request_hash;
pub mod secret;
mod secret_file;
mod secret_keyring;
#[cfg(windows)]
mod secret_keyring_windows;
pub mod token;
pub mod vault;

pub use auth::{AuthContext, AuthDecision, Authenticator};
pub use dnt::{DntSecretKeyProvider, DntSecretRefPolicy};
pub use hashtoken::HashTokenProvider;
pub use policy::{PolicyCheck, PolicyDecision};
pub use redaction::redact_text;
pub use secret::{
    format_secret_material, new_rotated_secret, parse_secret_material, EnvSecretResolver,
    PeerCredential, PeerCredentialProvider, SecretBytes, SecretFormatError, SecretResolver,
    SecretStore, SecuritySecretMaterial, SecuritySecretMetadata, SecuritySecretRef,
    SecuritySecretStatus, StaticPeerCredentialProvider, StaticSecretResolver,
};
pub use secret_file::FileSecretResolver;
#[cfg(windows)]
pub use secret_keyring::WINDOWS_DPAPI_USER_SECRET_KEYRING_FORMAT;
pub use secret_keyring::{
    FileSecretKeyring, SecretAccessError, SecretAccessResult, FILE_SECRET_KEYRING_FORMAT,
};
#[cfg(windows)]
pub use secret_keyring_windows::WindowsDpapiSecretKeyring;
pub use token::{
    compute_request_hash, CommandTokenError, CommandTokenFactory, CommandTokenValidator,
    RequestValidationDetails, RuntimeTokenClaims, SecurityError, SecurityResult, TokenClaims,
    TokenProvider, DEFAULT_RUNTIME_TOKEN_TTL_MS, LOCAL_ADMIN_SUBJECT,
};
pub use vault::{Vault, VaultState};
