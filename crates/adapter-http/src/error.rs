//! Erreur HTTP : traduit les erreurs métier en réponses **Problem Details**
//! (RFC 9457, `application/problem+json`) cohérentes.
//!
//! Le corps suit RFC 9457 (`type`/`title`/`status`/`detail`) et porte une
//! extension `code` : un identifiant court, **stable et machine-lisible**
//! (`no_data`, `bad_request`, …) sur lequel les clients s'alignent (le SDK lit
//! `code`). `type` reste `about:blank` : le `status` + `code` suffisent à
//! qualifier l'erreur, on n'expose pas d'URI à déréférencer (RFC 9457 §4.2.1).

use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use carbonfr_core::application::ApplicationError;
use carbonfr_core::ports::{ForecastError, RepositoryError};
use serde::Serialize;
use serde::de::DeserializeOwned;
use utoipa::ToSchema;

/// Type de média des Problem Details (RFC 9457 §3).
const PROBLEM_JSON: &str = "application/problem+json";

/// Erreur prête à être renvoyée au client : statut + titre + code stable.
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    title: &'static str,
    detail: String,
}

impl ApiError {
    pub(crate) fn bad_request(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            title: "Requête invalide",
            detail: detail.into(),
        }
    }

    pub(crate) fn not_found(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "no_data",
            title: "Donnée absente",
            detail: detail.into(),
        }
    }

    pub(crate) fn unauthorized(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            title: "Non autorisé",
            detail: detail.into(),
        }
    }

    /// Dépendance indisponible (503) — ex. base de données injoignable.
    pub(crate) fn unavailable(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "unavailable",
            title: "Service indisponible",
            detail: detail.into(),
        }
    }

    /// Erreur serveur générique : ne divulgue aucun détail interne au client.
    pub(crate) fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            title: "Erreur interne",
            detail: "erreur interne".to_string(),
        }
    }
}

/// Corps d'une erreur de l'API, au format **Problem Details** (RFC 9457).
#[derive(Serialize, ToSchema)]
pub(crate) struct ProblemDetails {
    /// Type de problème (URI). `about:blank` quand `status` + `code` suffisent
    /// (RFC 9457 §4.2.1) — pas d'URI à déréférencer.
    #[serde(rename = "type")]
    #[schema(rename = "type", example = "about:blank")]
    pub problem_type: &'static str,
    /// Résumé court et lisible du type de problème (stable par `code`).
    #[schema(example = "Donnée absente")]
    pub title: &'static str,
    /// Code de statut HTTP, répété dans le corps (RFC 9457 §3.1.2).
    #[schema(example = 404)]
    pub status: u16,
    /// Explication lisible spécifique à cette occurrence.
    #[schema(example = "aucune donnée disponible pour la région bretagne")]
    pub detail: String,
    /// Extension carbon-fr : code court **stable et machine-lisible**
    /// (`no_data`, `bad_request`, `unauthorized`, `unavailable`, `internal`,
    /// `rate_limited`). C'est la valeur sur laquelle un client doit s'aligner.
    #[schema(example = "no_data")]
    pub code: &'static str,
}

/// Construit une réponse `application/problem+json` (RFC 9457). Partagée par
/// [`ApiError`] et le middleware d'authentification (`auth.rs`) pour garantir un
/// corps d'erreur **uniforme** sur toute l'API.
pub(crate) fn problem_response(
    status: StatusCode,
    code: &'static str,
    title: &'static str,
    detail: impl Into<String>,
) -> Response {
    let body = ProblemDetails {
        problem_type: "about:blank",
        title,
        status: status.as_u16(),
        detail: detail.into(),
        code,
    };
    // `axum::Json` poserait `application/json` ; RFC 9457 impose
    // `application/problem+json`. On sérialise donc à la main avec le bon type.
    let payload = serde_json::to_vec(&body).unwrap_or_else(|_| {
        br#"{"type":"about:blank","title":"Erreur interne","status":500,"detail":"erreur interne","code":"internal"}"#.to_vec()
    });
    let mut response: Response =
        (status, [(header::CONTENT_TYPE, PROBLEM_JSON)], payload).into_response();
    // RFC 6750 §3 : un 401 sur un schéma Bearer DOIT porter `WWW-Authenticate`
    // pour indiquer au client le schéma d'auth attendu (audit F21). Code unique
    // `invalid_token` — simplification volontaire et conforme, couvrant les 3
    // points d'auth de l'API (jeton absent ou clé inconnue) sans logique par site.
    if status == StatusCode::UNAUTHORIZED {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static(r#"Bearer error="invalid_token""#),
        );
    }
    response
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        problem_response(self.status, self.code, self.title, self.detail)
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

/// Extracteur `Query<T>` qui traduit un échec de désérialisation des paramètres
/// de requête en **Problem Details** (`400 bad_request`) au lieu du rejet
/// `400 text/plain` par défaut d'axum (audit F15, ADR-0021). S'utilise comme
/// `Query` : `ValidatedQuery(query): ValidatedQuery<T>`.
pub(crate) struct ValidatedQuery<T>(pub(crate) T);

impl<T, S> FromRequestParts<S> for ValidatedQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(value)| Self(value))
            .map_err(|rejection| ApiError::bad_request(rejection.body_text()))
    }
}
