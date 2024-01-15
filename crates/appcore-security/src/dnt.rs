// =============================================================================
//        #######
//     ###       ###     F: dnt.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 00:04:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT key-provider adapter backed by security secret resolvers.

use crate::{SecretResolver, SecuritySecretRef};
use appcore_dnt::{DntContext, DntKeyError, DntKeyProvider, KeyId, SecretKey};
use std::fmt;

/// Mapping policy from DNT key IDs to security secret references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DntSecretRefPolicy {
    prefix: String,
}

impl DntSecretRefPolicy {
    /// Creates a mapping policy with an explicit non-secret reference prefix.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    /// Returns the default provider-owned DNT key reference policy.
    pub fn provider_default() -> Self {
        Self::new("provider:dnt-key/")
    }

    /// Maps a contextual key ID to an opaque security secret reference.
    pub fn reference_for(&self, key_id: &KeyId, context: &DntContext) -> SecuritySecretRef {
        let tenant = context
            .tenant_id
            .as_ref()
            .map(|value| value.as_str())
            .unwrap_or("_");
        SecuritySecretRef(format!(
            "{}{}/{}/{}",
            self.prefix,
            context.application_id.as_str(),
            tenant,
            key_id.as_str()
        ))
    }
}

impl Default for DntSecretRefPolicy {
    fn default() -> Self {
        Self::provider_default()
    }
}

/// DNT key provider backed by an existing AppCore secret resolver.
pub struct DntSecretKeyProvider<R> {
    resolver: R,
    policy: DntSecretRefPolicy,
}

impl<R> fmt::Debug for DntSecretKeyProvider<R> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DntSecretKeyProvider")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl<R> DntSecretKeyProvider<R>
where
    R: SecretResolver,
{
    /// Creates a provider using the default DNT key reference policy.
    pub fn new(resolver: R) -> Self {
        Self {
            resolver,
            policy: DntSecretRefPolicy::default(),
        }
    }

    /// Creates a provider using an explicit reference mapping policy.
    pub fn with_policy(resolver: R, policy: DntSecretRefPolicy) -> Self {
        Self { resolver, policy }
    }

    /// Returns the configured non-secret key-reference mapping policy.
    pub fn policy(&self) -> &DntSecretRefPolicy {
        &self.policy
    }
}

impl<R> DntKeyProvider for DntSecretKeyProvider<R>
where
    R: SecretResolver,
{
    fn resolve_key(&self, key_id: &KeyId, context: &DntContext) -> Result<SecretKey, DntKeyError> {
        let reference = self.policy.reference_for(key_id, context);
        let secret = self
            .resolver
            .resolve(&reference)
            .map_err(|_| DntKeyError::Unavailable)?;
        SecretKey::from_slice(secret.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SecretBytes, StaticSecretResolver};
    use appcore_contracts::ApplicationId;
    use appcore_dnt::{
        open, seal, BytesCodec, ContentType, DntOpenOptions, DntSealOptions, DNT_CONTENT_SECRET,
    };
    use appcore_types::TenantId;
    use std::collections::HashMap;

    #[test]
    fn secret_resolver_key_provider_opens_dnt_secret() {
        let key_id = KeyId::new("local-root").unwrap();
        let mut secrets = HashMap::new();
        secrets.insert(
            format!("provider:dnt-key/app-a/tenant-a/{}", key_id.as_str()),
            SecretBytes::new(vec![5; 32]),
        );
        let resolver = StaticSecretResolver::new(secrets);
        let provider = DntSecretKeyProvider::new(resolver);
        let codec = BytesCodec;
        let seal_options = DntSealOptions {
            application_id: ApplicationId::new("app-a").unwrap(),
            tenant_id: Some(TenantId::new("tenant-a").unwrap()),
            content_type: ContentType::new(DNT_CONTENT_SECRET).unwrap(),
            schema_version: 1,
            key_id,
            created_at_ms: 1,
            public_metadata: Vec::new(),
            encrypted_metadata: Vec::new(),
            flags: 0,
            max_payload_bytes: Some(1024),
        };
        let open_options = DntOpenOptions {
            application_id: ApplicationId::new("app-a").unwrap(),
            tenant_id: Some(TenantId::new("tenant-a").unwrap()),
            content_type: ContentType::new(DNT_CONTENT_SECRET).unwrap(),
            max_payload_bytes: Some(1024),
        };

        let sealed = seal(b"secret", &provider, &codec, seal_options).unwrap();
        let opened = open(&sealed, &provider, &codec, &open_options).unwrap();

        assert_eq!(opened.payload, b"secret");
    }

    #[test]
    fn missing_dnt_key_fails_closed() {
        let resolver = StaticSecretResolver::new(HashMap::new());
        let provider = DntSecretKeyProvider::new(resolver);
        let context = DntContext {
            application_id: ApplicationId::new("app-a").unwrap(),
            tenant_id: None,
            content_type: ContentType::new(DNT_CONTENT_SECRET).unwrap(),
            codec_id: appcore_dnt::CodecId::new("bytes").unwrap(),
            schema_version: 1,
        };

        assert_eq!(
            provider.resolve_key(&KeyId::new("missing").unwrap(), &context),
            Err(DntKeyError::Unavailable)
        );
    }
}
