// =============================================================================
//        #######
//     ###       ###     F: sensitive_diagnostics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Explicit sensitive output uses only an encrypted DNT sink.
//!
//! Set `APPCORE_LOG_DEMO_KEY_HEX` to a 64-character hexadecimal key before
//! running it. Real deployments obtain key material from their explicit
//! secret-management boundary instead of environment variables.

use appcore_contracts::ApplicationId;
use appcore_dnt::{KeyId, SecretKey, StaticDntKeyProvider};
use appcore_log::{
    LogDispatcher, LogPolicy, SensitiveDntSink, SensitiveDntSinkConfig, Sensitivity,
};
use std::sync::Arc;

fn main() -> Result<(), appcore_log::LogError> {
    let key_id = KeyId::new("example-log-key").map_err(|_| appcore_log::LogError::Encryption)?;

    let provider =
        StaticDntKeyProvider::new().with_key(key_id.clone(), SecretKey::new(demo_key()?));

    // The sink bounds the encrypted snapshot by both bytes and event count.
    let sink = SensitiveDntSink::new(
        SensitiveDntSinkConfig {
            path: std::env::temp_dir().join("appcore-sensitive-log.dnt"),
            application_id: ApplicationId::new("example-log")
                .map_err(|_| appcore_log::LogError::Encryption)?,
            key_id,
            max_bytes: 4096,
            max_events: 8,
            retention: 2,
        },
        provider,
    )?;

    // Sensitive mode must be explicit and has no console or JSONL fallback.
    let mut policy = LogPolicy::default();
    policy.sensitivity = Sensitivity::Sensitive;

    let log = LogDispatcher::new(policy, vec![Arc::new(sink)]);

    log.event(0, "security").verbosity(2).error("diagnostic");

    Ok(())
}

fn demo_key() -> Result<[u8; 32], appcore_log::LogError> {
    let value =
        std::env::var("APPCORE_LOG_DEMO_KEY_HEX").map_err(|_| appcore_log::LogError::Encryption)?;
    if value.len() != 64 || !value.is_ascii() {
        return Err(appcore_log::LogError::Encryption);
    }
    let mut key = [0_u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        let offset = index.saturating_mul(2);
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| appcore_log::LogError::Encryption)?;
    }
    Ok(key)
}
