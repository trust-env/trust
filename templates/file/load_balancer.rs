use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::any,
    Router,
};

/// Headers that must never be forwarded verbatim: `host` is specific to
/// whichever hop received the request, and `content-length`/
/// `transfer-encoding` are recomputed by reqwest/axum from the actual body
/// being sent at each hop.
fn is_hop_specific_header(name: &str) -> bool {
    matches!(name, "host" | "content-length" | "transfer-encoding")
}

pub struct RoundRobinProxy {
    targets: Vec<String>,
    next: AtomicUsize,
    client: reqwest::Client,
}

impl RoundRobinProxy {
    pub fn new(targets: Vec<String>) -> Self {
        Self {
            targets,
            next: AtomicUsize::new(0),
            client: reqwest::Client::new(),
        }
    }

    fn next_target(&self) -> &str {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.targets.len();
        &self.targets[index]
    }
}

async fn proxy_handler(State(proxy): State<Arc<RoundRobinProxy>>, req: Request) -> Response {
    let target = proxy.next_target().to_string();
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .to_string();
    let url = format!("http://{}{}", target, path_and_query);
    let method = req.method().clone();
    let headers = req.headers().clone();

    let body_bytes = match to_bytes(req.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to read request body: {}", error),
            )
                .into_response();
        }
    };

    let mut builder = proxy.client.request(method, &url);
    for (name, value) in headers.iter() {
        if is_hop_specific_header(name.as_str()) {
            continue;
        }
        builder = builder.header(name, value);
    }

    match builder.body(body_bytes).send().await {
        Ok(backend_response) => {
            let status = backend_response.status();
            let headers = backend_response.headers().clone();
            let bytes = match backend_response.bytes().await {
                Ok(bytes) => bytes,
                Err(error) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        format!("Failed to read backend response: {}", error),
                    )
                        .into_response();
                }
            };

            let mut response = Response::builder().status(status);
            for (name, value) in headers.iter() {
                if is_hop_specific_header(name.as_str()) {
                    continue;
                }
                response = response.header(name, value);
            }
            response
                .body(Body::from(bytes))
                .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
        }
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            format!("Failed to reach backend: {}", error),
        )
            .into_response(),
    }
}

pub fn load_balancer_router(proxy: Arc<RoundRobinProxy>) -> Router {
    Router::new()
        .fallback(any(proxy_handler))
        .with_state(proxy)
}
