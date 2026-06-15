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
//! - `GET /v1/intensity/forecast?from=&horizon_hours=` — intensité **prévue**
//!   sur l'horizon (modèle `climatology@1`, ADR-0009).
//! - `GET /v1/intensity/greenest-window?from=&horizon_hours=&window_minutes=` —
//!   créneau le plus bas-carbone à venir.
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
use axum::routing::{get, post};
use carbonfr_core::ports::{
    CrossBorderRepository, ForecastModel, IntensityRepository, VisitCounter,
};

pub use error::ApiError;

/// Sel par défaut du hachage des visiteurs. **À surcharger en production**
/// (`CARBONFR_VISIT_SALT`) : un sel secret stable empêche de retrouver une IP.
const DEFAULT_VISIT_SALT: &str = "carbon-fr";

/// État partagé par les handlers : repository, méthodologie servie, sel du
/// compteur de visiteurs.
#[derive(Clone)]
pub struct AppState<R> {
    pub(crate) repo: R,
    pub(crate) methodology: String,
    pub(crate) visit_salt: String,
}

impl<R> AppState<R> {
    /// Crée l'état avec la méthodologie par défaut du MVP (`rte-direct`).
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            methodology: "rte-direct".to_string(),
            visit_salt: DEFAULT_VISIT_SALT.to_string(),
        }
    }

    /// Sélectionne une autre méthodologie servie (ex. `acv-ademe` plus tard).
    pub fn with_methodology(mut self, methodology: impl Into<String>) -> Self {
        self.methodology = methodology.into();
        self
    }

    /// Définit le sel de hachage des visiteurs (depuis la config).
    pub fn with_visit_salt(mut self, salt: impl Into<String>) -> Self {
        self.visit_salt = salt.into();
        self
    }
}

/// État des endpoints de **prévision** (ADR-0009), distinct de [`AppState`] : il
/// porte un modèle [`ForecastModel`] (le port, injecté par la composition root —
/// l'adapter HTTP ignore l'implémentation concrète) plutôt que le repository.
///
/// `model` est l'identité versionnée annoncée au client (ex. `climatology@1`) ;
/// `methodology` est la méthodologie servie par défaut.
#[derive(Clone)]
pub struct ForecastState<F> {
    pub(crate) forecaster: F,
    pub(crate) model: String,
    pub(crate) methodology: String,
}

impl<F> ForecastState<F> {
    /// Crée l'état avec un modèle (son identité versionnée) et la méthodologie
    /// par défaut (`rte-direct`).
    pub fn new(forecaster: F, model: impl Into<String>) -> Self {
        Self {
            forecaster,
            model: model.into(),
            methodology: "rte-direct".to_string(),
        }
    }

    /// Sélectionne une autre méthodologie servie par défaut.
    pub fn with_methodology(mut self, methodology: impl Into<String>) -> Self {
        self.methodology = methodology.into();
        self
    }
}

/// Construit le routeur de l'API, prêt à être servi par `axum::serve`.
///
/// Les routes de lecture/écriture partagent [`AppState`] (le repository) ; les
/// routes de **prévision** ont leur propre [`ForecastState`] (un
/// [`ForecastModel`]). Deux sous-routeurs, chacun avec son état, **fusionnés**
/// (`merge`) — ce qui évite d'imposer le type du modèle aux handlers existants.
pub fn router<R, F>(state: AppState<R>, forecast: ForecastState<F>) -> Router
where
    R: IntensityRepository + VisitCounter + CrossBorderRepository + Clone + Send + Sync + 'static,
    F: ForecastModel + Clone + Send + Sync + 'static,
{
    let core = Router::new()
        .route("/v1/intensity/now", get(handlers::intensity_now::<R>))
        .route("/v1/intensity/date", get(handlers::intensity_date::<R>))
        .route("/v1/intensity/stats", get(handlers::intensity_stats::<R>))
        .route("/v1/mix", get(handlers::mix::<R>))
        .route("/v1/methodologies", get(handlers::methodologies))
        .route("/v1/factors", get(handlers::factors))
        .route("/v1/stats", get(handlers::visit_stats::<R>))
        .route("/v1/stats/visit", post(handlers::record_visit::<R>))
        .route("/v1/openapi.json", get(carbonfr_openapi::openapi))
        .route("/docs", get(carbonfr_openapi::swagger_ui))
        .route("/health", get(handlers::health))
        .with_state(state);

    let forecasting = Router::new()
        .route("/v1/intensity/forecast", get(handlers::forecast::<F>))
        .route(
            "/v1/intensity/greenest-window",
            get(handlers::greenest_window::<F>),
        )
        .with_state(forecast);

    core.merge(forecasting)
}
