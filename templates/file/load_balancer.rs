use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};

/// Headers that must never be forwarded verbatim: `host` is specific to
/// whichever hop received the request, and `content-length`/
/// `transfer-encoding` are recomputed by reqwest/axum from the actual body
/// being sent at each hop.
const HOP_SPECIFIC_HEADERS: [&str; 3] = ["host", "content-length", "transfer-encoding"];

fn strip_hop_specific_headers(headers: &mut HeaderMap) {
    for name in HOP_SPECIFIC_HEADERS {
        headers.remove(name);
    }
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

/// Bodies are streamed through in both directions — never buffered whole
/// in memory — so the proxy's footprint doesn't grow with payload size and
/// the backend starts receiving before the client has finished sending.
async fn proxy_handler(State(proxy): State<Arc<RoundRobinProxy>>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let url = format!("http://{}{}", proxy.next_target(), path_and_query);
    let mut headers = parts.headers;
    strip_hop_specific_headers(&mut headers);

    let sent = proxy
        .client
        .request(parts.method, url)
        .headers(headers)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await;

    match sent {
        Ok(mut backend_response) => {
            let status = backend_response.status();
            let mut headers = std::mem::take(backend_response.headers_mut());
            strip_hop_specific_headers(&mut headers);

            let mut response = Response::new(Body::from_stream(backend_response.bytes_stream()));
            *response.status_mut() = status;
            *response.headers_mut() = headers;
            response
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
