// =============================================================================
//        #######
//     ###       ###     F: auth.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bearer-token authorization for HTTP command and query ingress.

use crate::command_contract::{CommandRequest, CommandResponse};
use crate::query_contract::{QueryRequest, QueryResponse};
use appcore_security::CommandTokenError;
pub use appcore_security::RequestValidationDetails;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use std::sync::Arc;

use super::response::{command_forbidden, command_unauthorized};

#[derive(Clone)]
/// Bearer-token requirements and verifier for HTTP Runtime routes.
pub struct HttpCommandAuth {
    /// Whether command and query endpoints require a bearer token.
    pub require_token: bool,
    /// Whether unauthenticated callers may read the reduced status response.
    pub public_status: bool,
    /// Token verifier used when authentication is required.
    pub verifier: Option<Arc<dyn CommandTokenVerifier>>,
}

impl Default for HttpCommandAuth {
    fn default() -> Self {
        Self {
            require_token: true,
            public_status: false,
            verifier: None,
        }
    }
}

impl HttpCommandAuth {
    /// Explicitly disables command and query authentication for local tests.
    ///
    /// Production hosts should use [`Self::default`] or provide a verifier.
    pub fn insecure_local_for_testing() -> Self {
        Self {
            require_token: false,
            public_status: false,
            verifier: None,
        }
    }
}

/// Verifies command- and query-scoped bearer tokens.
pub trait CommandTokenVerifier: Send + Sync {
    /// Verifies a token for a command name.
    fn verify_command_token(
        &self,
        token: &str,
        command_name: &str,
    ) -> Result<(), CommandTokenError>;
    /// Verifies a token for a query name.
    fn verify_query_token(&self, token: &str, query_name: &str) -> Result<(), CommandTokenError>;

    /// Verifies a command token with optional request-bound validation details.
    fn verify_command_token_with_request(
        &self,
        token: &str,
        command_name: &str,
        _details: Option<&RequestValidationDetails>,
    ) -> Result<(), CommandTokenError> {
        self.verify_command_token(token, command_name)
    }

    /// Verifies a query token with optional request-bound validation details.
    fn verify_query_token_with_request(
        &self,
        token: &str,
        query_name: &str,
        _details: Option<&RequestValidationDetails>,
    ) -> Result<(), CommandTokenError> {
        self.verify_query_token(token, query_name)
    }
}

pub(crate) fn authorize_command(
    auth: &HttpCommandAuth,
    headers: &HeaderMap,
    request: &CommandRequest,
) -> Option<(StatusCode, Json<CommandResponse>)> {
    if !auth.require_token {
        return None;
    }
    let token = match extract_bearer_token(headers) {
        Some(token) => token,
        None => return Some(command_unauthorized("missing bearer token")),
    };
    let Some(verifier) = &auth.verifier else {
        return Some(command_unauthorized("token verifier not configured"));
    };
    let details = RequestValidationDetails {
        purpose: "command".to_string(),
        name: request.command_name.clone(),
        id: request.command_id.clone(),
        idempotency_key: request.idempotency_key.clone(),
        payload: request.payload.clone(),
        subject: None,
        audience: None,
    };
    match verifier.verify_command_token_with_request(token, &request.command_name, Some(&details)) {
        Ok(()) => None,
        Err(CommandTokenError::Forbidden) => {
            Some(command_forbidden("command not allowed for token"))
        }
        Err(CommandTokenError::InvalidFormat | CommandTokenError::Unauthorized) => {
            Some(command_unauthorized("invalid bearer token"))
        }
    }
}

pub(crate) fn authorize_query(
    auth: &HttpCommandAuth,
    headers: &HeaderMap,
    request: &QueryRequest,
) -> Option<(StatusCode, Json<QueryResponse>)> {
    if !auth.require_token {
        return None;
    }
    let token = match extract_bearer_token(headers) {
        Some(token) => token,
        None => {
            return Some((
                StatusCode::UNAUTHORIZED,
                Json(QueryResponse::rejected("missing bearer token")),
            ))
        }
    };
    let Some(verifier) = &auth.verifier else {
        return Some((
            StatusCode::UNAUTHORIZED,
            Json(QueryResponse::rejected("token verifier not configured")),
        ));
    };
    let details = RequestValidationDetails {
        purpose: "query".to_string(),
        name: request.query_name.clone(),
        id: request.query_id.clone(),
        idempotency_key: None,
        payload: serde_json::to_string(&request.payload).unwrap_or_default(),
        subject: None,
        audience: None,
    };
    match verifier.verify_query_token_with_request(token, &request.query_name, Some(&details)) {
        Ok(()) => None,
        Err(CommandTokenError::Forbidden) => Some((
            StatusCode::FORBIDDEN,
            Json(QueryResponse::rejected("query not allowed for token")),
        )),
        Err(_) => Some((
            StatusCode::UNAUTHORIZED,
            Json(QueryResponse::rejected("invalid bearer token")),
        )),
    }
}

pub(crate) fn authorize_status(
    auth: &HttpCommandAuth,
    headers: &HeaderMap,
) -> Result<bool, StatusCode> {
    let token_opt = extract_bearer_token(headers);
    if !auth.public_status {
        let Some(token) = token_opt else {
            return Err(StatusCode::UNAUTHORIZED);
        };
        let Some(verifier) = &auth.verifier else {
            return Err(StatusCode::UNAUTHORIZED);
        };
        match verifier.verify_query_token_with_request(token, "runtime.status", None) {
            Ok(()) => Ok(true),
            Err(CommandTokenError::Forbidden) => Err(StatusCode::FORBIDDEN),
            Err(_) => Err(StatusCode::UNAUTHORIZED),
        }
    } else {
        let Some(token) = token_opt else {
            return Ok(false);
        };
        let Some(verifier) = &auth.verifier else {
            return Ok(false);
        };
        match verifier.verify_query_token_with_request(token, "runtime.status", None) {
            Ok(()) => Ok(true),
            _ => Ok(false),
        }
    }
}

pub(crate) fn authorize_private_status(
    auth: &HttpCommandAuth,
    headers: &HeaderMap,
    query_name: &str,
) -> Result<(), StatusCode> {
    let Some(token) = extract_bearer_token(headers) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let Some(verifier) = &auth.verifier else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    match verifier.verify_query_token_with_request(token, query_name, None) {
        Ok(()) => Ok(()),
        Err(CommandTokenError::Forbidden) => Err(StatusCode::FORBIDDEN),
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    if headers.get_all("authorization").iter().count() != 1 {
        return None;
    }
    let auth = headers.get("authorization")?;
    let auth = auth.to_str().ok()?;
    let token = auth.strip_prefix("Bearer ")?;
    if token.is_empty() {
        return None;
    }
    Some(token)
}
