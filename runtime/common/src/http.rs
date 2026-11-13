use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tokio::net::TcpListener;

/// An HTTP adapter: contributes its routes to the app's router.
pub trait HttpPresenter: Send + Sync + 'static {
    fn router(self: Arc<Self>) -> Router;
}

/// An error answered as `status` with a `{"message": "..."}` JSON body.
#[derive(Debug)]
pub struct HttpError {
    status: StatusCode,
    message: String,
}

impl HttpError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (self.status, Json(serde_json::json!({ "message": self.message }))).into_response()
    }
}

/// What a generated HTTP handler returns.
pub type HttpResult<T> = Result<Json<T>, HttpError>;

/// Binds `address` and serves `router` until the server stops.
pub async fn start_http_server(router: Router, address: String) -> Result<(), String> {
    let listener = TcpListener::bind(&address)
        .await
        .map_err(|e| format!("Failed to bind HTTP server to {}: {}", address, e))?;
    println!("HTTP server listening on: {}", address);
    axum::serve(listener, router)
        .await
        .map_err(|e| format!("HTTP server error: {}", e))
}
