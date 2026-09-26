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
//! ## Endpoints
//!
//! Vue d'ensemble par famille ; le **contrat détaillé** (paramètres, schémas,
//! codes d'erreur) fait foi dans `GET /v1/openapi.json` et le README
//! (§ Fonctionnalités). Le routeur est dans [`router`].
//!
//! - **Intensité** : `GET /v1/intensity/now`, `/v1/intensity/date?from=&to=`
//!   (fenêtre ≤ 366 jours, ≤ 92 jours en `acv-ademe@2`),
//!   `/v1/intensity/stats?from=&to=[&interval=hour|day]` (résumé exact sur les
//!   mesures brutes ; série agrégée optionnelle depuis les rollups) ;
//!   `GET /v1/mix` (mix de production, MW par filière).
//! - **Prévision & scheduling** : `GET /v1/intensity/forecast` (`climatology@1`,
//!   ADR-0009), `/v1/intensity/greenest-window` (+ overlay électrolyseur
//!   `?eligibility=rfnbo|low-carbon`, ADR-0025/0026), `/v1/schedule`,
//!   `/v1/schedule/slots`, `/v1/intensity/below` (carbon-aware, ADR-0014).
//! - **Live** : `GET /v1/intensity/stream` — flux SSE des mises à jour (ADR-0014).
//! - **Contexte** : `GET /v1/exchanges`, `/v1/exchanges/date` (échanges
//!   transfrontaliers, ADR-0017), `/v1/weather`, `/v1/weather/date`,
//!   `/v1/renewable` (renouvelable estimé, ADR-0018).
//! - **Prix & coût** : `GET /v1/price`, `/v1/price/date` (ADR-0023),
//!   `/v1/cost-reference` (LCOE, ADR-0024).
//! - **Catalogues** : `GET /v1/methodologies`, `/v1/factors`,
//!   `/v1/eligibility/rulesets`.
//! - **Webhooks** : `POST`/`GET /v1/webhooks`, `DELETE /v1/webhooks/{id}`
//!   (ADR-0016, **clé API requise**).
//! - **Compteur de visites** : `GET /v1/stats`, `POST /v1/stats/visit` (IP jamais
//!   stockée).
//! - **Documentation** : `GET /v1` (redirige vers `/docs`), `GET /v1/openapi.json`
//!   (OpenAPI 3.1), `GET /docs` (Swagger UI).
//! - **Hors contrat `/v1`** : `GET /hydrogene` (+ `sites.json`,
//!   `regions.geojson`, `pays.geojson`) — carte électrolyseurs × carbone live
//!   (ADR-0029) ; `GET /health`, `GET /health/ready` (sondes). `/metrics` est
//!   monté par `bin/server`, pas ici.
//!
//! Les endpoints d'intensité acceptent `?region=<slug>` (national par défaut) et
//! `?methodology=<id>` (`rte-direct` par défaut, national uniquement ;
//! `acv-ademe` pour la vue cycle de vie, national + 12 régions, ADR-0008 ;
//! `&version=2` pour la vue consommation, national, ADR-0010 — refusée par
//! `/v1/mix`, qui ne sert que le mix de production).
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
mod hydrogene;

