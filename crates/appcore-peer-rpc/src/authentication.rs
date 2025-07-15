// =============================================================================
//        #######
//     ###       ###     F: authentication.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Issues short-lived credentials bound to peer requests.
pub trait PeerRpcTokenIssuer: Send + Sync {
    /// Issues a token for a request identifier and optional request hash.
    fn issue_peer_token(
        &self,
        request_id: &str,
        request_hash: Option<&str>,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<String, PeerRpcError>;
}

/// Token issuer backed by AppCore's signed local token provider.
#[derive(Debug, Clone)]
pub struct HashTokenPeerTokenIssuer<P = HashTokenProvider> {
    provider: P,
    claims: TokenClaims,
}

/// Token issuer for explicitly configured static credentials.
///
/// The credential is zeroized on drop and redacted from debug output.
#[cfg(any(test, feature = "insecure-testing"))]
#[derive(Clone)]
pub struct StaticPeerRpcTokenIssuer {
    token: Vec<u8>,
}

#[cfg(any(test, feature = "insecure-testing"))]
impl std::fmt::Debug for StaticPeerRpcTokenIssuer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StaticPeerRpcTokenIssuer")
            .field("token", &"REDACTED")
            .finish()
    }
}

#[cfg(any(test, feature = "insecure-testing"))]
impl Drop for StaticPeerRpcTokenIssuer {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.token.zeroize();
    }
}

/// Host integration contract for application-owned peer query and command handling.
pub trait PeerRpcDispatcher: Send + Sync {
    /// Dispatches a validated peer query envelope.
    fn dispatch_peer_query(
        &self,
        envelope: PeerRpcEnvelope,
    ) -> Result<PeerRpcResponse, PeerRpcError>;

    /// Dispatches a validated peer command envelope.
    fn dispatch_peer_command(
        &self,
        envelope: PeerRpcEnvelope,
    ) -> Result<PeerRpcResponse, PeerRpcError>;
}

/// Validates credentials supplied to peer RPC endpoints.
pub trait PeerRpcAuthenticator: Send + Sync {
    /// Authenticates a token and optionally binds it to the expected request hash.
    fn authenticate(
        &self,
        token: Option<&str>,
        expected_request_hash: Option<&str>,
        now_ms: u64,
    ) -> Result<(), PeerRpcError>;
}

/// Test-only authenticator that accepts every request.
#[cfg(any(test, feature = "insecure-testing"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowPeerAuthenticator;

#[cfg(any(test, feature = "insecure-testing"))]
impl PeerRpcAuthenticator for AllowPeerAuthenticator {
    fn authenticate(
        &self,
        _token: Option<&str>,
        _expected_request_hash: Option<&str>,
        _now_ms: u64,
    ) -> Result<(), PeerRpcError> {
        Ok(())
    }
}

/// Peer authenticator backed by AppCore's signed local token provider.
#[derive(Debug, Clone)]
pub struct HashTokenPeerAuthenticator<P = HashTokenProvider> {
    provider: P,
    claims: TokenClaims,
}

impl<P> HashTokenPeerAuthenticator<P>
where
    P: TokenProvider,
{
    /// Creates an authenticator and scopes its claims to peer RPC.
    pub fn new(provider: P, mut claims: TokenClaims) -> Self {
        claims.salt = "peer".to_string();
        Self { provider, claims }
    }
}

impl<P> PeerRpcAuthenticator for HashTokenPeerAuthenticator<P>
where
    P: TokenProvider + Send + Sync,
{
    fn authenticate(
        &self,
        token: Option<&str>,
        expected_request_hash: Option<&str>,
        now_ms: u64,
    ) -> Result<(), PeerRpcError> {
        let token = token.ok_or(PeerRpcError::Unauthorized)?;
        let token = token
            .strip_prefix("Bearer ")
            .or_else(|| token.strip_prefix("bearer "))
            .unwrap_or(token);
        let claims = CommandTokenValidator::new(&self.provider, self.claims.clone())
            .validate_and_get_claims(token, "peer", None, now_ms, expected_request_hash)
            .map_err(peer_auth_error)?;
        if expected_request_hash.is_some() && claims.request_hash.is_none() {
            return Err(PeerRpcError::Forbidden);
        }
        Ok(())
    }
}

impl<P> HashTokenPeerTokenIssuer<P>
where
    P: TokenProvider,
{
    /// Creates an issuer and scopes its claims to peer RPC.
    pub fn new(provider: P, mut claims: TokenClaims) -> Self {
        claims.salt = "peer".to_string();
        Self { provider, claims }
    }
}

impl<P> PeerRpcTokenIssuer for HashTokenPeerTokenIssuer<P>
where
    P: TokenProvider + Send + Sync,
{
    fn issue_peer_token(
        &self,
        request_id: &str,
        request_hash: Option<&str>,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<String, PeerRpcError> {
        CommandTokenFactory::new(&self.provider, self.claims.clone())
            .create_v1_with_jti_and_hash(
                "peer",
                None,
                Some("*"),
                Some(LOCAL_ADMIN_SUBJECT),
                now_ms,
                ttl_ms,
                Some(request_id.to_string()),
                request_hash.map(ToOwned::to_owned),
            )
            .map_err(peer_auth_error)
    }
}

#[cfg(any(test, feature = "insecure-testing"))]
impl StaticPeerRpcTokenIssuer {
    /// Creates an issuer from an explicitly configured static token.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into().into_bytes(),
        }
    }
}

#[cfg(any(test, feature = "insecure-testing"))]
impl PeerRpcTokenIssuer for StaticPeerRpcTokenIssuer {
    fn issue_peer_token(
        &self,
        _request_id: &str,
        _request_hash: Option<&str>,
        _now_ms: u64,
        _ttl_ms: u64,
    ) -> Result<String, PeerRpcError> {
        String::from_utf8(self.token.clone()).map_err(|_| PeerRpcError::Unauthorized)
    }
}
fn peer_auth_error(error: CommandTokenError) -> PeerRpcError {
    match error {
        CommandTokenError::Forbidden => PeerRpcError::Forbidden,
        CommandTokenError::InvalidFormat | CommandTokenError::Unauthorized => {
            PeerRpcError::Unauthorized
        }
    }
}
