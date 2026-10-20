//! Exchanging ID tokens for granary tokens.

use core::error::Error;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse as _, Response};
use axum::routing::post;
use axum::{Json, Router};

use super::bearer;
use crate::exchange::{Exchange, ExchangeError};
use crate::oidc::VerifyError;

impl ExchangeError {
    /// The status to respond with.
    const fn status(&self) -> StatusCode {
        match self {
            Self::Malformed(_)
            | Self::UnknownIssuer(_)
            | Self::Verify(
                VerifyError::Algorithm(_)
                | VerifyError::Invalid(_)
                | VerifyError::Key(_)
                | VerifyError::MissingKeyId
                | VerifyError::Rejected
                | VerifyError::UnknownKey(_),
            ) => StatusCode::UNAUTHORIZED,
            Self::NoTier { sub: _ } => StatusCode::FORBIDDEN,
            Self::Verify(VerifyError::Fetch { url: _, source: _ }) => StatusCode::BAD_GATEWAY,
            Self::Sign(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Builds the router for the token exchange.
pub(super) fn router(exchange: Arc<Exchange>) -> Router {
    Router::new()
        .route("/token", post(issue))
        .with_state(exchange)
}

/// Exchanges the ID token in the `Authorization` header.
async fn issue(State(exchange): State<Arc<Exchange>>, headers: HeaderMap) -> Response {
    let Some(id_token) = bearer(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match exchange.exchange(id_token).await {
        Ok(issued) => Json(issued).into_response(),
        Err(err) => {
            let status = err.status();
            if status.is_server_error() {
                tracing::error!(error = &err as &dyn Error, "failed to exchange a token");
            } else {
                tracing::info!(error = &err as &dyn Error, "rejected a token exchange");
            }
            status.into_response()
        }
    }
}
