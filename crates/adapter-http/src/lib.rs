//! # carbonfr-adapter-http
//!
//! Adapter **entrant** : API HTTP (axum) qui expose les cas d'usage du `core`.
//!
//! Tout endpoint public est versionné sous `/v1` (l'URL est un contrat,
//! ADR-0007). La sérialisation JSON et le mapping des erreurs vivent ici ; le
//! `core` reste pur.
//!
//! Le routeur est **générique sur le repository** (`R: IntensityRepository`) :
//! dispatch statique de bout en bout. La composition root (`bin/server`) injecte
//! l'implémentation concrète (PostgreSQL).
//!
//! ## Endpoints (socle national)
//!
//! - `GET /v1/intensity/now` — dernière intensité carbone (gCO₂eq/kWh).
//! - `GET /v1/intensity/date?from=&to=` — série historique sur un intervalle
//!   RFC 3339 (fenêtre ≤ 366 jours).
//! - `GET /v1/intensity/stats?from=&to=[&interval=hour|day]` — résumé
//!   (moyenne/min/max) et, optionnellement, série agrégée (rollups).
//! - `GET /v1/mix` — mix de production (MW par filière).
//! - `GET /v1/openapi.json` — spécification OpenAPI 3.1 ; `GET /docs` — Swagger UI.
//! - `GET /health` — sonde de disponibilité.
//!
//! Les endpoints `/v1` acceptent les paramètres optionnels `?region=<slug>`
//! (national par défaut) et `?methodology=<id>` (`rte-direct` par défaut ;
//! `acv-ademe` pour la vue cycle de vie, ADR-0008).

mod carbonfr_openapi;
mod dto;
mod error;
mod handlers;

use axum::Router;
use axum::routing::get;
use carbonfr_core::ports::IntensityRepository;

pub use error::ApiError;

/// État partagé par les handlers : le repository et la méthodologie servie.
#[derive(Clone)]
pub struct AppState<R> {
    pub(crate) repo: R,
    pub(crate) methodology: String,
}

impl<R> AppState<R> {
    /// Crée l'état avec la méthodologie par défaut du MVP (`rte-direct`).
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            methodology: "rte-direct".to_string(),
        }
    }

    /// Sélectionne une autre méthodologie servie (ex. `acv-ademe` plus tard).
    pub fn with_methodology(mut self, methodology: impl Into<String>) -> Self {
        self.methodology = methodology.into();
        self
    }
}

/// Construit le routeur de l'API, prêt à être servi par `axum::serve`.
pub fn router<R>(state: AppState<R>) -> Router
where
    R: IntensityRepository + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/v1/intensity/now", get(handlers::intensity_now::<R>))
        .route("/v1/intensity/date", get(handlers::intensity_date::<R>))
        .route("/v1/intensity/stats", get(handlers::intensity_stats::<R>))
        .route("/v1/mix", get(handlers::mix::<R>))
        .route("/v1/openapi.json", get(carbonfr_openapi::openapi))
        .route("/docs", get(carbonfr_openapi::swagger_ui))
        .route("/health", get(handlers::health))
        .with_state(state)
}
