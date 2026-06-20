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
//!
//! Les **erreurs** suivent **Problem Details** (RFC 9457, `application/problem+json`) :
//! `type`/`title`/`status`/`detail` + un `code` court et stable (ADR-0021, module
//! `error`).

mod auth;
mod carbonfr_openapi;
mod dto;
mod eligibility_uc;
mod error;
mod handlers;

pub use auth::{AuthConfig, AuthState, enforce, key_fingerprint};

use axum::Router;
use axum::routing::{get, post};
use carbonfr_core::ports::{
    ApiKeyRepository, CrossBorderRepository, ForecastModel, IntensityRepository,
    SpotPriceRepository, SubscriptionRepository, VisitCounter, WeatherRepository,
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
    /// Modèle de dérivation renouvelable **calibré au démarrage** (ADR-0018),
    /// servi par `/v1/renewable`. `None` si la calibration a échoué (historique
    /// insuffisant) → l'endpoint répond `503`.
    pub(crate) renewable_model: Option<carbonfr_core::domain::RenewableModel>,
}

impl<R> AppState<R> {
    /// Crée l'état avec la méthodologie par défaut du MVP (`rte-direct`).
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            methodology: "rte-direct".to_string(),
            visit_salt: DEFAULT_VISIT_SALT.to_string(),
            trust_proxy: false,
            renewable_model: None,
        }
    }

    /// Injecte le modèle de dérivation renouvelable calibré (composition root).
    pub fn with_renewable_model(
        mut self,
        model: Option<carbonfr_core::domain::RenewableModel>,
    ) -> Self {
        self.renewable_model = model;
        self
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

/// Accès minimal de l'**overlay d'éligibilité** (ADR-0025/0026) : mix nowcast
/// national + prix spot day-ahead. Trait **objet-safe** (dispatch dynamique) pour
/// ne pas contaminer le `F` générique du chemin de prévision — même motif que
/// `consumption: Arc<dyn ForecastModel>`. Implémenté en *blanket* par tout
/// repository qui sait lire l'intensité **et** le prix spot.
#[async_trait::async_trait]
pub trait EligibilityRepo: Send + Sync {
    /// Dernière mesure nationale (mix) — ancre `rte-direct` (convention canonique
    /// du mix national, cf. `GetElectricityPrice`). `None` si indisponible.
    async fn latest_national_mix(&self) -> Option<carbonfr_core::domain::Measurement>;

    /// Prix spot day-ahead (€/MWh) **frais** au créneau `at` (filtre d'ancienneté
    /// appliqué : pas d'extrapolation du dernier day-ahead sur le futur).
    async fn spot_price_at(&self, at: time::OffsetDateTime) -> Option<f64>;
}

/// Adaptateur d'un repository concret (`R: IntensityRepository +
/// SpotPriceRepository`) vers [`EligibilityRepo`]. Un **wrapper** plutôt qu'un
/// blanket impl `for R` : ce dernier entrerait en conflit de cohérence (E0119)
/// avec d'autres implémentations (ex. fakes de test) qu'on ne peut pas prouver
/// disjointes. Le composition root l'instancie sur le repo PostgreSQL.
pub struct EligibilityRepoAdapter<R>(pub R);

#[async_trait::async_trait]
impl<R> EligibilityRepo for EligibilityRepoAdapter<R>
where
    R: IntensityRepository + SpotPriceRepository,
{
    async fn latest_national_mix(&self) -> Option<carbonfr_core::domain::Measurement> {
        // Ancre `rte-direct` : convention canonique du mix national (alignée sur
        // `/v1/intensity/now`, `/v1/mix`, `GetElectricityPrice`). `rte-direct` est
        // strictement plus disponible et `acv-ademe` pourrait résoudre vers `@2`
        // (consommation, mix incertain) car `latest()` filtre l'id sans la version.
        self.0
            .latest(carbonfr_core::domain::Region::National, "rte-direct")
            .await
            .ok()
            .flatten()
    }

    async fn spot_price_at(&self, at: time::OffsetDateTime) -> Option<f64> {
        // `price_at` renvoie le prix au plus proche ≤ at. On REFUSE un prix périmé
        // de plus d'1 h pour ne pas propager le dernier day-ahead sur le futur
        // (PIÈGE 2 : au-delà du day-ahead, le signal prix reste indéterminé).
        self.0
            .price_at(at)
            .await
            .ok()
            .flatten()
            .filter(|p| (at - p.at).whole_hours().abs() <= 1)
            .map(|p| p.eur_per_mwh)
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
    /// Overlay d'**éligibilité électrolyseur** (ADR-0025/0026), optionnel. Fournit
    /// le mix nowcast + le prix spot à `greenest-window?eligibility=`. Dispatch
    /// dynamique (même motif que `consumption`). `None` → overlay non câblé (503
    /// si demandé), self-hosting et prévision classique intacts.
    pub(crate) eligibility: Option<std::sync::Arc<dyn EligibilityRepo>>,
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
            eligibility: None,
        }
    }

    /// Câble l'overlay d'éligibilité électrolyseur (ADR-0025/0026), servi via
    /// `GET /v1/intensity/greenest-window?eligibility=`. Sans cet appel, l'overlay
    /// répond `503` (et la prévision classique reste inchangée).
    pub fn with_eligibility(mut self, repo: std::sync::Arc<dyn EligibilityRepo>) -> Self {
        self.eligibility = Some(repo);
        self
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
        + WeatherRepository
        + SpotPriceRepository
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
        .route("/v1/exchanges", get(handlers::exchanges::<R>))
        .route("/v1/exchanges/date", get(handlers::exchanges_date::<R>))
        .route("/v1/weather", get(handlers::weather::<R>))
        .route("/v1/weather/date", get(handlers::weather_date::<R>))
        .route("/v1/renewable", get(handlers::renewable::<R>))
        .route("/v1/methodologies", get(handlers::methodologies))
        .route(
            "/v1/eligibility/rulesets",
            get(handlers::eligibility_rulesets),
        )
        .route("/v1/factors", get(handlers::factors))
        .route("/v1/price", get(handlers::price::<R>))
        .route("/v1/price/date", get(handlers::price_date::<R>))
        .route("/v1/cost-reference", get(handlers::cost_reference))
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
        // CORS **permissif** : l'API sert de la donnée publique en lecture et se
        // veut dev-first (cf. carbonintensity.org.uk). Toute origine peut donc lire
        // les réponses depuis un navigateur — nécessaire pour qu'un site tiers (dont
        // carbon-fr.kovelt.fr) consomme l'API. Pas de cookies : `Any` est sûr (les
        // clés API passent par l'en-tête `Authorization`, pas par `credentials`).
        // Couche la plus externe : gère le préflight `OPTIONS` avant le routage.
        .layer(cors_layer())
}

/// Politique CORS de l'API : ouverte en lecture (origine/méthodes/en-têtes `Any`),
/// expose les en-têtes de quota (`RateLimit-*`) au client navigateur. À restreindre
/// (origines explicites) seulement si une instance veut cloisonner son API.
fn cors_layer() -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any)
}
