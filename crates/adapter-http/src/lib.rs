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
//! ## Endpoints (phase 1, socle national)
//!
//! - `GET /v1/intensity/now` — dernière intensité carbone (gCO₂eq/kWh).
//! - `GET /v1/mix` — mix de production (MW par filière).
//! - `GET /health` — sonde de disponibilité.
//!
//! Les deux endpoints `/v1` acceptent un paramètre optionnel `?region=<slug>`
//! (national par défaut).

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
        .route("/v1/mix", get(handlers::mix::<R>))
        .route("/health", get(handlers::health))
        .with_state(state)
}
