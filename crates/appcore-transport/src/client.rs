// =============================================================================
//        #######
//     ###       ###     F: client.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0
// =============================================================================

//! Reusable and compatibility blocking HTTP clients.

use crate::connection::TransportConnection;
use crate::pool::{ConnectionPool, Origin};
use crate::wire;
use crate::{
    parse_response, CancellationToken, HttpClientConfig, HttpExchangeConfig, HttpPoolConfig,
    HttpRequest, HttpResponse, HttpScheme, HttpTarget, TransportError, TransportResult,
};
use std::sync::{Arc, OnceLock};

/// Reusable bounded blocking HTTP/1.1 client.
#[derive(Clone)]
pub struct HttpClient {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    pool: Arc<ConnectionPool>,
    tls: OnceLock<Result<native_tls::TlsConnector, String>>,
}

impl std::fmt::Debug for HttpClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("HttpClient").finish_non_exhaustive()
    }
}

impl HttpClient {
    /// Creates a client with explicit per-origin connection bounds.
    pub fn new(pool_config: HttpPoolConfig) -> TransportResult<Self> {
        validate_pool_config(pool_config)?;
        Ok(Self {
            inner: Arc::new(ClientInner {
                pool: ConnectionPool::new(pool_config),
                tls: OnceLock::new(),
            }),
        })
    }

    /// Sends one synchronous exchange and reuses only a fully drained connection.
    pub fn send(
        &self,
        target: &HttpTarget,
        request: &HttpRequest,
        config: HttpExchangeConfig,
        cancellation: Option<&CancellationToken>,
    ) -> TransportResult<HttpResponse> {
        validate_exchange(request, config, cancellation)?;
        let origin = Origin::from_target(target);
        let mut lease =
            self.inner
                .pool
                .acquire(origin, config.timeouts.connect_ms, cancellation)?;
        let mut connection = match lease.take() {
            Some(connection) => connection,
            None => TransportConnection::connect(
                target,
                config.timeouts,
                self.inner.tls_connector(target)?,
            )?,
        };
        connection.set_timeouts(config.timeouts)?;
        let raw = wire::exchange(&mut connection, target, request, config, cancellation, true)?;
        let response = parse_response(
            &raw.bytes,
            config.max_header_bytes,
            config.max_response_bytes,
        )?;
        if raw.reusable {
            lease.keep(connection);
        }
        Ok(response)
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self {
            inner: Arc::new(ClientInner {
                pool: ConnectionPool::new(HttpPoolConfig::default()),
                tls: OnceLock::new(),
            }),
        }
    }
}

impl ClientInner {
    fn tls_connector(
        &self,
        target: &HttpTarget,
    ) -> TransportResult<Option<&native_tls::TlsConnector>> {
        if target.scheme() != crate::HttpScheme::Https {
            return Ok(None);
        }
        self.tls
            .get_or_init(|| native_tls::TlsConnector::new().map_err(|error| error.to_string()))
            .as_ref()
            .map(Some)
            .map_err(|error| TransportError::Tls(error.clone()))
    }
}

/// Sends one bounded request through a compatibility one-shot client.
///
/// Existing synchronous consumers retain their V1 call shape. Consumers that
/// need keep-alive reuse should own one [`HttpClient`] and call
/// [`HttpClient::send`] for successive exchanges.
pub fn send(
    target: &HttpTarget,
    request: &HttpRequest,
    config: HttpClientConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<HttpResponse> {
    let config = HttpExchangeConfig::from(config);
    validate_exchange(request, config, cancellation)?;
    let connector = if target.scheme() == HttpScheme::Https {
        Some(
            native_tls::TlsConnector::new()
                .map_err(|error| TransportError::Tls(error.to_string()))?,
        )
    } else {
        None
    };
    let mut connection = TransportConnection::connect(target, config.timeouts, connector.as_ref())?;
    let raw = wire::exchange(
        &mut connection,
        target,
        request,
        config,
        cancellation,
        false,
    )?;
    parse_response(
        &raw.bytes,
        config.max_header_bytes,
        config.max_response_bytes,
    )
}

fn validate_pool_config(config: HttpPoolConfig) -> TransportResult<()> {
    if config.max_connections_per_origin == 0
        || config.max_idle_per_origin > config.max_connections_per_origin
        || config.max_origins == 0
        || config.idle_timeout_ms == 0
    {
        return Err(TransportError::InvalidRequest(
            "invalid HTTP pool configuration".to_string(),
        ));
    }
    Ok(())
}

fn validate_exchange(
    request: &HttpRequest,
    config: HttpExchangeConfig,
    cancellation: Option<&CancellationToken>,
) -> TransportResult<()> {
    if cancellation.is_some_and(CancellationToken::is_cancelled) {
        return Err(TransportError::Cancelled);
    }
    if request.body().len() > config.max_request_bytes {
        return Err(TransportError::RequestTooLarge {
            max: config.max_request_bytes,
        });
    }
    Ok(())
}
