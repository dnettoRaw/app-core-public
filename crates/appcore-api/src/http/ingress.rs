// =============================================================================
//        #######
//     ###       ###     F: ingress.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/05 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Admission before body collection and JSON decoding; health stays independent.

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, FromRequest, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub(super) struct Ingress {
    slots: Arc<Semaphore>,
    max_bytes: usize,
    receive_deadline: Duration,
}

impl Ingress {
    pub(super) fn new(max_bytes: usize) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(16)),
            max_bytes,
            receive_deadline: Duration::from_secs(10),
        }
    }
}

pub(super) async fn admit(
    State(ingress): State<Ingress>,
    request: Request,
    next: Next,
) -> Response {
    // Fail immediately: waiting requests must not form an unbounded queue.
    let Ok(_permit) = ingress.slots.try_acquire() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let (parts, body) = request.into_parts();
    let mut body_request = Request::new(body);
    DefaultBodyLimit::max(ingress.max_bytes).apply(&mut body_request);
    let read = async {
        Bytes::from_request(body_request, &())
            .await
            .map_err(|error| error.status())
    };
    let collected = receive(read, ingress.receive_deadline).await;
    match collected {
        Ok(bytes) => {
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await
        }
        Err(status) => status.into_response(),
    }
}

async fn receive<T>(
    read: impl std::future::Future<Output = Result<T, StatusCode>>,
    deadline: Duration,
) -> Result<T, StatusCode> {
    match tokio::time::timeout(deadline, read).await {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(status)) => Err(status),
        Err(_) => Err(StatusCode::REQUEST_TIMEOUT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{middleware, routing::post, Json, Router};
    use tower::ServiceExt;

    #[test]
    fn slow_tcp_body_times_out_and_releases_ingress_capacity() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let ingress = Ingress {
                    slots: Arc::new(Semaphore::new(16)),
                    max_bytes: 8,
                    receive_deadline: Duration::from_millis(25),
                };
                let router = Router::new()
                    .route("/", post(|| async { StatusCode::OK }))
                    .layer(middleware::from_fn_with_state(ingress.clone(), admit));
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let server = tokio::spawn(async move { axum::serve(listener, router).await });
                let client = tokio::net::TcpStream::connect(address).await.unwrap();
                let request = b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 8\r\n\r\n{";
                client.writable().await.unwrap();
                assert_eq!(client.try_write(request).unwrap(), request.len());
                let mut response = [0_u8; 512];
                let read = tokio::time::timeout(Duration::from_secs(1), async {
                    loop {
                        client.readable().await.unwrap();
                        match client.try_read(&mut response) {
                            Ok(read) => break read,
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(error) => panic!("TCP response read failed: {error}"),
                        }
                    }
                })
                .await
                .unwrap();
                assert!(String::from_utf8_lossy(&response[..read]).starts_with("HTTP/1.1 408"));
                assert_eq!(ingress.slots.available_permits(), 16);
                server.abort();
                assert!(server.await.unwrap_err().is_cancelled());
            });
    }

    #[test]
    fn cancellation_releases_ingress_and_health_bypasses_saturation() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let ingress = Ingress::new(8);
                let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let observed = Arc::clone(&entered);
                let slots = Arc::clone(&ingress.slots);
                let router = Router::new()
                    .route(
                        "/",
                        post(move || {
                            let entered = Arc::clone(&entered);
                            let slots = Arc::clone(&slots);
                            async move {
                                assert_eq!(slots.available_permits(), 15);
                                entered.store(true, std::sync::atomic::Ordering::SeqCst);
                                std::future::pending::<StatusCode>().await
                            }
                        }),
                    )
                    .route_layer(middleware::from_fn_with_state(ingress.clone(), admit))
                    .route(
                        "/v1/health",
                        axum::routing::get(|| async { StatusCode::OK }),
                    );
                let held = ingress.slots.acquire_many(16).await.unwrap();
                let health = Request::builder()
                    .uri("/v1/health")
                    .body(Body::empty())
                    .unwrap();
                assert_eq!(
                    router.clone().oneshot(health).await.unwrap().status(),
                    StatusCode::OK
                );
                drop(held);
                let request = Request::builder()
                    .method("POST")
                    .uri("/")
                    .body(Body::empty())
                    .unwrap();
                assert!(
                    tokio::time::timeout(Duration::from_millis(10), router.oneshot(request))
                        .await
                        .is_err()
                );
                assert!(observed.load(std::sync::atomic::Ordering::SeqCst));
                assert_eq!(ingress.slots.available_permits(), 16);
            });
    }

    #[test]
    fn admission_precedes_json_and_releases_capacity() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let ingress = Ingress::new(8);
                let router = Router::new()
                    .route(
                        "/",
                        post(|Json(_): Json<serde_json::Value>| async { StatusCode::OK }),
                    )
                    .layer(middleware::from_fn_with_state(ingress.clone(), admit));
                let permit = ingress.slots.acquire_many(16).await.unwrap();
                let request = || {
                    Request::builder()
                        .method("POST")
                        .uri("/")
                        .header("content-type", "application/json")
                        .body(Body::from("invalid json"))
                        .unwrap()
                };
                assert_eq!(
                    router.clone().oneshot(request()).await.unwrap().status(),
                    StatusCode::SERVICE_UNAVAILABLE
                );
                drop(permit);
                assert_eq!(
                    router.oneshot(request()).await.unwrap().status(),
                    StatusCode::PAYLOAD_TOO_LARGE
                );
                assert_eq!(ingress.slots.available_permits(), 16);
                let pending = std::future::pending::<Result<(), StatusCode>>();
                assert_eq!(
                    receive(pending, Duration::from_millis(1)).await,
                    Err(StatusCode::REQUEST_TIMEOUT)
                );
            });
    }
}
