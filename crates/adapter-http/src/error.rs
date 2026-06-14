//! Erreur HTTP : traduit les erreurs métier en réponses JSON cohérentes.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use carbonfr_core::application::ApplicationError;
use serde::Serialize;

/// Erreur prête à être renvoyée au client : statut + corps JSON `{error, message}`.
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: message.into(),
        }
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "no_data",
            message: message.into(),
        }
    }

    /// Erreur serveur générique : ne divulgue aucun détail interne au client.
    pub(crate) fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "erreur interne".to_string(),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: self.code,
            message: self.message,
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::NotFound(region) => Self::not_found(format!(
                "aucune donnée disponible pour la région {}",
                region.slug()
            )),
            // Erreurs de ports (base, source, prévision) ou série insuffisante :
            // côté serveur, on ne détaille pas au client.
            _ => Self::internal(),
        }
    }
}

impl From<time::error::Format> for ApiError {
    fn from(_: time::error::Format) -> Self {
        Self::internal()
    }
}
