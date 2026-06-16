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
//! - `GET /v1/schedule`, `GET /v1/schedule/slots`, `GET /v1/intensity/below` —
//!   scheduling carbon-aware (ADR-0014).
//! - `GET /v1/intensity/stream` — flux **live** SSE des mises à jour (ADR-0014).
//! - `POST`/`GET /v1/webhooks`, `DELETE /v1/webhooks/{id}` — abonnements webhook
//!   (ADR-0016, **clé API requise**).
//! - `GET /v1/openapi.json` — spécification OpenAPI 3.1 ; `GET /docs` — Swagger UI.
//! - `GET /health` — sonde de disponibilité.
//!
//! Les endpoints `/v1` acceptent les paramètres optionnels `?region=<slug>`
//! (national par défaut) et `?methodology=<id>` (`rte-direct` par défaut ;
//! `acv-ademe` pour la vue cycle de vie, ADR-0008).

mod auth;
mod carbonfr_openapi;
mod dto;
mod error;
mod handlers;

pub use auth::{AuthConfig, AuthState, enforce, key_fingerprint};

use axum::Router;
use axum::routing::{get, post};
use carbonfr_core::ports::{
    ApiKeyRepository, CrossBorderRepository, ForecastModel, IntensityRepository,
    SubscriptionRepository, VisitCounter,
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
    /// Faire confiance à `X-Forwarded-For` pour l'IP client (uniquement derrière
    /// un reverse proxy de confiance, ADR-0007). Faux par défaut : sans proxy,
    /// l'en-tête est spoofable.
    pub(crate) trust_proxy: bool,
}

impl<R> AppState<R> {
    /// Crée l'état avec la méthodologie par défaut du MVP (`rte-direct`).
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            methodology: "rte-direct".to_string(),
            visit_salt: DEFAULT_VISIT_SALT.to_string(),
            trust_proxy: false,
        }
    }

    /// Active la confiance dans `X-Forwarded-For`/`X-Real-IP` (derrière un proxy).
    pub fn with_trust_proxy(mut self, trust: bool) -> Self {
        self.trust_proxy = trust;
        self
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
    /// Modèle de prévision **`acv-ademe@2`** (consommation, ADR-0013), optionnel.
    /// Dispatch **dynamique** : son type concret (composé de deux ports) n'a pas
    /// à contaminer le `F` générique du chemin scalaire. `None` si non câblé.
    pub(crate) consumption:
        Option<std::sync::Arc<dyn carbonfr_core::ports::ForecastModel + Send + Sync>>,
    /// Identité versionnée du modèle `@2` (ex. `acv-clim@1`).
    pub(crate) consumption_model: String,
}

impl<F> ForecastState<F> {
    /// Crée l'état avec un modèle (son identité versionnée) et la méthodologie
    /// par défaut (`rte-direct`).
    pub fn new(forecaster: F, model: impl Into<String>) -> Self {
        Self {
            forecaster,
            model: model.into(),
            methodology: "rte-direct".to_string(),
            consumption: None,
            consumption_model: String::new(),
        }
    }

    /// Sélectionne une autre méthodologie servie par défaut.
    pub fn with_methodology(mut self, methodology: impl Into<String>) -> Self {
        self.methodology = methodology.into();
        self
    }

    /// Câble le modèle de prévision `acv-ademe@2` (ADR-0013) servi via
    /// `?methodology=acv-ademe&version=2`.
    pub fn with_consumption(
        mut self,
        model: std::sync::Arc<dyn carbonfr_core::ports::ForecastModel + Send + Sync>,
        model_id: impl Into<String>,
    ) -> Self {
        self.consumption = Some(model);
        self.consumption_model = model_id.into();
        self
    }
}

/// État des endpoints de **streaming** (ADR-0014 §2) : un canal de diffusion
/// (`broadcast`) alimenté par le poller. Chaque connexion SSE s'y abonne. Pas de
/// repository ni d'état par-client — la posture anonyme/sans état est préservée.
///
/// Mécanisme **canal mémoire** (poller intégré au même process). Pour un
/// `bin/poller` séparé (ADR-0007), remplacer la source du canal par
/// `LISTEN`/`NOTIFY` Postgres — l'abonnement SSE et le fan-out restent identiques.
#[derive(Clone)]
pub struct StreamState {
    pub(crate) updates: tokio::sync::broadcast::Sender<carbonfr_core::domain::IntensityUpdate>,
}

impl StreamState {
    pub fn new(
        updates: tokio::sync::broadcast::Sender<carbonfr_core::domain::IntensityUpdate>,
    ) -> Self {
        Self { updates }
    }
}

/// Construit le routeur de l'API, prêt à être servi par `axum::serve`.
///
/// Les routes de lecture/écriture partagent [`AppState`] (le repository) ; les
/// routes de **prévision** ont leur propre [`ForecastState`] (un
/// [`ForecastModel`]). Deux sous-routeurs, chacun avec son état, **fusionnés**
/// (`merge`) — ce qui évite d'imposer le type du modèle aux handlers existants.
pub fn router<R, F>(state: AppState<R>, forecast: ForecastState<F>, stream: StreamState) -> Router
where
    R: IntensityRepository
        + VisitCounter
        + CrossBorderRepository
        + ApiKeyRepository
        + SubscriptionRepository
        + Clone
        + Send
        + Sync
        + 'static,
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
        .route(
            "/v1/webhooks",
            post(handlers::create_webhook::<R>).get(handlers::list_webhooks::<R>),
        )
        .route(
            "/v1/webhooks/{id}",
            axum::routing::delete(handlers::delete_webhook::<R>),
        )
        .route("/v1/openapi.json", get(carbonfr_openapi::openapi))
        .route("/docs", get(carbonfr_openapi::swagger_ui))
        .route("/health", get(handlers::health))
        .route("/health/ready", get(handlers::health_ready::<R>))
        .with_state(state);

    let forecasting = Router::new()
        .route("/v1/intensity/forecast", get(handlers::forecast::<F>))
        .route(
            "/v1/intensity/greenest-window",
            get(handlers::greenest_window::<F>),
        )
        .route("/v1/schedule", get(handlers::schedule::<F>))
        .route("/v1/schedule/slots", get(handlers::schedule_slots::<F>))
        .route("/v1/intensity/below", get(handlers::intensity_below::<F>))
        .with_state(forecast);

    let streaming = Router::new()
        .route("/v1/intensity/stream", get(handlers::intensity_stream))
        .with_state(stream);

    core.merge(forecasting)
        .merge(streaming)
        // Limite de corps serrée : nos seuls POST (webhook, visite) sont de petits
        // JSON. 16 Kio plafonne un corps abusif bien sous le défaut axum (2 Mio).
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        // Trace HTTP (méthode, chemin, statut, latence) — observabilité prod.
        .layer(tower_http::trace::TraceLayer::new_for_http())
}
