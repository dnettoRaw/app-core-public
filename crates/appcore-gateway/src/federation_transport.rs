// =============================================================================
//        #######
//     ###       ###     F: federation_transport.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Bounded blocking HTTP transport for explicit federation V2 requests.

use crate::{
    GatewayError, GatewayFederationRequestV2, GatewayFederationResponseV2, GatewayFederationUrl,
    GatewayResult, GATEWAY_FEDERATION_PATH_V2,
};
use appcore_transport::{
    HttpClient, HttpClientConfig, HttpHeader, HttpRequest, HttpTarget, TransportError,
};

const MAX_FEDERATION_RESPONSE_OVERHEAD_BYTES: usize = 65_536;

#[derive(Debug, Clone, Default)]
pub(crate) struct GatewayFederationTransport {
    client: HttpClient,
}

impl GatewayFederationTransport {
    pub(crate) fn send(
        &self,
        target_url: &GatewayFederationUrl,
        credential: &str,
        request: &GatewayFederationRequestV2,
    ) -> GatewayResult<GatewayFederationResponseV2> {
        request.validate()?;
        let body = serde_json::to_vec(request)
            .map_err(|_| transport_error("federation request encoding failed"))?;
        if body.len() > crate::config::MAX_GATEWAY_HTTP_BODY_BYTES {
            return Err(transport_error("federation request exceeds its bound"));
        }
        let target = HttpTarget::parse(target_url.expose(), GATEWAY_FEDERATION_PATH_V2)
            .map_err(map_transport_error)?;
        let http_request = HttpRequest::new("POST", body.clone())
            .map_err(map_transport_error)?
            .with_header(
                HttpHeader::new("Content-Type", "application/json").map_err(map_transport_error)?,
            )
            .with_header(
                HttpHeader::new("Accept", "application/json").map_err(map_transport_error)?,
            )
            .with_header(
                HttpHeader::sensitive("Authorization", format!("Bearer {credential}"))
                    .map_err(map_transport_error)?,
            );
        let response = self
            .client
            .send(
                &target,
                &http_request,
                HttpClientConfig {
                    timeout_ms: request.request.timeout_ms,
                    max_request_bytes: body.len(),
                    max_response_bytes: crate::config::MAX_GATEWAY_HTTP_BODY_BYTES
                        .saturating_add(MAX_FEDERATION_RESPONSE_OVERHEAD_BYTES),
                    max_header_bytes: 32_768,
                }
                .into(),
                None,
            )
            .map_err(map_transport_error)?;
        if !(200..300).contains(&response.status_code) {
            return Err(transport_error("federation endpoint rejected the exchange"));
        }
        let response = serde_json::from_slice::<GatewayFederationResponseV2>(&response.body)
            .map_err(|_| transport_error("federation response decoding failed"))?;
        response.validate_for_request(request)?;
        Ok(response)
    }
}

fn map_transport_error(error: TransportError) -> GatewayError {
    match error {
        TransportError::ResponseTooLarge { .. } | TransportError::RequestTooLarge { .. } => {
            transport_error("federation transport bound was exceeded")
        }
        TransportError::Timeout
        | TransportError::ConnectionRefused
        | TransportError::Dns(_)
        | TransportError::Cancelled => transport_error("federation endpoint is unavailable"),
        _ => transport_error("federation transport failed"),
    }
}

fn transport_error(message: &'static str) -> GatewayError {
    GatewayError::Transport(message.to_string())
}