pub use auth::{AuthConfig, AuthState, enforce, key_fingerprint};
pub use eligibility_uc::ShareForecastConfig;

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
    /// En-tête d'IP réelle **écrasé par le proxy** (ex. `x-real-ip`), opt-in via
    /// `CARBONFR_REAL_IP_HEADER`. `None` (défaut) : dernier segment de
    /// `X-Forwarded-For` (cf. `AuthConfig::real_ip_header`, audit 2026-08).
    pub(crate) real_ip_header: Option<String>,
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
            real_ip_header: None,
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

    /// Active la confiance dans `X-Forwarded-For` (derrière un proxy).
    pub fn with_trust_proxy(mut self, trust: bool) -> Self {
        self.trust_proxy = trust;
        self
    }

    /// Configure l'en-tête d'IP réelle dédié (`CARBONFR_REAL_IP_HEADER`,
    /// opt-in) — uniquement si le proxy l'**écrase** systématiquement.
    pub fn with_real_ip_header(mut self, header: Option<String>) -> Self {
        self.real_ip_header = header;
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

    /// Prix spot day-ahead (€/MWh) sur un intervalle, **triés par horodatage
    /// croissant**. Un **seul** aller-retour pour tous les créneaux (vs une requête
    /// par créneau, audit F05) : la garde de fraîcheur (« ≤ 1 h, pas
    /// d'extrapolation ») est appliquée en mémoire par l'appelant. `Vec` vide si
    /// indisponible.
    async fn spot_prices_range(
        &self,
        range: carbonfr_core::domain::TimeRange,
    ) -> Vec<(time::OffsetDateTime, f64)>;

    /// Historique du mix national (`rte-direct`) sur un intervalle, **trié par
    /// horodatage croissant** — la matière première de la climatologie de part
    /// renouvelable `share-clim@1` (ADR-0028). Un **seul** aller-retour batch
    /// (même discipline que `spot_prices_range`, audit F05). `Vec` vide si
    /// indisponible.
    async fn national_mix_range(
        &self,
        range: carbonfr_core::domain::TimeRange,
    ) -> Vec<carbonfr_core::domain::Measurement>;
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
        // `price_at` renvoie le prix au plus proche ≤ at. On REFUSE un prix
        // périmé de `MAX_PRICE_AGE` (1 h) ou plus — borne STRICTE **partagée**
        // avec `freshest_price` (eligibility_uc.rs) : un prix horaire couvre
        // `[t, t + 1 h)`, à `t + 1 h` exactement c'est l'heure de livraison
        // suivante (audit 2026-08). But : ne pas propager le dernier day-ahead
        // sur le futur (PIÈGE 2 : au-delà du day-ahead, le signal prix reste
        // indéterminé). NB : comparer les `Duration` directement, PAS
        // `whole_hours()` (division entière → tolérerait jusqu'à ~2 h). Garde
        // `>= ZERO` au cas où une autre impl de `price_at` renverrait un prix
        // postérieur à `at`.
        self.0
            .price_at(at)
            .await
            .ok()
            .flatten()
            .filter(|p| {
                let age = at - p.at;
                age >= time::Duration::ZERO && age < crate::eligibility_uc::MAX_PRICE_AGE
            })
            .map(|p| p.eur_per_mwh)
    }

    async fn spot_prices_range(
        &self,
        range: carbonfr_core::domain::TimeRange,
    ) -> Vec<(time::OffsetDateTime, f64)> {
        // Prix bruts de l'intervalle (tri croissant garanti par le repository) ;
        // la garde de fraîcheur ≤ 1 h est appliquée à la lecture par l'appelant.
        self.0
            .price_range(range)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.at, p.eur_per_mwh))
            .collect()
    }

    async fn national_mix_range(
        &self,
        range: carbonfr_core::domain::TimeRange,
    ) -> Vec<carbonfr_core::domain::Measurement> {
        // Même ancre `rte-direct` que `latest_national_mix` (ADR-0026 décision 9).
        self.0
            .range(carbonfr_core::domain::Region::National, "rte-direct", range)
            .await
            .unwrap_or_default()
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
    /// Modèle `share-clim@1` (part renouvelable **prévue**, ADR-0028), optionnel.
    /// `None` (bandes non calibrées, opt-out) → la part future reste
    /// `Indeterminate`, comportement d'avant ADR-0028.
    pub(crate) share_forecast: Option<std::sync::Arc<crate::eligibility_uc::ShareForecastConfig>>,
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
            share_forecast: None,
        }
    }

    /// Câble le modèle `share-clim@1` (part renouvelable prévue pour le pilier
    /// `renewable-share` de `rfnbo`, ADR-0028). Sans cet appel, les créneaux
    /// futurs restent `Indeterminate` sur ce pilier (jamais d'extrapolation).
    pub fn with_share_forecast(
        mut self,
        config: std::sync::Arc<crate::eligibility_uc::ShareForecastConfig>,
    ) -> Self {
        self.share_forecast = Some(config);
        self
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

/// Plafond par défaut de connexions SSE **simultanées** (ADR-0014 addendum
/// 2026-09-26, item SEC-1/PERF-2 du plan I8). Volontairement généreux : le
/// risque d'un plafond trop bas (couper des clients légitimes) est pire que
/// celui d'un plafond trop haut ; on resserre métriques à l'appui
/// (`carbonfr_sse_connections_*`), jamais à l'aveugle.
pub const DEFAULT_SSE_MAX_CONNECTIONS: usize = 300;

/// `Retry-After` (secondes) joint au 503 « plafond de connexions SSE atteint ».
/// Aligné sur le palier de backoff du SDK Rust (1 → 30 s) : un client patient
/// réessaie vite, sans salve de reconnexions immédiates si le plafond reste
/// tendu (un créneau SSE se libère rarement en moins de quelques secondes).
pub(crate) const SSE_CAP_RETRY_AFTER_SECS: u32 = 30;

/// Plafond de connexions SSE simultanées (`GET /v1/intensity/stream`), global au
/// processus — ADR-0014 addendum 2026-09-26. Même motif que la borne des
/// livraisons webhook (`Semaphore::new(50)` dans la composition root) : un
/// sémaphore, un permis par connexion ouverte, **refus immédiat** au-delà
/// (503 avec `Retry-After`) plutôt qu'une file d'attente — un flux SSE est
/// long par nature, mettre une connexion en attente d'un créneau reviendrait
/// à bloquer un handler HTTP sans borne.
///
/// Le permis est **tenu par le flux de la réponse** (cf. `hold_permit` dans
/// `handlers.rs`) et rendu quand ce flux est abandonné : fermeture par le
/// client (FIN TCP, vu aussitôt par hyper), arrêt gracieux (`take_until` sur le
/// jeton), ou échec d'écriture si le client a disparu **sans** fermer (coupure
/// réseau, veille) — le keep-alive SSE de 15 s garantit une écriture régulière,
/// mais son échec n'est constaté qu'à l'épuisement des retransmissions TCP du
/// noyau (≈ 15 min avec `tcp_retries2 = 15`, défaut Linux ; aucune option
/// `SO_KEEPALIVE`/`TCP_USER_TIMEOUT` n'est posée sur les sockets acceptées).
/// Un tel « permis fantôme » est donc compté jusqu'à un quart d'heure :
/// dimensionner le plafond en conséquence, et surveiller
/// `carbonfr_sse_connections_active` avant de le resserrer.
///
/// Sans état par-client : le plafond n'est **pas** un sous-quota par IP ou par
/// clé (le tier hébergé, ADR-0015, borne le débit des requêtes, pas les
/// connexions tenues ouvertes). `Clone` bon marché (`Arc` interne) : la
/// composition root en garde une copie pour l'exposition `/metrics`.
#[derive(Clone)]
pub struct SseLimiter {
    inner: std::sync::Arc<SseLimiterInner>,
}

struct SseLimiterInner {
    semaphore: std::sync::Arc<tokio::sync::Semaphore>,
    max: usize,
    rejected_total: std::sync::atomic::AtomicU64,
}

impl SseLimiter {
    /// Limiteur à `max` connexions simultanées. Un plafond nul n'aurait aucun
    /// sens (toute connexion refusée) : `0` est relevé à `1` — la composition
    /// root refuse de toute façon la valeur à la configuration.
    pub fn new(max: usize) -> Self {
        let max = max.max(1);
        Self {
            inner: std::sync::Arc::new(SseLimiterInner {
                semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(max)),
                max,
                rejected_total: std::sync::atomic::AtomicU64::new(0),
            }),
        }
    }

    /// Plafond configuré.
    pub fn max(&self) -> usize {
        self.inner.max
    }

    /// Connexions SSE actuellement ouvertes (permis pris).
    pub fn active(&self) -> usize {
        self.inner
            .max
            .saturating_sub(self.inner.semaphore.available_permits())
    }

    /// Connexions refusées (503) depuis le démarrage parce que le plafond était
    /// atteint.
    pub fn rejected_total(&self) -> u64 {
        self.inner
            .rejected_total
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Tente de prendre un permis, **sans attendre**. `None` (et compteur de
    /// refus incrémenté) si le plafond est atteint.
    pub(crate) fn try_acquire(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        match std::sync::Arc::clone(&self.inner.semaphore).try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => {
                self.inner
                    .rejected_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                None
            }
        }
    }
}

impl Default for SseLimiter {
    fn default() -> Self {
        Self::new(DEFAULT_SSE_MAX_CONNECTIONS)
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
    /// Jeton d'**arrêt** propagé par la composition root (audit 2026-08) : son
    /// annulation clôt les flux SSE ouverts. Sans cette clôture, l'arrêt
    /// gracieux (`with_graceful_shutdown`) attendrait indéfiniment ces
    /// connexions infinies — le `Sender` broadcast vit dans l'app possédée par
    /// `serve`, le flux ne peut donc jamais se terminer de lui-même. Défaut :
    /// jeton jamais annulé (tests, usages sans arrêt orchestré).
    pub(crate) shutdown: tokio_util::sync::CancellationToken,
    /// Plafond de connexions SSE simultanées (ADR-0014 addendum 2026-09-26).
    /// Défaut : [`DEFAULT_SSE_MAX_CONNECTIONS`] ; la composition root injecte le
    /// sien (`CARBONFR_SSE_MAX_CONNECTIONS`) et en garde une copie pour `/metrics`.
    pub(crate) sse_limiter: SseLimiter,
}

impl StreamState {
    pub fn new(
        updates: tokio::sync::broadcast::Sender<carbonfr_core::domain::IntensityUpdate>,
    ) -> Self {
        Self {
            updates,
            shutdown: tokio_util::sync::CancellationToken::new(),
            sse_limiter: SseLimiter::default(),
        }
    }

    /// Propage le jeton d'arrêt de la composition root : à son annulation, les
    /// flux SSE en cours se terminent proprement et le drain de l'arrêt
    /// gracieux peut aboutir (audit 2026-08).
    pub fn with_shutdown(mut self, token: tokio_util::sync::CancellationToken) -> Self {
        self.shutdown = token;
        self
    }

    /// Remplace le plafond de connexions SSE par défaut (composition root :
    /// `CARBONFR_SSE_MAX_CONNECTIONS` ; tests : plafond minuscule).
    pub fn with_sse_limiter(mut self, limiter: SseLimiter) -> Self {
        self.sse_limiter = limiter;
        self
    }
}

/// Délai par défaut de traitement d'une requête (`CARBONFR_REQUEST_TIMEOUT_SECS`),
/// item SEC-4 du plan I8 : 2 × le `statement_timeout` Postgres par défaut
/// (`CARBONFR_DB_STATEMENT_TIMEOUT_MS` = 30 000 ms) pour que la base tranche
/// **d'abord** une requête lente (erreur précise) et que ce filet générique ne
/// joue qu'en dernier ressort. ⚠️ Relever le `statement_timeout` sans relever
/// ce délai en proportion réintroduit la course.
pub const DEFAULT_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Réglages **transverses** du routeur, hors états métier (cf. [`router_with_options`]).
/// `Default` = les valeurs servies par [`router`].
#[derive(Clone, Copy, Debug)]
pub struct RouterOptions {
    /// Borne le traitement complet d'une requête — auth/quota, lecture du corps,
    /// handler — au-delà de laquelle le client reçoit un 503 Problem Details
    /// (code `unavailable`). **N'affecte jamais un flux SSE ouvert** : seul le
    /// temps d'établir la réponse est borné, pas la durée du corps (cf. le
    /// middleware `request_timeout`).
    pub request_timeout: std::time::Duration,
}

impl Default for RouterOptions {
    fn default() -> Self {
        Self {
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

/// Construit le routeur de l'API, prêt à être servi par `axum::serve`.
///
/// Les routes de lecture/écriture partagent [`AppState`] (le repository) ; les
/// routes de **prévision** ont leur propre [`ForecastState`] (un
/// [`ForecastModel`]). Deux sous-routeurs, chacun avec son état, **fusionnés**
/// (`merge`) — ce qui évite d'imposer le type du modèle aux handlers existants.
///
/// `auth` (tier hébergé, ADR-0015, opt-in) : le middleware [`enforce`] est
/// appliqué **ici**, sous la couche CORS, et plus par la composition root
/// (audit 2026-08) : posé au-dessus du routeur, il devenait la couche la plus
/// externe → les préflights `OPTIONS` étaient décomptés du seau anonyme et ses
/// 401/429/503 partaient sans `Access-Control-Allow-Origin` (réponses opaques
/// en navigateur, `RateLimit-*`/`Retry-After` illisibles malgré
/// `expose_headers`).
pub fn router<R, F>(
    state: AppState<R>,
    forecast: ForecastState<F>,
    stream: StreamState,
    auth: Option<AuthState>,
) -> Router
where
    R: IntensityRepository
        + CrossBorderRepository
        + WeatherRepository
        + SpotPriceRepository
        + VisitCounter
        + ApiKeyRepository
        + SubscriptionRepository
        + Clone
        + Send
        + Sync
        + 'static,
    F: ForecastModel + Clone + Send + Sync + 'static,
{
    router_with_options(state, forecast, stream, auth, RouterOptions::default())
}

/// [`router`] avec des réglages transverses explicites ([`RouterOptions`]) —
/// utilisé par la composition root (variables d'environnement) et par les tests
/// qui ont besoin d'un délai minuscule. Le corps du routeur vit ici ; [`router`]
/// n'est qu'un raccourci avec les valeurs par défaut.
pub fn router_with_options<R, F>(
    state: AppState<R>,
    forecast: ForecastState<F>,
    stream: StreamState,
    auth: Option<AuthState>,
    options: RouterOptions,
) -> Router
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
        .route("/v1", get(handlers::v1_index))
        .route("/v1/openapi.json", get(carbonfr_openapi::openapi))
        .route("/docs", get(carbonfr_openapi::swagger_ui))
        // Page carte « électrolyseurs × carbone live » (couche B-light,
        // ADR-0025/0029) — HORS contrat /v1, comme `/docs` : page auto-contenue
        // (zéro CDN) + ses trois jeux de données embarqués (attribution incluse).
        .route("/hydrogene", get(hydrogene::page))
        .route("/hydrogene/sites.json", get(hydrogene::sites))
        .route("/hydrogene/regions.geojson", get(hydrogene::regions))
        .route("/hydrogene/pays.geojson", get(hydrogene::pays))
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

    let mut app = core
        .merge(forecasting)
        .merge(streaming)
        // Fallback AVANT les couches : un chemin inconnu doit traverser CORS/Trace
        // comme une réponse normale (sinon un client navigateur ne peut pas lire le
        // corps du 404). Cf. `fallback_not_found` (audit F16).
        .fallback(fallback_not_found)
        // Limite de corps serrée : nos seuls POST (webhook, visite) sont de petits
        // JSON. 16 Kio plafonne un corps abusif bien sous le défaut axum (2 Mio).
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        // En-tête `Cache-Control` sur les lectures stables (audit perf 2026-08).
        .layer(axum::middleware::from_fn(cache_control))
        // Trace HTTP (méthode, chemin, statut, latence) — observabilité prod.
        .layer(tower_http::trace::TraceLayer::new_for_http());
    // Tier hébergé (ADR-0015, opt-in) : auth + quota, appliqué SOUS la couche
    // CORS (dernier `.layer()` = plus externe) — cf. doc de `router()`.
    if let Some(auth) = auth {
        app = app.layer(axum::middleware::from_fn_with_state(auth, enforce));
    }
    // Délai de traitement (SEC-4, plan I8) : posé juste SOUS la couche CORS,
    // donc AU-DESSUS d'`enforce`, de la trace, du cache et de la limite de corps
    // — il borne la requête complète, lecture du corps ET auth/quota compris :
    // la résolution d'une clé API non encore en cache (`enforce`) est un accès
    // Postgres, précisément le genre d'attente qu'on veut borner (défense en
    // profondeur, en plus du `statement_timeout`). Son 503 traverse CORS et
    // reste lisible en navigateur. Le préflight `OPTIONS`, lui, n'atteint
    // jamais cette couche : CORS (plus externe) y répond sans déléguer, cf.
    // ci-dessous. Conséquence assumée : `TraceLayer` étant à l'intérieur, le
    // 503 de délai n'est pas classé par elle — le middleware le journalise
    // lui-même en `error` (méthode, chemin). Un flux SSE ouvert n'est pas
    // concerné (cf. `request_timeout`).
    app = app.layer(axum::middleware::from_fn_with_state(
        options.request_timeout,
        request_timeout,
    ));
    // CORS **permissif** : l'API sert de la donnée publique en lecture et se
    // veut dev-first (cf. carbonintensity.org.uk). Toute origine peut donc lire
    // les réponses depuis un navigateur — nécessaire pour qu'un site tiers (dont
    // carbon-fr.kovelt.fr) consomme l'API. Pas de cookies : `Any` est sûr (les
    // clés API passent par l'en-tête `Authorization`, pas par `credentials`).
    // Couche la plus externe : gère le préflight `OPTIONS` avant le routage
    // (donc avant `enforce` : un préflight n'est jamais décompté du quota) et
    // ajoute `Access-Control-Allow-Origin` aux réponses d'`enforce`.
    app.layer(cors_layer())
}

/// Chemins `GET` de **lecture stable** : leurs données ne changent qu'au cycle
/// du poller (~15 min) ou au démarrage (catalogues, constantes versionnées).
/// Exclus volontairement : le SSE (`/v1/intensity/stream`), les endpoints à
/// clé (`/v1/webhooks…`), le compteur de visiteurs (`/v1/stats`) et les sondes.
const CACHEABLE_PATHS: &[&str] = &[
    "/v1/intensity/now",
    "/v1/intensity/date",
    "/v1/intensity/stats",
    "/v1/mix",
    "/v1/exchanges",
    "/v1/exchanges/date",
    "/v1/weather",
    "/v1/weather/date",
    "/v1/renewable",
    "/v1/methodologies",
    "/v1/factors",
    "/v1/eligibility/rulesets",
    "/v1/price",
    "/v1/price/date",
    "/v1/cost-reference",
    "/v1/intensity/forecast",
    "/v1/intensity/greenest-window",
    "/v1/schedule",
    "/v1/schedule/slots",
    "/v1/intensity/below",
];

/// Marque les réponses `200` des lectures stables ([`CACHEABLE_PATHS`]) d'un
/// `Cache-Control: public, max-age=60` (audit perf 2026-08) : un cache HTTP
/// intermédiaire (navigateur, proxy, CDN d'une instance self-hostée) peut
/// absorber les rafales de polling sans risquer de servir plus d'une minute de
/// retard sur une donnée rafraîchie toutes les ~15 min. Pas d'`ETag` (les
/// corps sont petits, la revalidation n'apporterait rien) ; les erreurs ne
/// sont jamais marquées.
async fn cache_control(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let cacheable = request.method() == axum::http::Method::GET
        && CACHEABLE_PATHS.contains(&request.uri().path());
    let mut response = next.run(request).await;
    if cacheable && response.status() == axum::http::StatusCode::OK {
        response.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("public, max-age=60"),
        );
    }
    response
}

/// Borne le **traitement** d'une requête (SEC-4, plan I8) : au-delà de `limit`,
/// le futur de la requête est abandonné et le client reçoit un 503 Problem
/// Details (code `unavailable`), journalisé ici en `error` — le niveau que
/// `TraceLayer` (à l'intérieur de cette couche, donc jamais résolue dans ce
/// cas) applique à tout 5xx. Écrit à la main plutôt qu'avec
/// `tower_http::timeout::TimeoutLayer` (408 à corps vide) pour rester dans le
/// contrat RFC 9457 de toute l'API — y compris le 404 de repli.
///
/// **Portée, volontairement limitée à l'établissement de la réponse** :
/// `next.run(request)` se résout dès que le handler renvoie son objet
/// `Response` — pour `GET /v1/intensity/stream`, dès que `Sse::new(...)` est
/// construit. Le corps (le flux SSE, infini par construction) n'est jamais
/// tiré par ce middleware : un flux ouvert n'est **jamais** coupé par ce délai,
/// quelle que soit sa durée de vie (test `request_timeout_leaves_open_sse_alone`).
/// Ne pas remplacer par `tower_http::timeout::{ResponseBodyTimeoutLayer,
/// ResponseBodyDeadlineLayer}`, qui enveloppent le corps et tueraient le SSE.
async fn request_timeout(
    axum::extract::State(limit): axum::extract::State<std::time::Duration>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    match tokio::time::timeout(limit, next.run(request)).await {
        Ok(response) => response,
        Err(_elapsed) => {
            tracing::error!(
                %method,
                path,
                status = 503,
                timeout_secs = limit.as_secs(),
                "requête interrompue : délai de traitement dépassé"
            );
            axum::response::IntoResponse::into_response(ApiError::unavailable(format!(
                "traitement interrompu après {} s (délai maximal de requête)",
                limit.as_secs()
            )))
        }
    }
}

/// Fallback du routeur : tout chemin qui ne correspond à aucune route déclarée
/// reçoit un 404 **Problem Details** (ADR-0021) au lieu du 404 axum par défaut
/// (corps vide, sans `Content-Type`) — audit F16.
async fn fallback_not_found() -> axum::response::Response {
    error::problem_response(
        axum::http::StatusCode::NOT_FOUND,
        "not_found",
        "Route inexistante",
        "aucune route ne correspond à ce chemin",
    )
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
        // Cache du préflight côté navigateur : sans `Access-Control-Max-Age`,
        // les navigateurs re-préflightent ~toutes les 5 s (audit 2026-08).
        .max_age(std::time::Duration::from_secs(3600))
}
