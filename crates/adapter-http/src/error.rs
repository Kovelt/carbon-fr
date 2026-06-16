//! Erreur HTTP : traduit les erreurs métier en réponses JSON cohérentes.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use carbonfr_core::application::ApplicationError;
use carbonfr_core::ports::{ForecastError, RepositoryError};
use serde::Serialize;
use utoipa::ToSchema;

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

    pub(crate) fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
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

/// Corps JSON d'une erreur de l'API.
#[derive(Serialize, ToSchema)]
pub(crate) struct ErrorBody {
    /// Code court et stable (`no_data`, `bad_request`, `internal`).
    pub error: &'static str,
    /// Message lisible.
    pub message: String,
}

impl ErrorBody {
    pub(crate) fn new(error: &'static str, message: impl Into<String>) -> Self {
        Self {
            error,
            message: message.into(),
        }
    }
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
            // Pas assez d'historique pour prévoir, ou série trop courte pour
            // dégager un créneau : ce n'est pas une panne serveur mais une
            // absence de donnée exploitable → 404.
            ApplicationError::Forecast(ForecastError::NotEnoughData) => {
                Self::not_found("historique insuffisant pour établir une prévision")
            }
            ApplicationError::InsufficientSeries => {
                Self::not_found("série insuffisante pour déterminer un créneau bas-carbone")
            }
            // Autres erreurs de ports (base, source, prévision indisponible) :
            // côté serveur, on ne détaille pas au client.
            _ => Self::internal(),
        }
    }
}

impl From<ForecastError> for ApiError {
    fn from(error: ForecastError) -> Self {
        match error {
            ForecastError::NotEnoughData => {
                Self::not_found("historique insuffisant pour établir une prévision")
            }
            ForecastError::Unavailable(_) => Self::internal(),
        }
    }
}

impl From<time::error::Format> for ApiError {
    fn from(_: time::error::Format) -> Self {
        Self::internal()
    }
}

impl From<RepositoryError> for ApiError {
    fn from(_: RepositoryError) -> Self {
        Self::internal()
    }
}
