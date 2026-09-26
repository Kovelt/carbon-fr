//! # carbonfr-server — composition root
//!
//! Le seul composant qui connaît les implémentations concrètes des ports et les
//! assemble (ADR-0002). Modes selon la sous-commande :
//!
//! - (aucune) : sert l'API et lance le **poller** (temps réel) ; calibre les
//!   intervalles de prévision au démarrage (ADR-0011).
//! - `backfill` : rapatrie l'historique par **export de masse** (ADR-0003),
//!   puis s'arrête.
//! - `backtest` : évalue `climatology@1` (walk-forward) — MAE/RMSE modèle vs
//!   persistance (ADR-0009).
//! - `backtest-acv` : évalue la prévision `acv-ademe@2` sur vérité dérivée
//!   (ADR-0013).
//! - `backtest-sweep` : balaie une grille N × τ, classe par RMSE.
//! - `backtest-bands` : calibre et imprime les bandes d'incertitude par horizon
//!   (ADR-0011).
//! - `backtest-renewable` : backtest de la dérivation météo → production
//!   renouvelable (ADR-0018).
//! - `analyze-renewable-signal` : gate ADR-0018 (étape A) — l'anomalie de
//!   renouvelable améliore-t-elle la climatologie d'intensité ?
//! - `backtest-share` : gate de `share-clim@1` — part renouvelable prévue du
//!   pilier rfnbo (ADR-0028).
//! - `backtest-share-meteo` : gate de `share-meteo@2` — part renouvelable
//!   météo-pilotée (addendum ADR-0028 ; mesurée, non servie).
//! - `train` : entraîne le modèle ML GBDT (ADR-0012) → artefact, et compare
//!   `gbdt@1` à `climatology@1` au backtest (garde de promotion).
//! - `mint-key` : délivre une clé API tier gratuit (ADR-0015) — stocke son
//!   empreinte, affiche la clé une seule fois.
//! - `list-keys` : liste les clés API (empreinte, tier, création, abonnements
//!   webhook, libellé) — jamais la clé en clair, qui n'est pas stockée.
//! - `revoke-key` : révoque une clé (`CARBONFR_REVOKE_KEY` = clé `cfr_…` ou son
//!   empreinte) et supprime ses abonnements webhook (ADR-0015, addendum 2026-09).
//! - `--version` / `-V` : imprime la version du build et sort (ADR-0019).
//!
//! ## Configuration (variables d'environnement)
//!
//! | Variable                     | Défaut         | Rôle                              |
//! |------------------------------|----------------|-----------------------------------|
//! | `DATABASE_URL`               | — (requis)     | DSN PostgreSQL                    |
//! | `CARBONFR_BIND`              | `0.0.0.0:8080` | adresse d'écoute de l'API         |
//! | `CARBONFR_POLL_SECS`         | `900` (15 min) | période d'ingestion ODRÉ (et TTL des caches de prévision), > 0 |
//! | `CARBONFR_POLL_WINDOW_HOURS` | `3`            | largeur de la fenêtre glissante relue à chaque cycle (`IngestRecent`, rattrape un retard de publication ODRÉ sans appel supplémentaire), > 0, ADR-0003 addendum 2026-09-25 |
//! | `CARBONFR_ENTSOE_TOKEN`      | (non défini)   | active l'ingestion ENTSO-E (imports `acv-ademe@2` + prix spot `/v1/price`) |
//! | `CARBONFR_ENTSOE_BASE_URL`   | `https://web-api.tp.entsoe.eu/api` | endpoint de l'API ENTSO-E |
//! | `CARBONFR_ENTSOE_WINDOW_HOURS` | `6`          | fenêtre récente interrogée par cycle de poll |
//! | `CARBONFR_BACKFILL_FROM`     | `2012-01-01T00:00:00Z` | début du backfill (RFC 3339) |
//! | `CARBONFR_BACKFILL_TO`       | maintenant     | fin du backfill (RFC 3339)        |
//! | `CARBONFR_BACKFILL_WINDOW_DAYS` | `90`        | largeur de tranche d'export       |
//! | `CARBONFR_BACKFILL_SOURCE`   | `consolidated` | jeu exporté : `consolidated` (consolidé/définitif) ou `realtime` (temps réel, pour un trou récent pas encore consolidé) |
//! | `CARBONFR_BACKFILL_SCOPE`    | `national`     | périmètre du backfill : `national` (inchangé), `regional` (mix `acv-ademe` des 12 régions, item PROD-1) ou `all` (les deux) — ADR-0003 addendum 2026-09-26 |
//! | `CARBONFR_BACKTEST_FROM`/`_TO` | 30 derniers jours | fenêtre de test (RFC 3339)   |
//! | `CARBONFR_BACKTEST_REGION`   | `national`     | région évaluée (slug)             |
//! | `CARBONFR_BACKTEST_METHODOLOGY` | `rte-direct` | méthodologie évaluée             |
//! | `CARBONFR_BACKTEST_ORIGIN_STEP_HOURS` | `24`  | espacement des origines           |
//! | `CARBONFR_BACKTEST_STEP_MINUTES` | `15`       | pas natif (30 pour le jeu consolidé) |
//! | `CARBONFR_BACKTEST_WEEKS`/`_TAU_HOURS` | `4,6,8,10,12` × `3,6,12,24` (sweep) | `backtest-sweep` : grilles N × τ balayées ; `backtest` : le 1er élément de chaque liste surcharge le couple calé (N=10, τ=336 h) |
//! | `CARBONFR_BACKTEST_HORIZON_HOURS` | `24`      | `backtest-bands` : horizon calibré |
//! | `CARBONFR_BACKTEST_BAND_QUANTILE` | `0.1`     | `backtest-bands` : quantile de bord |
//! | `CARBONFR_FORECAST_CALIBRATE_WEEKS` | `8`     | auto-calibration au démarrage (0 = off) |
//! | `CARBONFR_RENEWABLE_CALIBRATE_WEEKS` | `52`   | calibration `/v1/renewable` au démarrage (0 = off) |
//! | `CARBONFR_SHARE_CALIBRATE_WEEKS` | `8`        | bandes `share-clim@1` au démarrage (0 = off, ADR-0028) |
//! | `CARBONFR_SHARE_CALIBRATE_TO` | maintenant    | fin de calibration `share-clim@1` (RFC 3339, repro/dev) |
//! | `CARBONFR_TRAIN_FROM`/`_TO`  | 120 j av. test | `train` : période d'entraînement   |
//! | `CARBONFR_TRAIN_ORIGIN_STEP_HOURS` | `6`      | `train` : espacement des origines  |
//! | `CARBONFR_GBDT_MODEL`        | `gbdt.model`   | `train` : chemin de l'artefact GBDT |
//! | `CARBONFR_RATELIMIT_ENABLED` | `0` (off)      | tier hébergé : auth+quota (ADR-0015) |
//! | `CARBONFR_RATELIMIT_ANON_PER_MIN` | `60`      | quota anonyme (req/min)            |
//! | `CARBONFR_RATELIMIT_FREE_PER_MIN` | `600`     | quota clé gratuite (req/min)        |
//! | `CARBONFR_KEY_LABEL`         | `` (vide)      | `mint-key` : libellé de la clé      |
//! | `CARBONFR_REVOKE_KEY`        | — (requis par `revoke-key`) | `revoke-key` : clé `cfr_…` ou son empreinte (64 hex, cf. `list-keys`) |
//! | `CARBONFR_WEBHOOK_MAX_FAILURES` | `10`        | livraisons webhook échouées **consécutives** avant désactivation de l'abonnement (> 0, ADR-0016) |
//! | `CARBONFR_WEBHOOK_PURGE_DAYS` | `30`         | délai avant suppression d'un abonnement **désactivé** (> 0, ADR-0016 addendum « purge ») ; purge au démarrage puis toutes les 6 h |
//! | `CARBONFR_SELF_HEAL_DAYS`    | `7`            | auto-réparation quotidienne : réimport (export de masse, jeu temps réel) des N derniers jours nationaux, 10 min après le démarrage puis toutes les 24 h (0 = désactivée, ≤ 7 = fenêtre de recalcul des rollups ; ADR-0003 addendum 2026-09-25) |
//! | `CARBONFR_SELF_HEAL_REGIONAL` | `0` (off)     | étend l'auto-réparation quotidienne au régional (même fenêtre, +1 export/jour sur le jeu régional temps réel) — item PERF-3, **à activer seulement une fois le quota ODRÉ réel visible** (PROD-3, jauges `carbonfr_odre_quota_*`) ; ADR-0003 addendum 2026-09-26 |
//! | `CARBONFR_TRUST_PROXY`       | `0` (off)      | faire confiance à `X-Forwarded-For` (derrière un reverse proxy) |
//! | `CARBONFR_REAL_IP_HEADER`    | (non défini)   | en-tête d'IP réelle dédié (ex. `x-real-ip`) — **uniquement** si le proxy l'écrase systématiquement ; défaut = dernier segment de `X-Forwarded-For` (audit 2026-08) |
//! | `CARBONFR_DB_MAX_CONNECTIONS` | `20`         | taille du pool PostgreSQL           |
//! | `CARBONFR_DB_STATEMENT_TIMEOUT_MS` | `30000` | borne serveur `statement_timeout` + `idle_in_transaction_session_timeout` |
//! | `CARBONFR_VISIT_SALT`        | `carbon-fr` (⚠ requis si TRUST_PROXY) | sel du hachage des IP visiteurs |
//! | `CARBONFR_LOG_FORMAT`        | (texte)        | `json` pour des logs structurés (prod) |
//! | `RUST_LOG`                   | `info`         | filtre de logs (`tracing`)        |

mod metrics;

use std::net::SocketAddr;

use anyhow::Context;
use carbonfr_adapter_entsoe::{EntsoeClient, EntsoeError};
use carbonfr_adapter_forecast::{AcvAdemeForecaster, CachedForecaster, ClimatologyForecaster};
use carbonfr_adapter_gbdt::{
    GbdtForecaster, GbdtHyperParams, build_training_examples, train_model,
};
use carbonfr_adapter_http::{
    AppState, AuthConfig, AuthState, EligibilityRepoAdapter, ForecastState, ShareForecastConfig,
    StreamState, key_fingerprint, router,
};
use carbonfr_adapter_meteo::OpenMeteoClient;
use carbonfr_adapter_odre::quota::{DatasetQuota, QuotaTracker};
use carbonfr_adapter_odre::{ArchiveSource, OdreClient};
use carbonfr_adapter_postgres::PgIntensityRepository;
use carbonfr_adapter_webhook::HttpNotifier;
use carbonfr_core::application::{
    AnalyzeRenewableSignal, BackfillHistory, BacktestConsumptionForecast, BacktestForecast,
    BacktestRenewable, BacktestReport, CalibrateRenewable, INGEST_RECENT_DEFAULT_WINDOW,
    IngestRecent,
};
use carbonfr_core::domain::{
    ACV_FORECAST_ID, ACV_FORECAST_VERSION, CLIMATOLOGY_ID, CLIMATOLOGY_VERSION, ClimatologyParams,
    DEFAULT_WEBHOOK_MAX_CONSECUTIVE_FAILURES, DEFAULT_WEBHOOK_PURGE_DAYS, ErrorMetrics,
    IntensityUpdate, Region, TimeRange, WeatherForecast, hmac_sha256_hex, render_webhook_payload,
    should_fire,
};
use carbonfr_core::ports::{
    ApiKeyRepository, ApiTier, ConsumptionRepository, ConsumptionSource, CrossBorderRepository,
    CrossBorderSource, Eco2mixArchive, Eco2mixSource, IntensityRepository, Notifier,
    SpotPriceRepository, SpotPriceSource, SubscriptionRepository, WeatherForecastSource,
    WeatherRepository, WebhookDelivery,
};
use metrics::{Metrics, QuotaGauge, render_odre_quota};
use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, Month, OffsetDateTime};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Bootstrap : provider crypto `ring` de rustls installé comme provider par
    // défaut du **processus**, avant tout `reqwest::Client` (adapters ODRÉ,
    // ENTSO-E, météo, webhook) — reqwest 0.13 (feature `rustls-no-provider`,
    // cf. Cargo.toml racine) ne tire plus `aws-lc-rs` : sans provider installé,
    // la construction du moindre client PANIQUE, même pour un usage HTTP en
    // clair (la pile TLS est montée dès `.build()`). Un seul provider dans tout
    // le workspace (ADR-0031 décision 3 : pas de double provider) → `ring`,
    // déjà celui de sqlx (`tls-rustls-ring`). C'est le tout premier appel de
    // `main()`, avant tout autre code : `.expect()` est du bootstrap légitime
    // (CONTRIBUTING.md) — un échec ici ne peut signifier qu'un provider a déjà
    // été installé plus tôt dans le process, ce qui ne devrait jamais arriver.
    // Chaque adapter réinstalle aussi, défensivement et silencieusement, le
    // même provider avant son propre premier client (utile hors de ce binaire :
    // tests d'un adapter en isolation, usage de la crate sans passer par
    // `main()`) — cf. `ensure_crypto_provider` dans chacun d'eux.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("provider crypto rustls déjà installé avant même le début de main()");

    let arg = std::env::args().nth(1);

    // `--version` : répond et sort, sans bruit de logs (ADR-0019 — traçabilité du
    // build déployé). La version vient du workspace (`version.workspace = true`).
    if matches!(arg.as_deref(), Some("--version" | "-V")) {
        println!("carbonfr-server {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    init_tracing();
    // Annonce la version au démarrage de **tous** les modes (service, backfill,
    // backtest…) : on sait quel build répond / a produit un résultat (ADR-0019).
    info!(version = env!("CARGO_PKG_VERSION"), "carbonfr-server");

    match arg.as_deref() {
        None => run_server().await,
        Some("backfill") => run_backfill().await,
        Some("backtest") => run_backtest().await,
        Some("backtest-acv") => run_backtest_acv().await,
        Some("backtest-sweep") => run_backtest_sweep().await,
        Some("backtest-renewable") => run_backtest_renewable().await,
        Some("analyze-renewable-signal") => run_analyze_renewable_signal().await,
        Some("backtest-bands") => run_backtest_bands().await,
        Some("backtest-share") => run_backtest_share().await,
        Some("backtest-share-meteo") => run_backtest_share_meteo().await,
        Some("train") => run_train().await,
        Some("mint-key") => run_mint_key().await,
        Some("list-keys") => run_list_keys().await,
        Some("revoke-key") => run_revoke_key().await,
        Some(other) => {
            anyhow::bail!(
                "sous-commande inconnue : « {other} » (attendu : `backfill`, `backtest`, `backtest-acv`, `backtest-sweep`, `backtest-bands`, `backtest-renewable`, `analyze-renewable-signal`, `backtest-share`, `backtest-share-meteo`, `train`, `mint-key`, `list-keys`, `revoke-key`, ou aucune pour servir l'API)"
            )
        }
    }
}

/// Mode service : poller temps réel + API HTTP.
async fn run_server() -> anyhow::Result<()> {
    let config = ServerConfig::from_env()?;

    let repo = connect_repo(&config.database_url).await?;

    // Quota ODRÉ réel (ADR-0022 addendum 2026-09-26, PROD-3) : un traceur unique
    // pour le processus serveur, partagé par les DEUX clients ODRÉ qu'il
    // construit (poller + auto-réparation) — pas celui de la sous-commande
    // `backfill`, un processus séparé sans `/metrics`. Observation opportuniste
    // (en-têtes déjà reçus), 0 appel ODRÉ supplémentaire.
    let odre_quota = QuotaTracker::new();

    // Poller unique : un seul composant tape les sources amont, l'API sert
    // depuis la base. ODRÉ (intensité + charge) et Open-Meteo (prévision météo).
    let source = OdreClient::new()
        .context("initialisation du client ODRÉ")?
        .with_quota_tracker(odre_quota.clone());
    let weather = OpenMeteoClient::new().context("initialisation du client Open-Meteo")?;
    // ENTSO-E : optionnel (ADR-0010) — seulement si `CARBONFR_ENTSOE_TOKEN` est
    // défini. Sans token, `acv-ademe@2` reste calculable mais sans donnée d'import.
    let cross_border = match EntsoeClient::from_env() {
        Ok(client) => {
            info!("source d'import ENTSO-E configurée (acv-ademe@2 alimentée)");
            Some(client)
        }
        // Seule l'absence de configuration est tolérée : un client qui ne peut
        // pas se construire (TLS, certificats) arrête le démarrage.
        Err(err @ EntsoeError::Config(_)) => {
            info!(raison = %err, "ENTSO-E non configuré : acv-ademe@2 sans contexte d'import");
            None
        }
        Err(err) => return Err(anyhow::Error::new(err).context("initialisation du client ENTSO-E")),
    };
    // Canal de diffusion live (ADR-0014 §2) : le poller publie chaque mise à jour
    // nationale, les connexions SSE s'y abonnent. Canal mémoire (poller intégré) ;
    // pour un bin/poller séparé (ADR-0007), basculer sur LISTEN/NOTIFY.
    let (updates_tx, _) = tokio::sync::broadcast::channel(64);
    // Métriques d'exploitation (Prometheus `/metrics`) : le poller les alimente,
    // le handler les rend. `build_info` porte la version du binaire (ADR-0019).
    let metrics = Metrics::new(env!("CARGO_PKG_VERSION"));
    let poller = spawn_poller(
        source,
        weather,
        cross_border,
        repo.clone(),
        updates_tx.clone(),
        config.poll_interval,
        config.poll_window,
        metrics.clone(),
    );

    // Watcher de webhooks (ADR-0016) : s'abonne au même flux que le SSE, détecte
    // les franchissements de seuil et livre des notifications signées.
    let webhook_watcher = spawn_webhook_watcher(
        updates_tx.subscribe(),
        repo.clone(),
        HttpNotifier::new().context("initialisation du client webhook (anti-SSRF, ADR-0016)")?,
        config.webhook_max_failures,
    );

    // Purge des abonnements webhook désactivés (ADR-0016 addendum « purge ») :
    // même repository, tâche indépendante du watcher (fréquence bien plus
    // basse — 6 h contre chaque mise à jour du flux).
    let webhook_purge = spawn_webhook_purge(repo.clone(), config.webhook_purge_days);

    // Auto-réparation quotidienne de la collecte (ADR-0003 addendum 2026-09-25) :
    // un export de masse du jeu temps réel par jour comble les trous laissés par
    // une panne de la source.
    let self_heal = spawn_self_heal(
        OdreClient::new()
            .context("initialisation du client ODRÉ (auto-réparation)")?
            .with_archive_source(ArchiveSource::Realtime)
            .with_quota_tracker(odre_quota.clone()),
        repo.clone(),
        config.self_heal_days,
        config.self_heal_regional,
    );

    // Prévision (ADR-0009) : modèle climatology@1 alimenté par le même
    // repository. Intervalles **calibrés** au démarrage par quantiles de résidus
    // par horizon (ADR-0011), repli sur la dispersion par créneau si l'historique
    // récent est insuffisant. Son identité versionnée est annoncée au client.
    // Les **quatre calibrations de démarrage** (intervalles climatology@1 et
    // acv-ademe@2, bandes share-clim@1, modèle renouvelable) sont indépendantes
    // — elles ne partagent que le pool sqlx — et s'exécutent **en parallèle**
    // (audit 2026-08) : empilées en séquence, leurs timeouts individuels de
    // 120 s se cumulaient (jusqu'à ~8 min sans écoute HTTP, `/health` compris,
    // sur base dégradée), trahissant l'intention de `CALIBRATION_TIMEOUT` de
    // borner le temps de boot.
    let (climatology, acv, share_config, renewable_model) = tokio::join!(
        build_calibrated_forecaster(repo.clone()),
        build_calibrated_acv_forecaster(repo.clone()),
        build_share_forecast_config(repo.clone(), config.poll_interval),
        build_calibrated_renewable_model(repo.clone()),
    );
    // Cache TTL des séries prévues (audit perf 2026-08) : la donnée ne change
    // qu'au cycle du poller — sans cache, chaque requête des 5 endpoints de
    // prévision relisait ~10 semaines d'historique et rebâtissait le modèle.
    let forecaster = CachedForecaster::new(climatology, config.poll_interval);
    let model = format!("{CLIMATOLOGY_ID}@{CLIMATOLOGY_VERSION}");
    // Prévision `acv-ademe@2` (ADR-0013) : climatologie des entrées (mix + import)
    // + calculateur. Servie via `?methodology=acv-ademe&version=2`. Même cache.
    let acv_forecaster = CachedForecaster::new(acv, config.poll_interval);
    let acv_model = format!("{ACV_FORECAST_ID}@{ACV_FORECAST_VERSION}");
    let forecast_state = ForecastState::new(forecaster, model)
        .with_consumption(std::sync::Arc::new(acv_forecaster), acv_model)
        // Overlay d'éligibilité électrolyseur (ADR-0025/0026) : le repo PostgreSQL
        // fournit le mix nowcast (rte-direct) + le prix spot day-ahead.
        .with_eligibility(std::sync::Arc::new(EligibilityRepoAdapter(repo.clone())));
    // `share-clim@1` (ADR-0028) : part renouvelable prévue du pilier rfnbo,
    // seulement si les bandes ont pu être calibrées au démarrage (sinon la part
    // future reste `Indeterminate` — comportement d'avant ADR-0028, gate oblige).
    let forecast_state = match share_config {
        Some(cfg) => forecast_state.with_share_forecast(std::sync::Arc::new(cfg)),
        None => forecast_state,
    };

    let mut state = AppState::new(repo.clone())
        .with_trust_proxy(config.trust_proxy)
        .with_real_ip_header(config.real_ip_header.clone())
        .with_renewable_model(renewable_model);
    if let Some(salt) = config.visit_salt {
        state = state.with_visit_salt(salt);
    }
    // Arrêt gracieux **borné** (audit 2026-08) : un jeton partagé, annulé au
    // signal, (1) déclenche `with_graceful_shutdown`, (2) **clôt les flux SSE**
    // via `StreamState` — sans quoi `serve` attendait indéfiniment ces
    // connexions infinies (le `Sender` broadcast vit dans l'app) et le
    // superviseur finissait par SIGKILL à chaque déploiement dès qu'un client
    // `/v1/intensity/stream` (ou un onglet `/hydrogene`) restait ouvert — et
    // (3) arme le délai de grâce qui force la sortie si le drain traîne.
    let shutdown = tokio_util::sync::CancellationToken::new();
    {
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            shutdown.cancel();
        });
    }
    let stream_state = StreamState::new(updates_tx).with_shutdown(shutdown.clone());
    // `/metrics` (hors contrat `/v1`, comme `/health`) : exposition Prometheus en
    // texte, pas du JSON versionné → fusionnée ici plutôt que dans le routeur de
    // l'adapter. En prod, restreindre l'accès au scrapeur côté reverse proxy.
    // État à deux champs (`Clone` dérivé) : les compteurs/jauges faits main
    // (`Metrics`) + le traceur de quota ODRÉ réel (`QuotaTracker`), tous deux
    // bon marché à cloner (`Arc` interne) — cf. `MetricsState`.
    let metrics_router = axum::Router::new()
        .route("/metrics", axum::routing::get(serve_metrics))
        .with_state(MetricsState {
            metrics,
            quota: odre_quota,
        });
    // Tier hébergé (ADR-0015) : middleware clés API + quota, **opt-in**. Désactivé
    // par défaut → l'API reste anonyme et sans limite (parité self-hosting).
    // Appliqué PAR le routeur, sous sa couche CORS (audit 2026-08 : posé ici en
    // `.layer()`, il devenait la couche la plus externe → préflights décomptés
    // du quota et 401/429 sans en-têtes CORS, illisibles en navigateur).
    let auth_state = build_auth_state(repo.clone());
    if auth_state.is_some() {
        info!("tier hébergé activé : auth par clé + quota par minute");
    }
    let app = router(state, forecast_state, stream_state, auth_state).merge(metrics_router);
    let listener = TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("écoute sur {}", config.bind))?;
    info!(addr = %config.bind, "API à l'écoute");

    // Supervision **fail-fast** : le serveur s'arrête sur signal (arrêt gracieux) ;
    // mais si le poller, le watcher ou la purge meurt (panique → boucle infinie
    // terminée), on sort en erreur plutôt que de continuer en silence (donnée
    // gelée / webhooks muets / quota qui ne se libère plus). Le superviseur
    // (systemd `Restart=on-failure`) relance.
    let mut poller = poller;
    let mut webhook_watcher = webhook_watcher;
    let mut webhook_purge = webhook_purge;
    let mut self_heal = self_heal;
    let serve = {
        let shutdown = shutdown.clone();
        async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
        }
    };
    tokio::pin!(serve);

    let serve_result = tokio::select! {
        result = &mut serve => result.context("serveur HTTP"),
        joined = &mut poller => Err(anyhow::anyhow!(
            "le poller s'est arrêté ({joined:?}) — l'ingestion est interrompue"
        )),
        joined = &mut webhook_watcher => Err(anyhow::anyhow!(
            "le watcher de webhooks s'est arrêté ({joined:?})"
        )),
        joined = &mut webhook_purge => Err(anyhow::anyhow!(
            "la purge des abonnements webhook désactivés s'est arrêtée ({joined:?})"
        )),
        joined = &mut self_heal => Err(anyhow::anyhow!(
            "l'auto-réparation de la collecte s'est arrêtée ({joined:?})"
        )),
        // Filet de sécurité (audit 2026-08) : si une connexion refuse de se
        // drainer malgré la clôture des flux SSE, on force la sortie après le
        // délai de grâce plutôt que d'attendre le SIGKILL du superviseur.
        _ = async {
            shutdown.cancelled().await;
            tokio::time::sleep(SHUTDOWN_GRACE).await;
        } => {
            warn!(
                grace_secs = SHUTDOWN_GRACE.as_secs(),
                "arrêt gracieux non terminé dans le délai de grâce — sortie forcée"
            );
            Ok(())
        }
    };

    poller.abort();
    webhook_watcher.abort();
    webhook_purge.abort();
    self_heal.abort();
    serve_result
}

/// État du routeur `/metrics` : les compteurs/jauges maison (`Metrics`) et le
/// traceur de quota ODRÉ réel (`QuotaTracker`, ADR-0022 addendum 2026-09-26).
/// `Clone` (dérivé) : les deux champs sont bon marché à cloner (`Arc` interne).
#[derive(Clone)]
struct MetricsState {
    metrics: Metrics,
    quota: QuotaTracker,
}

/// `GET /metrics` — exposition Prometheus (text format 0.0.4). Hors du contrat
/// `/v1` (endpoint d'exploitation, comme `/health`). Rend d'abord les
/// métriques maison, puis les jauges de quota ODRÉ réel (vide si aucune
/// observation encore faite).
async fn serve_metrics(
    axum::extract::State(state): axum::extract::State<MetricsState>,
) -> impl axum::response::IntoResponse {
    let snapshot = state.quota.snapshot();
    let gauges: Vec<QuotaGauge<'_>> = snapshot
        .iter()
        .map(|q: &DatasetQuota| QuotaGauge {
            dataset: &q.dataset,
            limit: q.limit,
            remaining: q.remaining,
            reset_unix: q.reset_unix,
            observed_unix: q.observed_unix,
        })
        .collect();
    let body = state.metrics.render() + &render_odre_quota(&gauges);
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

/// Construit l'état du middleware d'auth/quota si le tier hébergé est **activé**
/// (`CARBONFR_RATELIMIT_ENABLED=1`), sinon `None` (mode anonyme par défaut,
/// ADR-0015 §6). Limites surchargeables par env.
fn build_auth_state(repo: PgIntensityRepository) -> Option<AuthState> {
    let enabled = matches!(
        std::env::var("CARBONFR_RATELIMIT_ENABLED").as_deref(),
        Ok("1") | Ok("true")
    );
    if !enabled {
        return None;
    }
    let env_u32 = |name: &str, default: u32| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let defaults = AuthConfig::default();
    let config = AuthConfig {
        anonymous_per_min: env_u32(
            "CARBONFR_RATELIMIT_ANON_PER_MIN",
            defaults.anonymous_per_min,
        ),
        free_per_min: env_u32("CARBONFR_RATELIMIT_FREE_PER_MIN", defaults.free_per_min),
        trust_proxy: matches!(
            std::env::var("CARBONFR_TRUST_PROXY").as_deref(),
            Ok("1") | Ok("true")
        ),
        real_ip_header: real_ip_header_from_env(),
    };
    let keys: std::sync::Arc<dyn ApiKeyRepository> = std::sync::Arc::new(repo);
    Some(AuthState::new(keys, config))
}

/// `CARBONFR_REAL_IP_HEADER` : en-tête d'IP réelle dédié, **opt-in** (audit
/// 2026-08). ⚠️ À ne configurer que si le reverse proxy **écrase**
/// systématiquement cet en-tête (ex. Caddy avec `header_up X-Real-IP
/// {remote_host}`) : sinon il est fourni par le client → quota contournable et
/// compteur de visiteurs pollué. Non défini (défaut) : dernier segment de
/// `X-Forwarded-For`, sûr avec tout proxy qui appende l'IP réelle à droite.
fn real_ip_header_from_env() -> Option<String> {
    std::env::var("CARBONFR_REAL_IP_HEADER")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
}

/// Mode `mint-key` : génère une clé API gratuite, en stocke l'empreinte, et
/// l'affiche **une seule fois** (ADR-0015). Libellé via `CARBONFR_KEY_LABEL`.
async fn run_mint_key() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let label = std::env::var("CARBONFR_KEY_LABEL").unwrap_or_default();

    let key = generate_api_key();
    let hash = key_fingerprint(&key);
    repo.insert_key(&hash, ApiTier::Free, &label)
        .await
        .context("enregistrement de la clé")?;

    // La clé en clair n'est jamais stockée ni re-affichable : on ne garde que
    // son empreinte. À transmettre une seule fois au porteur.
    println!("Clé API (tier gratuit) — à conserver, non ré-affichée :");
    println!("{key}");
    Ok(())
}

/// Mode `list-keys` : liste les clés enregistrées pour retrouver celle à
/// révoquer — empreinte, tier, date de création, abonnements webhook, libellé.
/// Jamais de clé en clair (elle n'est pas stockée) ; l'empreinte n'est pas un
/// secret (elle ne permet pas de s'authentifier).
async fn run_list_keys() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let keys = repo.list_keys().await.context("lecture des clés")?;
    if keys.is_empty() {
        println!("Aucune clé enregistrée.");
        return Ok(());
    }
    println!("empreinte\ttier\tcréée le\twebhooks\tlibellé");
    for key in keys {
        let tier = match key.tier {
            Some(ApiTier::Free) => "free",
            // `ApiTier` est `#[non_exhaustive]` (ADR-0030) : un palier pas
            // encore connu de cet outil CLI s'affiche comme un tier absent,
            // même libellé que `None` — jamais un succès trompeur.
            None | Some(_) => "inconnu",
        };
        let created = key
            .created_at
            .format(&Rfc3339)
            .unwrap_or_else(|_| key.created_at.to_string());
        println!(
            "{}\t{tier}\t{created}\t{}\t{}",
            key.key_hash, key.subscriptions, key.label
        );
    }
    Ok(())
}

/// Mode `revoke-key` : révoque la clé désignée par `CARBONFR_REVOKE_KEY` (clé en
/// clair `cfr_…`, ou son empreinte telle qu'affichée par `list-keys`) et
/// supprime ses abonnements webhook, atomiquement (ADR-0015, addendum 2026-09).
/// Échoue si aucune clé ne correspond. Une instance en cours peut encore
/// accepter la clé jusqu'à 60 s (cache positif de l'adapter HTTP).
async fn run_revoke_key() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let raw = std::env::var("CARBONFR_REVOKE_KEY").context(
        "la variable CARBONFR_REVOKE_KEY est requise (clé `cfr_…` ou son empreinte, cf. `list-keys`)",
    )?;
    let hash = revocation_target(&raw)?;
    let repo = connect_repo(&database_url).await?;
    match repo
        .revoke_key(&hash)
        .await
        .context("révocation de la clé")?
    {
        Some(revocation) => {
            println!(
                "Clé révoquée (empreinte {hash}) — {} abonnement(s) webhook supprimé(s). \
                 Les instances en cours peuvent encore l'accepter jusqu'à 60 s (cache).",
                revocation.subscriptions_removed
            );
            Ok(())
        }
        None => anyhow::bail!("aucune clé ne correspond à l'empreinte {hash} (cf. `list-keys`)"),
    }
}

/// Empreinte visée par `revoke-key` : la clé en clair (`cfr_…`, hachée ici comme
/// à la délivrance) ou directement son empreinte (SHA-256, 64 caractères hex).
fn revocation_target(raw: &str) -> anyhow::Result<String> {
    let raw = raw.trim();
    if raw.starts_with("cfr_") {
        return Ok(key_fingerprint(raw));
    }
    if raw.len() == 64 && raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(raw.to_ascii_lowercase());
    }
    anyhow::bail!(
        "CARBONFR_REVOKE_KEY doit être une clé `cfr_…` ou son empreinte (64 caractères hex, cf. `list-keys`)"
    )
}

/// Génère une clé aléatoire `cfr_<64 hex>` (32 octets, CSPRNG userspace `rand` —
/// pas d'I/O fichier synchrone, cohérent avec `random_hex`, audit F29).
fn generate_api_key() -> String {
    use rand::RngExt;
    let mut buf = [0u8; 32];
    rand::rng().fill(&mut buf);
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    format!("cfr_{hex}")
}

/// Mode backfill : rapatriement de l'historique (national et/ou régional selon
/// `CARBONFR_BACKFILL_SCOPE`, item PROD-1), puis arrêt.
async fn run_backfill() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let source = backfill_source()?;
    let scope = backfill_scope()?;
    let archive = OdreClient::new()
        .context("initialisation du client ODRÉ")?
        .with_archive_source(source);

    let (range, window) = backfill_params()?;
    let backfill = BackfillHistory::new(archive.clone(), repo.clone(), window);

    info!(from = %range.start(), to = %range.end(), window_days = window.whole_days(), source = ?source, scope = ?scope, "backfill historique démarré");

    if matches!(scope, BackfillScope::National | BackfillScope::All) {
        let report = backfill
            .execute(range)
            .await
            .context("backfill historique")?;
        info!(
            read = report.read,
            written = report.written,
            windows = report.windows,
            "backfill national terminé"
        );

        // Reconstruction COMPLÈTE des rollups après le backfill massif (il écrit des
        // seaux historiques arbitraires que l'incrémental récent du poller ne couvre pas).
        repo.rebuild_rollups()
            .await
            .context("reconstruction des rollups")?;
        info!("rollups reconstruits");

        // Backfill de la **charge réalisée** historique (consommation) — store de
        // charge réutilisable (features du futur modèle ML, ADR-0012). Les prévisions
        // de charge, elles, sont ingérées en continu par le poller.
        //
        // **Fenêtré** comme le national (un export de masse par tranche) : un export
        // unique sur tout l'historique dépasse le timeout du client HTTP → corps
        // tronqué (« error decoding response body »).
        let mut loads = Vec::new();
        let mut win_start = range.start();
        while win_start < range.end() {
            let win_end = (win_start + window).min(range.end());
            let Some(slice) = TimeRange::new(win_start, win_end) else {
                break;
            };
            let mut part = archive
                .export_national_loads(slice)
                .await
                .context("backfill de la charge")?;
            loads.append(&mut part);
            win_start = win_end;
        }
        let loads_written = repo
            .upsert_loads(&loads)
            .await
            .context("écriture de la charge")?;
        info!(loads = loads_written, "charge réalisée backfillée");

        // Backfill de la **prévision météo archivée** (ADR-0012) pour entraîner le
        // GBDT, par tranches de 30 j (limite raisonnable de l'API). `run_at =
        // valid_at − 24 h` (anti-fuite). Échec non bloquant (best-effort).
        //
        // L'API Historical Forecast d'Open-Meteo accepte des requêtes dès
        // 2016-01-01, mais les variables utilisées ici (`wind_speed_100m`,
        // `shortwave_radiation`) répondent tout-`null` sur **toute 2016** (données
        // réelles ~2017→, vérifié live — audit 2026-08 ; l'agrégation saute
        // désormais ces créneaux plutôt que fabriquer des 0,0). On borne donc le
        // départ à **2017-01-01** : sans ce garde-fou, 2012→2017 = ~61 tranches
        // inutiles (400 avant 2016, tout-`null` ensuite).
        let weather_min = OffsetDateTime::new_utc(
            time::Date::from_calendar_date(2017, time::Month::January, 1)
                .context("date plancher de l'archive météo")?,
            time::Time::MIDNIGHT,
        );
        let meteo = OpenMeteoClient::new().context("initialisation du client Open-Meteo")?;
        let mut weather_written = 0usize;
        let mut chunk_start = range.start().max(weather_min);
        while chunk_start < range.end() {
            let chunk_end = (chunk_start + Duration::days(30)).min(range.end());
            if let Some(chunk) = TimeRange::new(chunk_start, chunk_end) {
                match meteo.historical_forecast(chunk).await {
                    Ok(forecasts) => match repo.upsert_weather(&forecasts).await {
                        Ok(n) => weather_written += n,
                        Err(err) => warn!(error = %err, "échec d'écriture de la météo"),
                    },
                    Err(err) => warn!(error = %err, "échec d'archive météo (tranche ignorée)"),
                }
            }
            chunk_start = chunk_end;
        }
        info!(
            weather = weather_written,
            "prévisions météo archivées backfillées"
        );
    }

    if matches!(scope, BackfillScope::Regional | BackfillScope::All) {
        // Périmètre régional (item PROD-1) : mix `acv-ademe` des 12 régions
        // métropolitaines. **Pas** de backfill de charge ni de météo (entrées
        // nationales, cf. `ConsumptionRepository`/`WeatherRepository`).
        let report = backfill
            .execute_regional(range)
            .await
            .context("backfill historique régional")?;
        info!(
            read = report.read,
            written = report.written,
            windows = report.windows,
            "backfill régional terminé"
        );

        repo.rebuild_rollups()
            .await
            .context("reconstruction des rollups (régional)")?;
        info!("rollups reconstruits (régional)");
    }

    Ok(())
}

/// Horizons rapportés (ADR-0009).
const BACKTEST_CHECKPOINTS: [Duration; 3] =
    [Duration::hours(1), Duration::hours(6), Duration::hours(24)];

/// Configuration commune aux modes backtest, lue de l'environnement.
struct BacktestParams {
    region: Region,
    methodology: String,
    test: TimeRange,
    origin_step: Duration,
    /// Pas natif de la série évaluée : 15 min en temps réel, **30 min** pour le
    /// jeu consolidé/définitif éCO2mix (`CARBONFR_BACKTEST_STEP_MINUTES`).
    step: Duration,
}

impl BacktestParams {
    fn from_env() -> anyhow::Result<Self> {
        let region_slug =
            std::env::var("CARBONFR_BACKTEST_REGION").unwrap_or_else(|_| "national".to_string());
        let region = Region::from_slug(&region_slug).with_context(|| {
            format!("CARBONFR_BACKTEST_REGION : région inconnue « {region_slug} »")
        })?;
        let methodology = std::env::var("CARBONFR_BACKTEST_METHODOLOGY")
            .unwrap_or_else(|_| "rte-direct".to_string());

        let to = parse_rfc3339_env("CARBONFR_BACKTEST_TO")?.unwrap_or_else(OffsetDateTime::now_utc);
        let from = parse_rfc3339_env("CARBONFR_BACKTEST_FROM")?.unwrap_or(to - Duration::days(30));
        let test =
            TimeRange::new(from, to).context("fenêtre de backtest invalide (fin <= début)")?;

        let origin_step_hours = std::env::var("CARBONFR_BACKTEST_ORIGIN_STEP_HOURS")
            .ok()
            .map(|raw| raw.parse::<i64>())
            .transpose()
            .context("CARBONFR_BACKTEST_ORIGIN_STEP_HOURS : entier invalide")?
            .unwrap_or(24);
        anyhow::ensure!(
            origin_step_hours > 0,
            "CARBONFR_BACKTEST_ORIGIN_STEP_HOURS doit être > 0"
        );

        let step_minutes = std::env::var("CARBONFR_BACKTEST_STEP_MINUTES")
            .ok()
            .map(|raw| raw.parse::<i64>())
            .transpose()
            .context("CARBONFR_BACKTEST_STEP_MINUTES : entier invalide")?
            .unwrap_or(15);
        anyhow::ensure!(
            step_minutes > 0,
            "CARBONFR_BACKTEST_STEP_MINUTES doit être > 0"
        );

        Ok(Self {
            region,
            methodology,
            test,
            origin_step: Duration::hours(origin_step_hours),
            step: Duration::minutes(step_minutes),
        })
    }
}

/// Mode backtest : évalue `climatology@1` (paramètres par défaut) sur
/// l'historique (walk-forward), imprime MAE/RMSE (modèle vs persistance, global
/// et par horizon), puis arrête.
///
/// Configuration : `CARBONFR_BACKTEST_FROM`/`_TO` (RFC 3339 ; défaut 30 derniers
/// jours), `_REGION` (slug ; défaut `national`), `_METHODOLOGY` (défaut
/// `rte-direct`), `_ORIGIN_STEP_HOURS` (défaut 24).
async fn run_backtest() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let params = BacktestParams::from_env()?;
    let model = format!("{CLIMATOLOGY_ID}@{CLIMATOLOGY_VERSION}");

    info!(region = params.region.slug(), methodology = %params.methodology, model = %model, from = %params.test.start(), to = %params.test.end(), "backtest démarré");

    // Paramètres par défaut, sauf surcharge explicite (premier élément des
    // grilles) — utile pour inspecter le détail par horizon à un couple calé.
    let weeks = parse_u32_list("CARBONFR_BACKTEST_WEEKS", "")?;
    let taus = parse_u32_list("CARBONFR_BACKTEST_TAU_HOURS", "")?;
    let forecaster = match (weeks.first(), taus.first()) {
        (Some(&w), Some(&t)) => ClimatologyForecaster::with_config(
            repo.clone(),
            w,
            ClimatologyParams {
                step: params.step,
                tau: Duration::hours(t as i64),
            },
        ),
        _ => ClimatologyForecaster::new(repo.clone()),
    };
    let backtest = BacktestForecast::new(forecaster, repo, params.methodology.clone());
    let report = backtest
        .execute(
            params.region,
            params.test,
            params.origin_step,
            params.step,
            &BACKTEST_CHECKPOINTS,
        )
        .await
        .context("backtest")?;

    print_backtest_report(&model, params.region.slug(), &params.methodology, &report);
    Ok(())
}

/// Mode backtest **`acv-ademe@2`** (ADR-0013) : la vérité est dérivée de l'observé
/// (mix + contexte d'import), national uniquement.
async fn run_backtest_acv() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let params = BacktestParams::from_env()?;
    let model = format!("{ACV_FORECAST_ID}@{ACV_FORECAST_VERSION}");

    info!(model = %model, from = %params.test.start(), to = %params.test.end(), "backtest acv-ademe@2 démarré (vérité dérivée)");

    let forecaster = AcvAdemeForecaster::new(repo.clone(), repo.clone());
    let backtest = BacktestConsumptionForecast::new(forecaster, repo.clone(), repo);
    let report = backtest
        .execute(
            Region::National,
            params.test,
            params.origin_step,
            params.step,
            &BACKTEST_CHECKPOINTS,
        )
        .await
        .context("backtest acv-ademe")?;

    print_backtest_report(&model, "national", "acv-ademe@2", &report);
    Ok(())
}

/// Analyse-gate de la **prévision météo-pilotée** (ADR-0018, étape A) : mesure si
/// l'anomalie de renouvelable **réel** améliore la climatologie d'intensité (borne
/// supérieure). Si même le renouvelable parfait n'aide pas, la version prévue est
/// vaine — on ne construit `forecast@N` que si ce gate est franchi.
async fn run_analyze_renewable_signal() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let params = BacktestParams::from_env()?;

    info!(from = %params.test.start(), to = %params.test.end(), "analyse du signal renouvelable (upper-bound : renouvelable réel)");

    let report = AnalyzeRenewableSignal::new(repo)
        .execute(params.test)
        .await
        .context("analyse du signal renouvelable")?;

    info!(
        beta = (report.beta * 1000.0).round() / 1000.0,
        train = report.train,
        test = report.test,
        "coefficient calé (gCO2eq/kWh par MW au-dessus de la normale)"
    );
    info!(
        baseline_rmse = report.baseline.rmse.round(),
        adjusted_rmse = report.adjusted.rmse.round(),
        "RMSE intensité : climatologie seule vs climatologie + anomalie renouvelable"
    );
    info!(
        ameliore = report.improves(),
        gain_rmse = ((report.baseline.rmse - report.adjusted.rmse) * 100.0).round() / 100.0,
        "verdict : le renouvelable aide-t-il (hors échantillon) ?"
    );
    Ok(())
}

/// Backtest + **GATE** de `share-clim@1` (part renouvelable prévue, ADR-0028) :
/// walk-forward pur, vérité **dérivée** du mix observé (jamais stockée), modèle
/// vs persistance (MAE/RMSE, global + par horizon), et — avec les bandes
/// calibrées sur une fenêtre disjointe antérieure — comptage des verdicts fermes
/// au seuil Article 4 (0,90). **GO** = bat la persistance en RMSE global ET zéro
/// faux `pass` ferme. Même politique de promotion que `gbdt@1` : pas de GO, pas
/// de mise en service.
async fn run_backtest_share() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let params = BacktestParams::from_env()?;
    anyhow::ensure!(
        params.region == Region::National,
        "share-clim@1 est national-only (zone de dépôt FR, ADR-0026)"
    );

    let lookback = Duration::days(SHARE_LOOKBACK_DAYS);
    let horizon = Duration::hours(SHARE_MAX_HORIZON_HOURS);
    let calib_weeks = share_calibrate_weeks()?;
    anyhow::ensure!(
        calib_weeks > 0,
        "CARBONFR_SHARE_CALIBRATE_WEEKS doit être > 0 en backtest"
    );
    let calib_start = params.test.start() - Duration::days(calib_weeks * 7);

    info!(
        model = carbonfr_eligibility::SHARE_FORECAST_MODEL,
        from = %params.test.start(), to = %params.test.end(),
        calib_weeks, "backtest share-clim@1 démarré (vérité dérivée du mix)"
    );

    // Un SEUL chargement d'historique couvrant calibration + test + horizon.
    let full = TimeRange::new(
        calib_start - lookback,
        params.test.end() + horizon + params.step,
    )
    .context("fenêtre d'historique invalide")?;
    let history = repo
        .range(Region::National, &params.methodology, full)
        .await
        .context("lecture de l'historique de mix")?;
    anyhow::ensure!(
        !history.is_empty(),
        "aucun historique de mix sur la fenêtre demandée"
    );

    let clim_params = ClimatologyParams {
        step: params.step,
        tau: Duration::days(14),
    };
    // Bandes calibrées sur une fenêtre STRICTEMENT antérieure au test (anti-fuite).
    let calib = TimeRange::new(calib_start, params.test.start())
        .context("fenêtre de calibration invalide")?;
    let bands = carbonfr_eligibility::calibrate_share_bands(
        &history,
        calib,
        lookback,
        params.origin_step,
        clim_params,
        horizon,
        0.1,
    );
    if bands.is_none() {
        warn!("bandes non calibrées (fenêtre de calibration vide ?) — verdicts fermes non mesurés");
    }

    let checkpoints = [
        Duration::hours(1),
        Duration::hours(6),
        Duration::hours(24),
        Duration::hours(72),
    ];
    let report = carbonfr_eligibility::backtest_share(
        &history,
        params.test,
        lookback,
        params.origin_step,
        clim_params,
        &checkpoints,
        bands.as_ref(),
        0.90,
    )
    .context("backtest share-clim@1 : série inexploitable")?;

    let pts = |v: f64| (v * 10_000.0).round() / 10_000.0;
    info!(origins = report.origins, "origines évaluées (walk-forward)");
    if let (Some(m), Some(p)) = (&report.model, &report.persistence) {
        info!(
            model_mae = pts(m.mae),
            model_rmse = pts(m.rmse),
            persistence_mae = pts(p.mae),
            persistence_rmse = pts(p.rmse),
            n = m.n,
            "erreur GLOBALE de part renouvelable (fraction 0-1)"
        );
    }
    for h in &report.by_horizon {
        if let (Some(m), Some(p)) = (&h.model, &h.persistence) {
            info!(
                horizon_h = h.horizon.whole_hours(),
                model_rmse = pts(m.rmse),
                persistence_rmse = pts(p.rmse),
                n = m.n,
                "erreur par horizon"
            );
        }
    }
    info!(
        firm_pass = report.firm_pass,
        false_pass = report.false_pass,
        firm_fail = report.firm_fail,
        false_fail = report.false_fail,
        straddle = report.straddle,
        "verdicts fermes au seuil 0,90 (bandes calibrées, fenêtre disjointe)"
    );
    info!(
        go = report.passes_gate(),
        "GATE share-clim@1 : bat la persistance (RMSE global) ET zéro faux pass ?"
    );
    Ok(())
}

/// Backtest comparatif de l'**expérience** `share-meteo@2` (addendum ADR-0028) :
/// météo vs `share-clim@1` vs persistance sur les **mêmes** origines/cibles.
/// Modèle **non servi** — même politique de promotion que `gbdt@1` : le GATE
/// (battre `share-clim@1` en RMSE global, zéro faux `pass`) se mesure ici,
/// séparément du GATE de production de `backtest-share`.
async fn run_backtest_share_meteo() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let params = BacktestParams::from_env()?;
    anyhow::ensure!(
        params.region == Region::National,
        "share-meteo@2 est national-only (zone de dépôt FR, ADR-0026)"
    );

    let lookback = Duration::days(SHARE_LOOKBACK_DAYS);
    let horizon = Duration::hours(SHARE_MAX_HORIZON_HOURS);
    let calib_weeks = share_calibrate_weeks()?;
    anyhow::ensure!(
        calib_weeks > 0,
        "CARBONFR_SHARE_CALIBRATE_WEEKS doit être > 0 en backtest"
    );
    let calib_start = params.test.start() - Duration::days(calib_weeks * 7);

    info!(
        model = carbonfr_eligibility::SHARE_METEO_MODEL,
        from = %params.test.start(), to = %params.test.end(),
        calib_weeks, "backtest share-meteo@2 démarré (comparaison à trois, vérité dérivée du mix)"
    );

    let full = TimeRange::new(
        calib_start - lookback,
        params.test.end() + horizon + params.step,
    )
    .context("fenêtre d'historique invalide")?;
    let history = repo
        .range(Region::National, &params.methodology, full)
        .await
        .context("lecture de l'historique de mix")?;
    anyhow::ensure!(
        !history.is_empty(),
        "aucun historique de mix sur la fenêtre demandée"
    );
    let weather_rows = repo
        .weather_range(full)
        .await
        .context("lecture de l'historique météo")?;
    anyhow::ensure!(
        !weather_rows.is_empty(),
        "aucun historique météo sur la fenêtre demandée (table weather_forecast)"
    );

    let clim_params = ClimatologyParams {
        step: params.step,
        tau: Duration::days(14),
    };
    let calib = TimeRange::new(calib_start, params.test.start())
        .context("fenêtre de calibration invalide")?;
    let index = carbonfr_eligibility::WeatherIndex::build(&weather_rows);
    let meteo_bands = carbonfr_eligibility::calibrate_share_meteo_bands(
        &history,
        &index,
        calib,
        lookback,
        params.origin_step,
        clim_params,
        horizon,
        0.1,
    );
    if meteo_bands.is_none() {
        warn!(
            "bandes share-meteo non calibrées (fenêtre de calibration vide ?) — verdicts fermes non mesurés"
        );
    }
    let checkpoints = [
        Duration::hours(1),
        Duration::hours(6),
        Duration::hours(24),
        Duration::hours(72),
    ];
    let report = carbonfr_eligibility::backtest_share_meteo(
        &history,
        &index,
        params.test,
        lookback,
        params.origin_step,
        clim_params,
        &checkpoints,
        meteo_bands.as_ref(),
        0.90,
    )
    .context("backtest share-meteo@2 : série inexploitable")?;

    let pts = |v: f64| (v * 10_000.0).round() / 10_000.0;
    info!(
        origins = report.origins,
        weather_driven = report.weather_driven,
        fallback = report.fallback,
        "origines évaluées (walk-forward ; repli = share-clim@1 hors couverture météo)"
    );
    if let (Some(m), Some(c), Some(p)) = (&report.meteo, &report.clim, &report.persistence) {
        info!(
            meteo_rmse = pts(m.rmse),
            clim_rmse = pts(c.rmse),
            persistence_rmse = pts(p.rmse),
            n = m.n,
            "erreur GLOBALE de part renouvelable (fraction 0-1)"
        );
    }
    for h in &report.by_horizon {
        if let (Some(m), Some(c)) = (&h.meteo, &h.clim) {
            info!(
                horizon_h = h.horizon.whole_hours(),
                meteo_rmse = pts(m.rmse),
                clim_rmse = pts(c.rmse),
                n = m.n,
                "erreur par horizon (météo vs climatologie)"
            );
        }
    }
    info!(
        firm_pass = report.firm_pass,
        false_pass = report.false_pass,
        firm_fail = report.firm_fail,
        false_fail = report.false_fail,
        straddle = report.straddle,
        "verdicts fermes share-meteo@2 au seuil 0,90"
    );
    info!(
        go = report.passes_gate(),
        "GATE share-meteo@2 : bat share-clim@1 (RMSE global) ET zéro faux pass ?"
    );
    Ok(())
}

/// Backtest de la **dérivation renouvelable** (ADR-0018) : production estimée
/// depuis la météo vs production réelle, calibrée puis testée hors échantillon,
/// comparée au baseline « moyenne ». N'est pas une prévision — on mesure d'abord
/// que la météo **explique** la production avant d'en faire un modèle servi.
async fn run_backtest_renewable() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let params = BacktestParams::from_env()?;

    info!(from = %params.test.start(), to = %params.test.end(), "backtest dérivation renouvelable démarré");

    let backtest = BacktestRenewable::new(repo.clone(), repo);
    let report = backtest
        .execute(params.test)
        .await
        .context("backtest renouvelable")?;

    info!(
        train = report.train,
        test = report.test,
        wind_capacity_mw = report.model.wind_capacity_mw.round(),
        solar_capacity_mw = report.model.solar_capacity_mw.round(),
        "modèle calibré"
    );
    info!(
        rmse = report.wind.rmse.round(),
        mae = report.wind.mae.round(),
        baseline_rmse = report.wind_baseline.rmse.round(),
        "éolien (MW) — modèle vs baseline"
    );
    info!(
        rmse = report.solar.rmse.round(),
        mae = report.solar.mae.round(),
        baseline_rmse = report.solar_baseline.rmse.round(),
        "solaire (MW) — modèle vs baseline"
    );
    info!(
        eolien = report.wind.rmse < report.wind_baseline.rmse,
        solaire = report.solar.rmse < report.solar_baseline.rmse,
        "verdict : la météo bat le baseline (RMSE) ?"
    );
    Ok(())
}

/// Construit le prévisionniste `acv-ademe@2` avec ses intervalles **auto-calibrés**
/// au démarrage (résidus de backtest, ADR-0013 §6), repli sur la dispersion par
/// créneau si l'historique récent (ou le contexte d'import) est insuffisant.
async fn build_calibrated_acv_forecaster(
    repo: PgIntensityRepository,
) -> AcvAdemeForecaster<PgIntensityRepository, PgIntensityRepository> {
    let base = AcvAdemeForecaster::new(repo.clone(), repo.clone());

    let weeks = std::env::var("CARBONFR_FORECAST_CALIBRATE_WEEKS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(8);
    if weeks <= 0 {
        return base;
    }
    let now = OffsetDateTime::now_utc();
    let Some(window) = TimeRange::new(now - Duration::weeks(weeks), now) else {
        return base;
    };

    let calibrator = BacktestConsumptionForecast::new(
        AcvAdemeForecaster::new(repo.clone(), repo.clone()),
        repo.clone(),
        repo,
    );
    let calibration = calibrator.calibrate_bands(
        Region::National,
        window,
        Duration::days(1),
        Duration::minutes(15),
        Duration::hours(24),
        0.1,
    );
    match tokio::time::timeout(CALIBRATION_TIMEOUT, calibration).await {
        Ok(Ok(bands)) if !bands.is_empty() => {
            info!(
                horizons = bands.len(),
                "intervalles acv-ademe@2 calibrés (résidus par horizon)"
            );
            base.with_bands(bands)
        }
        Ok(Ok(_)) => base,
        Ok(Err(err)) => {
            warn!(error = %err, "calibration acv-ademe@2 impossible — bandes par créneau");
            base
        }
        Err(_) => {
            warn!("calibration acv-ademe@2 : timeout au démarrage — bandes par créneau");
            base
        }
    }
}

/// Mode *sweep* : balaie une grille de paramètres (N semaines × τ heures),
/// classe par RMSE global, et recommande le meilleur couple. Sert au **calage
/// mesuré** de `climatology@1` (ADR-0009).
///
/// Grilles : `CARBONFR_BACKTEST_WEEKS` (défaut `4,6,8,10,12`),
/// `CARBONFR_BACKTEST_TAU_HOURS` (défaut `3,6,12,24`). Même fenêtre/région que
/// `backtest`.
async fn run_backtest_sweep() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let params = BacktestParams::from_env()?;

    let weeks_grid = parse_u32_list("CARBONFR_BACKTEST_WEEKS", "4,6,8,10,12")?;
    let tau_grid = parse_u32_list("CARBONFR_BACKTEST_TAU_HOURS", "3,6,12,24")?;
    anyhow::ensure!(
        !weeks_grid.is_empty() && !tau_grid.is_empty(),
        "les grilles N et τ ne doivent pas être vides"
    );

    info!(
        region = params.region.slug(),
        methodology = %params.methodology,
        from = %params.test.start(),
        to = %params.test.end(),
        combos = weeks_grid.len() * tau_grid.len(),
        "sweep de backtest démarré"
    );

    println!();
    println!(
        "Sweep climatology — région {}, méthodologie {}",
        params.region.slug(),
        params.methodology
    );
    println!("Fenêtre {} → {}", params.test.start(), params.test.end());
    println!();
    println!(
        "{:>7} {:>7} {:>10} {:>10} {:>9}",
        "N(sem)", "τ(h)", "MAE", "RMSE", "n"
    );

    let mut best: Option<(u32, u32, f64)> = None; // (semaines, τ heures, RMSE)
    let mut persistence: Option<ErrorMetrics> = None;

    for &weeks in &weeks_grid {
        for &tau_hours in &tau_grid {
            let forecaster = ClimatologyForecaster::with_config(
                repo.clone(),
                weeks,
                ClimatologyParams {
                    step: params.step,
                    tau: Duration::hours(tau_hours as i64),
                },
            );
            let backtest =
                BacktestForecast::new(forecaster, repo.clone(), params.methodology.clone());
            let report = backtest
                .execute(
                    params.region,
                    params.test,
                    params.origin_step,
                    params.step,
                    &BACKTEST_CHECKPOINTS,
                )
                .await
                .context("backtest (combinaison)")?;

            persistence = persistence.or(report.persistence);
            match report.model {
                Some(m) => {
                    println!(
                        "{weeks:>7} {tau_hours:>7} {:>10.2} {:>10.2} {:>9}",
                        m.mae, m.rmse, m.n
                    );
                    if best.is_none_or(|(_, _, rmse)| m.rmse < rmse) {
                        best = Some((weeks, tau_hours, m.rmse));
                    }
                }
                None => println!("{weeks:>7} {tau_hours:>7} {:>10} {:>10} {:>9}", "—", "—", 0),
            }
        }
    }

    println!();
    if let Some(p) = persistence {
        println!(
            "Référence persistance : MAE {:.2}, RMSE {:.2} (n = {})",
            p.mae, p.rmse, p.n
        );
    }
    match best {
        Some((weeks, tau, rmse)) => println!(
            "Meilleur (RMSE) : N = {weeks} semaines, τ = {tau} h  →  RMSE {rmse:.2} gCO₂eq/kWh"
        ),
        None => println!("Aucune combinaison n'a produit de métriques (historique insuffisant ?)."),
    }
    Ok(())
}

/// Parse une liste d'entiers séparés par des virgules depuis l'environnement.
/// Une valeur vide (absente et `default` vide) donne une liste vide.
fn parse_u32_list(name: &str, default: &str) -> anyhow::Result<Vec<u32>> {
    let raw = std::env::var(name).unwrap_or_else(|_| default.to_string());
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    raw.split(',')
        .map(|item| item.trim().parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("{name} : liste d'entiers invalide (ex. « 4,6,8 »)"))
}

/// Imprime le rapport de backtest sous forme de tableau lisible (stdout).
fn print_backtest_report(model: &str, region: &str, methodology: &str, report: &BacktestReport) {
    println!();
    println!("Backtest {model} — région {region}, méthodologie {methodology}");
    println!("Origines évaluées : {}", report.origins);
    println!();
    println!("{:<20} {:>10} {:>10} {:>10}", "Série", "MAE", "RMSE", "n");
    print_metrics_row("global (modèle)", report.model);
    print_metrics_row("global (persist.)", report.persistence);
    for horizon in &report.by_horizon {
        let label = format!("h+{}", horizon.horizon.whole_hours());
        print_metrics_row(&format!("{label} (modèle)"), horizon.model);
        print_metrics_row(&format!("{label} (persist.)"), horizon.persistence);
    }
    println!();
    println!(
        "Unité : gCO₂eq/kWh. Plus bas = mieux ; le modèle n'a de valeur que s'il bat la persistance."
    );
}

fn print_metrics_row(label: &str, metrics: Option<ErrorMetrics>) {
    match metrics {
        Some(m) => println!("{label:<20} {:>10.2} {:>10.2} {:>10}", m.mae, m.rmse, m.n),
        None => println!("{label:<20} {:>10} {:>10} {:>10}", "—", "—", 0),
    }
}

/// Délai maximum d'une calibration au démarrage : borne le temps de boot si la
/// base est lente (gros historique, REFRESH concurrent, pool saturé) ; au-delà,
/// on démarre quand même en mode non-calibré plutôt que de pendre indéfiniment.
const CALIBRATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Délai de grâce de l'arrêt (audit 2026-08) : après le signal, temps laissé au
/// drain des connexions en vol (les flux SSE, eux, sont clos immédiatement via
/// le jeton d'arrêt) avant de **forcer** la sortie. Sous les bornes des
/// superviseurs (Docker `stop-timeout` 10 s, systemd `TimeoutStopSec=30` du
/// `deploy/carbonfr.service`) pour sortir proprement plutôt que par SIGKILL.
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(8);

/// Période de la purge des abonnements webhook désactivés (ADR-0016 addendum
/// « purge ») : les désactivations sont rares (quelques-unes par jour au
/// plus), 6 h suffisent largement sans justifier une planification plus fine.
const WEBHOOK_PURGE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// Auto-réparation de la collecte (ADR-0003 addendum 2026-09-25) : jours
/// réimportés par défaut, période et délai avant le premier passage (laisse le
/// démarrage — calibrations, premier cycle du poller — se faire d'abord).
const DEFAULT_SELF_HEAL_DAYS: u32 = 7;
const SELF_HEAL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
const SELF_HEAL_START_DELAY: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Profondeur de la climatologie de part renouvelable `share-clim@1`
/// (ADR-0028) : 10 semaines, alignée sur `climatology@1` (N calé, ADR-0009).
const SHARE_LOOKBACK_DAYS: i64 = 70;
/// Horizon **calibré** de `share-clim@1` : 72 h (le maximum de
/// `greenest-window?horizon_hours=`). Au-delà : `Indeterminate`.
const SHARE_MAX_HORIZON_HOURS: i64 = 72;

/// Fenêtre de calibration des bandes `share-clim@1` (`CARBONFR_SHARE_CALIBRATE_WEEKS`,
/// défaut 8 sem., `0` = opt-out → part future `Indeterminate` comme avant ADR-0028).
fn share_calibrate_weeks() -> anyhow::Result<i64> {
    std::env::var("CARBONFR_SHARE_CALIBRATE_WEEKS")
        .ok()
        .map(|raw| raw.parse::<i64>())
        .transpose()
        .context("CARBONFR_SHARE_CALIBRATE_WEEKS : entier invalide")
        .map(|v| v.unwrap_or(8))
}

/// Calibre les bandes de `share-clim@1` (part renouvelable **prévue** du pilier
/// `renewable-share`, ADR-0028) au démarrage — walk-forward pur sur l'historique
/// récent, quantiles de résidus par horizon (ADR-0011 §5). `None` (le pilier
/// reste `Indeterminate` au-delà du nowcast, comportement d'avant ADR-0028) si
/// l'historique est trop maigre, `…_WEEKS=0`, ou timeout.
async fn build_share_forecast_config(
    repo: PgIntensityRepository,
    cache_ttl: std::time::Duration,
) -> Option<ShareForecastConfig> {
    // Env invalide **tracée** (audit 2026-08) : avalée, la feature était coupée
    // (ou la date de calibration remplacée) en silence — indiscernable d'un
    // opt-out volontaire au journal.
    let weeks = match share_calibrate_weeks() {
        Ok(weeks) => weeks,
        Err(err) => {
            warn!(
                error = %err,
                "share-clim@1 : CARBONFR_SHARE_CALIBRATE_WEEKS invalide — calibration désactivée \
                 (part renouvelable future Indeterminate)"
            );
            return None;
        }
    };
    if weeks <= 0 {
        return None;
    }
    // Fin de la fenêtre de calibration : maintenant par défaut ;
    // `CARBONFR_SHARE_CALIBRATE_TO` (RFC 3339) pour une calibration reproductible
    // (dev, self-hosting sur historique figé, re-jeu du GATE de neutralité).
    let now = match parse_rfc3339_env("CARBONFR_SHARE_CALIBRATE_TO") {
        Ok(Some(to)) => to,
        Ok(None) => OffsetDateTime::now_utc(),
        Err(err) => {
            warn!(
                error = %err,
                "share-clim@1 : CARBONFR_SHARE_CALIBRATE_TO invalide — fin de calibration = maintenant"
            );
            OffsetDateTime::now_utc()
        }
    };
    let lookback = Duration::days(SHARE_LOOKBACK_DAYS);
    let horizon = Duration::hours(SHARE_MAX_HORIZON_HOURS);
    let params = ClimatologyParams::default();
    let calib = TimeRange::new(now - Duration::weeks(weeks), now)?;
    let full = TimeRange::new(calib.start() - lookback, now)?;

    let calibration = async {
        let history = repo
            .range(Region::National, "rte-direct", full)
            .await
            .ok()?;
        carbonfr_eligibility::calibrate_share_bands(
            &history,
            calib,
            lookback,
            Duration::days(1),
            params,
            horizon,
            0.1,
        )
    };
    match tokio::time::timeout(CALIBRATION_TIMEOUT, calibration).await {
        Ok(Some(bands)) => {
            info!(
                weeks,
                horizons = bands.len(),
                "bandes share-clim@1 calibrées (part renouvelable prévue, ADR-0028)"
            );
            // `cache_ttl` = intervalle de poll : durée de vie du cache de la
            // fenêtre climatologique de part (audit perf 2026-08).
            Some(ShareForecastConfig::new(
                bands, params, lookback, horizon, cache_ttl,
            ))
        }
        Ok(None) => {
            warn!(
                "calibration share-clim@1 impossible (historique insuffisant) — part renouvelable \
                 future Indeterminate"
            );
            None
        }
        Err(_) => {
            warn!(
                "calibration share-clim@1 : timeout au démarrage — part renouvelable future \
                 Indeterminate"
            );
            None
        }
    }
}

/// Calibre le modèle de dérivation renouvelable (ADR-0018) sur l'historique
/// récent au démarrage, pour servir `/v1/renewable`. Fenêtre large par défaut
/// (52 sem.) car le `rte-direct` récent est creux (le `def` accuse du retard ;
/// le poller n'alimente que depuis peu) → on capte l'historique dense. `None`
/// (endpoint `503`) si l'assise est trop maigre, `…_WEEKS=0`, ou timeout.
async fn build_calibrated_renewable_model(
    repo: PgIntensityRepository,
) -> Option<carbonfr_core::domain::RenewableModel> {
    let weeks = std::env::var("CARBONFR_RENEWABLE_CALIBRATE_WEEKS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(52);
    if weeks <= 0 {
        return None;
    }
    let now = OffsetDateTime::now_utc();
    let range = TimeRange::new(now - Duration::weeks(weeks), now)?;
    let use_case = CalibrateRenewable::new(repo.clone(), repo);
    let calibration = use_case.execute(range);
    match tokio::time::timeout(CALIBRATION_TIMEOUT, calibration).await {
        Ok(Ok(model)) => {
            info!(
                wind_capacity_mw = model.wind_capacity_mw.round(),
                solar_capacity_mw = model.solar_capacity_mw.round(),
                weeks,
                "modèle renouvelable calibré (/v1/renewable)"
            );
            Some(model)
        }
        Ok(Err(err)) => {
            warn!(error = %err, "calibration renouvelable impossible — /v1/renewable répondra 503");
            None
        }
        Err(_) => {
            warn!("calibration renouvelable : timeout au démarrage — /v1/renewable répondra 503");
            None
        }
    }
}

/// Construit le modèle de prévision avec **intervalles calibrés** : auto-
/// calibration des quantiles de résidus par horizon (ADR-0011) sur l'historique
/// récent. Repli silencieux sur les bandes par créneau si l'historique est
/// insuffisant, si `CARBONFR_FORECAST_CALIBRATE_WEEKS=0`, ou en cas de timeout.
async fn build_calibrated_forecaster(
    repo: PgIntensityRepository,
) -> ClimatologyForecaster<PgIntensityRepository> {
    let base = ClimatologyForecaster::new(repo.clone());

    let weeks = std::env::var("CARBONFR_FORECAST_CALIBRATE_WEEKS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(8);
    if weeks <= 0 {
        info!("auto-calibration des intervalles désactivée (bandes par créneau)");
        return base;
    }

    let now = OffsetDateTime::now_utc();
    let Some(window) = TimeRange::new(now - Duration::weeks(weeks), now) else {
        return base;
    };

    let calibrator =
        BacktestForecast::new(ClimatologyForecaster::new(repo.clone()), repo, "rte-direct");
    let calibration = calibrator.calibrate_bands(
        Region::National,
        window,
        Duration::days(1),
        Duration::minutes(15),
        Duration::hours(24),
        0.1,
    );
    match tokio::time::timeout(CALIBRATION_TIMEOUT, calibration).await {
        Ok(Ok(bands)) if !bands.is_empty() => {
            info!(
                horizons = bands.len(),
                "intervalles de prévision calibrés (quantiles de résidus par horizon)"
            );
            base.with_bands(bands)
        }
        Ok(Ok(_)) => {
            info!(
                "historique récent insuffisant pour calibrer les intervalles — bandes par créneau"
            );
            base
        }
        Ok(Err(err)) => {
            warn!(error = %err, "calibration des intervalles impossible — bandes par créneau");
            base
        }
        Err(_) => {
            warn!("calibration des intervalles : timeout au démarrage — bandes par créneau");
            base
        }
    }
}

/// Mode `backtest-bands` : calibre et imprime les bandes d'incertitude par
/// horizon (ADR-0011), puis arrête. Mêmes paramètres de fenêtre que `backtest`,
/// plus `CARBONFR_BACKTEST_HORIZON_HOURS` (déf. 24) et
/// `CARBONFR_BACKTEST_BAND_QUANTILE` (déf. 0.1).
async fn run_backtest_bands() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let params = BacktestParams::from_env()?;

    let horizon_hours = std::env::var("CARBONFR_BACKTEST_HORIZON_HOURS")
        .ok()
        .map(|raw| raw.parse::<i64>())
        .transpose()
        .context("CARBONFR_BACKTEST_HORIZON_HOURS : entier invalide")?
        .unwrap_or(24);
    anyhow::ensure!(
        horizon_hours > 0,
        "CARBONFR_BACKTEST_HORIZON_HOURS doit être > 0"
    );
    let q = std::env::var("CARBONFR_BACKTEST_BAND_QUANTILE")
        .ok()
        .map(|raw| raw.parse::<f64>())
        .transpose()
        .context("CARBONFR_BACKTEST_BAND_QUANTILE : réel invalide")?
        .unwrap_or(0.1);

    // Forecaster aligné sur le pas de la donnée évaluée (30 min pour le jeu
    // consolidé) ; défauts calés N=10, τ=2 sem.
    let forecaster = ClimatologyForecaster::with_config(
        repo.clone(),
        10,
        ClimatologyParams {
            step: params.step,
            tau: Duration::days(14),
        },
    );
    let calibrator = BacktestForecast::new(forecaster, repo, params.methodology.clone());
    let bands = calibrator
        .calibrate_bands(
            params.region,
            params.test,
            params.origin_step,
            params.step,
            Duration::hours(horizon_hours),
            q,
        )
        .await
        .context("calibration des bandes")?;

    println!();
    println!(
        "Bandes d'incertitude — région {}, méthodologie {}, q={q}",
        params.region.slug(),
        params.methodology
    );
    println!(
        "Horizons calibrés : {} (pas {} min)",
        bands.len(),
        params.step.whole_minutes()
    );
    println!();
    println!(
        "{:>8} {:>10} {:>10} {:>10}",
        "Horizon", "bas", "haut", "largeur"
    );
    for cp in BACKTEST_CHECKPOINTS {
        if let Some((low, high)) = bands.at(cp) {
            println!(
                "{:>7}h {:>10.2} {:>10.2} {:>10.2}",
                cp.whole_hours(),
                low,
                high,
                high - low
            );
        }
    }
    println!();
    println!("Bornes en gCO₂eq/kWh, relatives à l'estimation centrale (erreur = observé − prévu).");
    Ok(())
}

/// Sélectionne, pour une `origin` d'entraînement, la météo **connue au plus tard
/// à cette origine** : par `valid_at`, le `run_at` le plus récent tel que
/// `run_at ≤ origin`. Reproduit le filtre anti-fuite de `GbdtForecaster::forecast`
/// (adapter-gbdt) côté entraînement, par origine — au lieu d'un maximum global sur
/// toute la fenêtre qui fuiterait un run publié après l'origine (audit F11).
fn weather_as_of(
    rows: &[WeatherForecast],
    origin: OffsetDateTime,
) -> std::collections::HashMap<OffsetDateTime, (f64, f64)> {
    use std::collections::HashMap;
    let mut best: HashMap<OffsetDateTime, (OffsetDateTime, f64, f64)> = HashMap::new();
    for w in rows {
        if w.run_at > origin {
            continue;
        }
        match best.get(&w.valid_at) {
            Some((run, _, _)) if *run >= w.run_at => {}
            _ => {
                best.insert(w.valid_at, (w.run_at, w.wind, w.irradiance));
            }
        }
    }
    best.into_iter()
        .map(|(v, (_, wind, irr))| (v, (wind, irr)))
        .collect()
}

/// Mode `train` : entraîne le modèle **ML GBDT** (ADR-0012) sur l'historique,
/// sauvegarde l'artefact, puis **compare** `gbdt@1` à `climatology@1` au backtest
/// (garde de promotion : on ne sert le GBDT que s'il bat la climatologie).
///
/// Config : `CARBONFR_TRAIN_FROM`/`_TO` (période d'entraînement ; défaut 120 j
/// avant la fenêtre de test), `CARBONFR_GBDT_MODEL` (chemin de l'artefact ; déf.
/// `gbdt.model`), `CARBONFR_TRAIN_ORIGIN_STEP_HOURS` (déf. 6) ; fenêtre de test
/// et pas via les variables `CARBONFR_BACKTEST_*`.
async fn run_train() -> anyhow::Result<()> {
    use std::collections::HashMap;
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let params = BacktestParams::from_env()?;

    let train_to = parse_rfc3339_env("CARBONFR_TRAIN_TO")?.unwrap_or(params.test.start());
    let train_from =
        parse_rfc3339_env("CARBONFR_TRAIN_FROM")?.unwrap_or(train_to - Duration::days(120));
    anyhow::ensure!(
        train_to <= params.test.start(),
        "la période d'entraînement doit précéder la fenêtre de test (anti-fuite)"
    );
    let origin_step_hours = std::env::var("CARBONFR_TRAIN_ORIGIN_STEP_HOURS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(6);

    // Historique d'intensité couvrant l'entraînement (+ 1 semaine de lags amont).
    let read = TimeRange::new(train_from - Duration::weeks(1), train_to)
        .context("période d'entraînement invalide")?;
    let history = repo
        .range(params.region, &params.methodology, read)
        .await
        .context("lecture de l'historique")?;
    let intensity: HashMap<OffsetDateTime, f64> = history
        .iter()
        .map(|m| (m.at, m.intensity.value()))
        .collect();
    // Météo prévue (archive + poller) sur la période. On garde l'historique brut
    // multi-run et on sélectionne le run **par origine** (cf. `weather_as_of`) :
    // un maximum global sur toute la fenêtre fuiterait un run publié après
    // l'origine (F11), contrairement à l'inférence réelle.
    let weather_rows = repo
        .weather_range(read)
        .await
        .context("lecture de la météo")?;

    // Origines d'entraînement : du début +1 sem. à la fin −24 h.
    let mut origins = Vec::new();
    let mut o = train_from + Duration::weeks(1);
    while o < train_to - Duration::hours(24) {
        origins.push(o);
        o += Duration::hours(origin_step_hours);
    }
    let mut examples = Vec::new();
    for &origin in &origins {
        // Anti-fuite : météo connue *au plus tard à l'origine* (même filtre que
        // `GbdtForecaster::forecast`), appliquée par origine.
        let weather = weather_as_of(&weather_rows, origin);
        examples.extend(build_training_examples(
            &intensity,
            &weather,
            10, // fenêtre glissante (semaines), identique à l'inférence
            std::slice::from_ref(&origin),
            params.step,
            Duration::hours(24),
        ));
    }
    anyhow::ensure!(
        !examples.is_empty(),
        "aucun exemple d'entraînement (historique insuffisant ?)"
    );
    info!(
        examples = examples.len(),
        origins = origins.len(),
        "entraînement GBDT"
    );

    let model = train_model(&examples, GbdtHyperParams::default())
        .context("entraînement GBDT (aucun exemple)")?;
    let path = std::env::var("CARBONFR_GBDT_MODEL").unwrap_or_else(|_| "gbdt.model".to_string());
    model.save(&path).map_err(anyhow::Error::msg)?;
    info!(path = %path, "artefact GBDT sauvegardé");

    // Comparaison sur la fenêtre de test (postérieure → pas de fuite de labels).
    let climatology = ClimatologyForecaster::with_config(
        repo.clone(),
        10,
        ClimatologyParams {
            step: params.step,
            tau: Duration::days(14),
        },
    );
    let r1 = BacktestForecast::new(climatology, repo.clone(), params.methodology.clone())
        .execute(
            params.region,
            params.test,
            params.origin_step,
            params.step,
            &BACKTEST_CHECKPOINTS,
        )
        .await
        .context("backtest climatology@1")?;

    let gbdt = GbdtForecaster::with_config(repo.clone(), repo.clone(), model, 10, params.step);
    let r2 = BacktestForecast::new(gbdt, repo.clone(), params.methodology.clone())
        .execute(
            params.region,
            params.test,
            params.origin_step,
            params.step,
            &BACKTEST_CHECKPOINTS,
        )
        .await
        .context("backtest gbdt@1")?;

    println!();
    println!(
        "Comparaison climatology@1 vs gbdt@1 — région {}, méthodologie {}",
        params.region.slug(),
        params.methodology
    );
    println!(
        "Entraîné sur {} → {}  ({} exemples) ; testé sur {} → {}",
        train_from,
        train_to,
        examples.len(),
        params.test.start(),
        params.test.end()
    );
    println!();
    println!("{:<16} {:>10} {:>10} {:>10}", "Série", "MAE", "RMSE", "n");
    print_metrics_row("climato @1", r1.model);
    print_metrics_row("gbdt @1", r2.model);
    for (h1, h2) in r1.by_horizon.iter().zip(r2.by_horizon.iter()) {
        print_metrics_row(&format!("climato h+{}", h1.horizon.whole_hours()), h1.model);
        print_metrics_row(&format!("gbdt h+{}", h2.horizon.whole_hours()), h2.model);
    }
    Ok(())
}

/// Ouvre le pool PostgreSQL et applique les migrations.
async fn connect_repo(database_url: &str) -> anyhow::Result<PgIntensityRepository> {
    // Retry borné : la base peut démarrer quelques secondes après l'API
    // (compose/systemd sans ordering strict) — on évite un crash-loop au boot.
    let mut attempt = 0u32;
    let repo = loop {
        attempt += 1;
        match PgIntensityRepository::connect(database_url).await {
            Ok(repo) => break repo,
            Err(err) if attempt < 10 => {
                warn!(attempt, error = %err, "connexion PostgreSQL échouée — nouvelle tentative dans 2 s");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Err(err) => return Err(anyhow::Error::new(err).context("connexion à PostgreSQL")),
        }
    };
    repo.migrate().await.context("application des migrations")?;
    info!("base prête (migrations appliquées)");
    Ok(repo)
}

/// Configuration du mode service.
struct ServerConfig {
    database_url: String,
    bind: SocketAddr,
    poll_interval: std::time::Duration,
    /// Largeur de la fenêtre glissante ingérée à chaque cycle du poller
    /// (`CARBONFR_POLL_WINDOW_HOURS`, ADR-0003 addendum 2026-09-25).
    poll_window: Duration,
    visit_salt: Option<String>,
    trust_proxy: bool,
    /// En-tête d'IP réelle dédié (`CARBONFR_REAL_IP_HEADER`, opt-in — cf.
    /// [`real_ip_header_from_env`]).
    real_ip_header: Option<String>,
    /// Livraisons webhook échouées consécutives avant désactivation automatique
    /// d'un abonnement (`CARBONFR_WEBHOOK_MAX_FAILURES`, ADR-0016).
    webhook_max_failures: u32,
    /// Délai (jours) avant suppression d'un abonnement webhook **désactivé**
    /// (`CARBONFR_WEBHOOK_PURGE_DAYS`, ADR-0016 addendum « purge »).
    webhook_purge_days: u32,
    /// Jours réimportés chaque jour par l'auto-réparation
    /// (`CARBONFR_SELF_HEAL_DAYS`, 0 = désactivée, ADR-0003 addendum 2026-09-25).
    self_heal_days: u32,
    /// Étend l'auto-réparation quotidienne au régional
    /// (`CARBONFR_SELF_HEAL_REGIONAL`, défaut `0`/désactivée — item PERF-3,
    /// ADR-0003 addendum 2026-09-26). Livré désactivé par défaut : n'activer
    /// qu'une fois le quota ODRÉ réel visible en prod (PROD-3).
    self_heal_regional: bool,
}

impl ServerConfig {
    fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .context("la variable DATABASE_URL est requise (DSN PostgreSQL)")?;

        let bind: SocketAddr = std::env::var("CARBONFR_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse()
            .context("CARBONFR_BIND : adresse d'écoute invalide")?;

        let poll_secs = parse_poll_secs(std::env::var("CARBONFR_POLL_SECS").ok().as_deref())?;
        let poll_window =
            parse_poll_window_hours(std::env::var("CARBONFR_POLL_WINDOW_HOURS").ok().as_deref())?;
        let webhook_max_failures = parse_webhook_max_failures(
            std::env::var("CARBONFR_WEBHOOK_MAX_FAILURES")
                .ok()
                .as_deref(),
        )?;
        let webhook_purge_days =
            parse_webhook_purge_days(std::env::var("CARBONFR_WEBHOOK_PURGE_DAYS").ok().as_deref())?;
        let self_heal_days =
            parse_self_heal_days(std::env::var("CARBONFR_SELF_HEAL_DAYS").ok().as_deref())?;
        // Même patron booléen que `CARBONFR_TRUST_PROXY` : "1"/"true" = activée,
        // tout le reste (y compris absent) = désactivée (défaut sûr, PERF-3).
        let self_heal_regional = matches!(
            std::env::var("CARBONFR_SELF_HEAL_REGIONAL").as_deref(),
            Ok("1") | Ok("true")
        );

        let trust_proxy = matches!(
            std::env::var("CARBONFR_TRUST_PROXY").as_deref(),
            Ok("1") | Ok("true")
        );

        let visit_salt = std::env::var("CARBONFR_VISIT_SALT").ok();
        if visit_salt.is_none() {
            // `trust_proxy=1` = derrière un reverse proxy = **production** : un sel
            // par défaut public rendrait les empreintes d'IP réversibles → refus de
            // démarrer. En dev/self-hosting direct (trust_proxy=0), simple
            // avertissement (parité, aucun blocage).
            anyhow::ensure!(
                !trust_proxy,
                "CARBONFR_VISIT_SALT est requis en production (CARBONFR_TRUST_PROXY=1) : \
                 sans sel secret à haute entropie, les empreintes d'IP des visiteurs \
                 seraient réversibles (RGPD). Définir CARBONFR_VISIT_SALT."
            );
            warn!(
                "CARBONFR_VISIT_SALT non défini : sel de hachage des visiteurs PAR DÉFAUT \
                 (public) — les empreintes d'IP seraient réversibles. À définir en production."
            );
        }

        Ok(Self {
            database_url,
            bind,
            poll_interval: std::time::Duration::from_secs(poll_secs),
            poll_window,
            visit_salt,
            trust_proxy,
            real_ip_header: real_ip_header_from_env(),
            webhook_max_failures,
            webhook_purge_days,
            self_heal_days,
            self_heal_regional,
        })
    }
}

/// Période du poller (`CARBONFR_POLL_SECS`, défaut 900 s). **Refusée si nulle**
/// (audit 2026-08) : `tokio::time::interval` panique sur une période de zéro —
/// on échoue à la configuration, avec un message clair, plutôt qu'à l'exécution
/// du poller.
fn parse_poll_secs(raw: Option<&str>) -> anyhow::Result<u64> {
    let secs = raw
        .map(|raw| raw.parse::<u64>())
        .transpose()
        .context("CARBONFR_POLL_SECS : durée invalide")?
        .unwrap_or(900);
    anyhow::ensure!(secs > 0, "CARBONFR_POLL_SECS doit être > 0 (secondes)");
    Ok(secs)
}

/// Largeur de la fenêtre glissante interrogée par le poller à chaque cycle
/// (`CARBONFR_POLL_WINDOW_HOURS`, défaut [`INGEST_RECENT_DEFAULT_WINDOW`], 3 h
/// — ADR-0003 addendum 2026-09-25) : au lieu du seul dernier point
/// ([`IngestLatest`](carbonfr_core::application::IngestLatest), l'ancien
/// comportement), chaque cycle relit les `N` dernières heures via
/// [`IngestRecent`], ce qui comble d'éventuels retards de publication d'ODRÉ
/// sans appel supplémentaire (toujours un appel par zone et par cycle).
/// **Refusée si nulle** : le poller n'ingérerait alors plus jamais rien,
/// silencieusement — même logique que [`parse_poll_secs`].
fn parse_poll_window_hours(raw: Option<&str>) -> anyhow::Result<Duration> {
    let hours = raw
        .map(|raw| raw.trim().parse::<u32>())
        .transpose()
        .context("CARBONFR_POLL_WINDOW_HOURS : nombre invalide")?;
    let window = match hours {
        None => INGEST_RECENT_DEFAULT_WINDOW,
        Some(hours) => {
            // Plafond 24 h : au-delà, `range()` pagine (100 points par page au
            // pas de 15 min) et le coût en appels ODRÉ par cycle n'est plus de 1.
            anyhow::ensure!(
                (1..=24).contains(&hours),
                "CARBONFR_POLL_WINDOW_HOURS doit être entre 1 et 24 (heures)"
            );
            Duration::hours(i64::from(hours))
        }
    };
    Ok(window)
}

/// Jours réimportés par l'auto-réparation quotidienne (`CARBONFR_SELF_HEAL_DAYS`,
/// défaut [`DEFAULT_SELF_HEAL_DAYS`]). `0` la désactive ; plafond 7 : au-delà,
/// `refresh_rollups` (recalcul incrémental sur 7 j) ne mettrait pas à jour les
/// séries agrégées des jours plus anciens réimportés.
fn parse_self_heal_days(raw: Option<&str>) -> anyhow::Result<u32> {
    let days = raw
        .map(|raw| raw.trim().parse::<u32>())
        .transpose()
        .context("CARBONFR_SELF_HEAL_DAYS : nombre invalide")?
        .unwrap_or(DEFAULT_SELF_HEAL_DAYS);
    anyhow::ensure!(
        days <= 7,
        "CARBONFR_SELF_HEAL_DAYS doit être entre 0 (désactivée) et 7 (fenêtre de recalcul des rollups)"
    );
    Ok(days)
}

/// Seuil de désactivation automatique des webhooks
/// (`CARBONFR_WEBHOOK_MAX_FAILURES`, défaut
/// [`DEFAULT_WEBHOOK_MAX_CONSECUTIVE_FAILURES`]). Refusé si nul : « désactiver
/// après 0 échec » n'a pas de sens, et la désactivation n'est pas débrayable
/// (un endpoint mort serait martelé indéfiniment).
fn parse_webhook_max_failures(raw: Option<&str>) -> anyhow::Result<u32> {
    let max = raw
        .map(|raw| raw.trim().parse::<u32>())
        .transpose()
        .context("CARBONFR_WEBHOOK_MAX_FAILURES : nombre invalide")?
        .unwrap_or(DEFAULT_WEBHOOK_MAX_CONSECUTIVE_FAILURES);
    anyhow::ensure!(max > 0, "CARBONFR_WEBHOOK_MAX_FAILURES doit être > 0");
    Ok(max)
}

/// Délai de purge des abonnements webhook **désactivés**
/// (`CARBONFR_WEBHOOK_PURGE_DAYS`, défaut [`DEFAULT_WEBHOOK_PURGE_DAYS`],
/// ADR-0016 addendum 2026-09-23 « Purge des abonnements désactivés »). Refusé
/// si nul : « purger après 0 jour » supprimerait un abonnement avant même que
/// son propriétaire ait pu constater la désactivation via `GET /v1/webhooks`.
fn parse_webhook_purge_days(raw: Option<&str>) -> anyhow::Result<u32> {
    let days = raw
        .map(|raw| raw.trim().parse::<u32>())
        .transpose()
        .context("CARBONFR_WEBHOOK_PURGE_DAYS : nombre invalide")?
        .unwrap_or(DEFAULT_WEBHOOK_PURGE_DAYS);
    anyhow::ensure!(days > 0, "CARBONFR_WEBHOOK_PURGE_DAYS doit être > 0");
    Ok(days)
}

/// Jeu exporté par le backfill (`CARBONFR_BACKFILL_SOURCE`) : consolidé par
/// défaut ; `realtime` pour combler un trou récent que RTE n'a pas encore
/// consolidé (ses mesures `tr` seront remplacées par un backfill ultérieur).
fn backfill_source() -> anyhow::Result<ArchiveSource> {
    parse_backfill_source(std::env::var("CARBONFR_BACKFILL_SOURCE").ok().as_deref())
}

fn parse_backfill_source(value: Option<&str>) -> anyhow::Result<ArchiveSource> {
    match value.map(str::trim) {
        None | Some("") | Some("consolidated") => Ok(ArchiveSource::Consolidated),
        Some("realtime") => Ok(ArchiveSource::Realtime),
        Some(other) => anyhow::bail!(
            "CARBONFR_BACKFILL_SOURCE : « {other} » invalide (attendu : consolidated ou realtime)"
        ),
    }
}

/// Périmètre du backfill (`CARBONFR_BACKFILL_SCOPE`, item PROD-1) : national
/// par défaut (comportement historique inchangé — charge + météo archivées
/// comprises), régional (mix `acv-ademe` des 12 régions, sans charge ni
/// météo — entrées nationales), ou les deux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum BackfillScope {
    #[default]
    National,
    Regional,
    All,
}

/// Périmètre exporté par le backfill (`CARBONFR_BACKFILL_SCOPE`).
fn backfill_scope() -> anyhow::Result<BackfillScope> {
    parse_backfill_scope(std::env::var("CARBONFR_BACKFILL_SCOPE").ok().as_deref())
}

fn parse_backfill_scope(value: Option<&str>) -> anyhow::Result<BackfillScope> {
    match value.map(str::trim) {
        None | Some("") | Some("national") => Ok(BackfillScope::National),
        Some("regional") => Ok(BackfillScope::Regional),
        Some("all") => Ok(BackfillScope::All),
        Some(other) => anyhow::bail!(
            "CARBONFR_BACKFILL_SCOPE : « {other} » invalide (attendu : national, regional ou all)"
        ),
    }
}

/// Résout l'intervalle et la largeur de tranche du backfill depuis l'environnement.
fn backfill_params() -> anyhow::Result<(TimeRange, Duration)> {
    let default_start = Date::from_calendar_date(2012, Month::January, 1)
        .expect("2012-01-01 est une date valide")
        .midnight()
        .assume_utc();

    let start = parse_rfc3339_env("CARBONFR_BACKFILL_FROM")?.unwrap_or(default_start);
    let end = parse_rfc3339_env("CARBONFR_BACKFILL_TO")?.unwrap_or_else(OffsetDateTime::now_utc);
    let range =
        TimeRange::new(start, end).context("intervalle de backfill invalide (fin <= début)")?;

    let window_days = std::env::var("CARBONFR_BACKFILL_WINDOW_DAYS")
        .ok()
        .map(|raw| raw.parse::<i64>())
        .transpose()
        .context("CARBONFR_BACKFILL_WINDOW_DAYS : entier invalide")?
        .unwrap_or(90);
    anyhow::ensure!(
        window_days > 0,
        "CARBONFR_BACKFILL_WINDOW_DAYS doit être > 0"
    );

    Ok((range, Duration::days(window_days)))
}

fn parse_rfc3339_env(name: &str) -> anyhow::Result<Option<OffsetDateTime>> {
    match std::env::var(name) {
        Ok(raw) => OffsetDateTime::parse(&raw, &Rfc3339)
            .map(Some)
            .with_context(|| format!("{name} : horodatage RFC 3339 invalide")),
        Err(_) => Ok(None),
    }
}

/// Démarre la tâche d'ingestion périodique. La première itération s'exécute
/// immédiatement. Une erreur d'ingestion est journalisée sans interrompre la
/// boucle (la donnée sera rattrapée à la prochaine itération ou au backfill).
///
/// Chaque cycle relit une **fenêtre glissante** de `window` (ADR-0003 addendum
/// 2026-09-25, [`IngestRecent`]) plutôt que le seul dernier point
/// ([`IngestLatest`](carbonfr_core::application::IngestLatest), ancien
/// comportement) : un retard de publication d'ODRÉ (jeu temps réel, ~30 min)
/// ne fait alors plus perdre le point définitivement, il est rattrapé au(x)
/// cycle(s) suivant(s) — **sans appel ODRÉ supplémentaire** (toujours un appel
/// par zone et par cycle, `range()` tenant en une page pour une fenêtre de
/// quelques heures).
#[allow(clippy::too_many_arguments)]
fn spawn_poller<S, W, C, R>(
    source: S,
    weather: W,
    cross_border: Option<C>,
    repo: R,
    updates: tokio::sync::broadcast::Sender<IntensityUpdate>,
    interval: std::time::Duration,
    window: Duration,
    metrics: Metrics,
) -> JoinHandle<()>
where
    S: Eco2mixSource + ConsumptionSource + Clone + 'static,
    W: WeatherForecastSource + 'static,
    C: CrossBorderSource + SpotPriceSource + 'static,
    R: IntensityRepository
        + ConsumptionRepository
        + WeatherRepository
        + CrossBorderRepository
        + SpotPriceRepository
        + Clone
        + 'static,
{
    let ingest = IngestRecent::new(source.clone(), repo.clone(), window);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // Ticks manqués (cycle plus long que l'intervalle, machine suspendue…) :
        // le défaut tokio (`Burst`) les rattrape en rafale — autant d'appels
        // ODRÉ/ENTSO-E consécutifs pour ré-ingérer la même donnée. `Delay`
        // repart du tick courant et respecte l'espacement (audit 2026-08).
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            // « Maintenant » figé une fois par cycle (pas par région) : la
            // fenêtre glissante de chaque zone porte sur le même instant, et
            // `IngestRecent` ne lit jamais l'horloge lui-même (pureté, ADR-0002).
            let now = OffsetDateTime::now_utc();

            // National (rte-direct + acv-ademe dérivée) puis les 12 régions
            // (acv-ademe). Une région en échec ne bloque pas les autres.
            let mut written = 0usize;
            for region in std::iter::once(Region::National).chain(Region::METROPOLITAN) {
                // Un appel ODRÉ par région : compté pour suivre le quota (50k/mois).
                metrics.add_upstream_odre(1);
                match ingest.execute(region, now).await {
                    Ok(report) => written += report.written,
                    Err(err) => {
                        metrics.inc_error();
                        warn!(region = region.slug(), error = %err, "échec d'ingestion ODRÉ")
                    }
                }
            }
            metrics.inc_cycle();
            metrics.add_written(written);
            if written > 0 {
                metrics.set_last_success(OffsetDateTime::now_utc().unix_timestamp());
            }
            info!(written, "ingestion ODRÉ (national + régions)");

            // Diffusion live (ADR-0014 §2) : on pousse la dernière mesure
            // nationale `rte-direct` aux abonnés SSE. `send` échoue sans abonné —
            // sans conséquence (canal sans rétention forte). Une erreur de
            // lecture est **tracée** (audit 2026-08) : avalée, le SSE et la
            // jauge de fraîcheur gelaient sans aucun indice au journal.
            match repo.latest(Region::National, "rte-direct").await {
                Ok(Some(m)) => {
                    metrics.set_last_measurement(m.at.unix_timestamp());
                    let _ = updates.send(IntensityUpdate::from_measurement(&m));
                }
                Ok(None) => {}
                Err(err) => warn!(
                    error = %err,
                    "lecture de la dernière mesure impossible — SSE et jauge de fraîcheur non rafraîchis"
                ),
            }

            // Charge nationale : consommation récente + prévisions RTE — entrée
            // du futur modèle ML (ADR-0012).
            metrics.add_upstream_odre(1);
            match source.recent_loads(Region::National).await {
                Ok(loads) if !loads.is_empty() => match repo.upsert_loads(&loads).await {
                    Ok(n) => info!(loads = n, "ingestion charge (conso + prévisions)"),
                    Err(err) => warn!(error = %err, "échec d'écriture de la charge"),
                },
                Ok(_) => {}
                Err(err) => warn!(error = %err, "échec de récupération de la charge ODRÉ"),
            }

            // Prévision météo nationale (vent + irradiance, ADR-0012) : chaque
            // cycle enregistre un nouveau `run_at` (historique anti-fuite).
            metrics.inc_upstream_open_meteo();
            match weather.current_forecast().await {
                Ok(forecasts) if !forecasts.is_empty() => {
                    match repo.upsert_weather(&forecasts).await {
                        Ok(n) => info!(weather = n, "ingestion météo (prévisions)"),
                        Err(err) => warn!(error = %err, "échec d'écriture de la météo"),
                    }
                }
                Ok(_) => {}
                Err(err) => warn!(error = %err, "échec de récupération de la météo"),
            }

            // Contexte d'import transfrontalier (ENTSO-E, ADR-0010) — entrée du
            // calcul `acv-ademe@2`. Optionnel : seulement si un token est
            // configuré. Échec non bloquant.
            if let Some(entsoe) = cross_border.as_ref() {
                metrics.inc_upstream_entsoe();
                match entsoe.recent_flows().await {
                    Ok(snapshots) if !snapshots.is_empty() => {
                        match repo.upsert_flows(&snapshots).await {
                            Ok(n) => {
                                metrics.set_last_flows(OffsetDateTime::now_utc().unix_timestamp());
                                info!(flows = n, "ingestion contexte d'import (ENTSO-E)");
                            }
                            Err(err) => {
                                warn!(error = %err, "échec d'écriture du contexte d'import")
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(err) => warn!(error = %err, "échec de récupération ENTSO-E"),
                }

                // Prix spot day-ahead (ENTSO-E A44, ADR-0023) — composante énergie
                // de `/v1/price`. Même token. Échec non bloquant.
                metrics.inc_upstream_entsoe();
                match entsoe.recent_prices().await {
                    Ok(prices) if !prices.is_empty() => match repo.upsert_prices(&prices).await {
                        Ok(n) => {
                            metrics.set_last_price(OffsetDateTime::now_utc().unix_timestamp());
                            info!(prices = n, "ingestion prix spot day-ahead (ENTSO-E)");
                        }
                        Err(err) => warn!(error = %err, "échec d'écriture du prix spot"),
                    },
                    Ok(_) => {}
                    Err(err) => warn!(error = %err, "échec de récupération du prix spot ENTSO-E"),
                }
            }

            // Rollups rafraîchis une fois par cycle si la donnée a changé.
            if written > 0
                && let Err(err) = repo.refresh_rollups().await
            {
                warn!(error = %err, "échec du rafraîchissement des rollups");
            }
        }
    })
}

/// Tâche de fond **watcher de webhooks** (ADR-0016) : consomme le flux des mises
/// à jour nationales, détecte les **franchissements de seuil** (*edge-triggered*)
/// des abonnements actifs, et émet une livraison **signée** par abonnement. La
/// livraison (avec garde SSRF + retries) est déléguée au `Notifier`, hors du
/// chemin d'évaluation ; son issue est enregistrée, et un abonnement dont
/// `max_failures` livraisons d'affilée ont échoué est **désactivé** (ADR-0016,
/// addendum 2026-09).
fn spawn_webhook_watcher<R, N>(
    mut updates: tokio::sync::broadcast::Receiver<IntensityUpdate>,
    repo: R,
    notifier: N,
    max_failures: u32,
) -> JoinHandle<()>
where
    R: SubscriptionRepository + Clone + 'static,
    N: Notifier + Clone + 'static,
{
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    // Borne la concurrence des livraisons : un événement peut déclencher N
    // abonnements ; sans plafond, un pic de franchissements ouvrirait N connexions
    // HTTPS sortantes simultanées (pression FD/sockets).
    let delivery_slots = Arc::new(Semaphore::new(50));

    tokio::spawn(async move {
        // Dernière intensité connue par région (pour détecter le franchissement).
        let mut previous: HashMap<Region, f64> = HashMap::new();
        loop {
            let update = match updates.recv().await {
                Ok(u) => u,
                // Événements perdus (abonné en retard) : un franchissement a pu
                // passer inaperçu. On invalide l'état pour ne pas comparer contre
                // une baseline périmée (la prochaine mise à jour réamorce).
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(perdus = n, "watcher webhooks en retard — état réamorcé");
                    previous.clear();
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            let prev = previous.get(&update.region).copied();
            let current = update.intensity.value();

            // On lit les abonnements **avant** d'avancer la baseline : si la base
            // est indisponible, on n'avance pas `previous` → le franchissement sera
            // ré-évalué à la prochaine mise à jour (pas consommé en silence).
            let subscriptions = match repo.active().await {
                Ok(s) => s,
                Err(err) => {
                    warn!(error = %err, "watcher webhooks : lecture des abonnements impossible");
                    continue;
                }
            };
            previous.insert(update.region, current);

            let Ok(timestamp) = update.at.format(&Rfc3339) else {
                continue;
            };

            for sub in subscriptions
                .into_iter()
                .filter(|s| s.region == update.region)
            {
                if !should_fire(sub.direction, sub.threshold, prev, current) {
                    continue;
                }
                // Contrat de payload + signature dans le domaine (pur, testé).
                let body = render_webhook_payload(&sub, &timestamp, current);
                let signature = hmac_sha256_hex(sub.secret.as_bytes(), body.as_bytes());
                let delivery = WebhookDelivery {
                    url: sub.callback_url.clone(),
                    body,
                    signature,
                };
                // Livraison hors du chemin d'évaluation, sous permis (concurrence
                // bornée). Permis indisponible → on saute (best-effort assumé).
                let Ok(permit) = delivery_slots.clone().try_acquire_owned() else {
                    warn!(subscription = %sub.id, "livraison webhook ignorée (saturation)");
                    continue;
                };
                let notifier = notifier.clone();
                let repo = repo.clone();
                let id = sub.id.clone();
                tokio::spawn(async move {
                    let delivered = match notifier.deliver(&delivery).await {
                        Ok(()) => {
                            info!(subscription = %id, "webhook livré");
                            true
                        }
                        Err(err) => {
                            warn!(subscription = %id, error = %err, "livraison webhook échouée");
                            false
                        }
                    };
                    // Le permis borne les connexions HTTPS sortantes, pas les
                    // écritures en base : relâché dès la fin de la livraison, pour
                    // qu'une base lente ne fasse pas sauter d'autres livraisons.
                    drop(permit);
                    // Compteur d'échecs consécutifs (remis à zéro par un succès).
                    // Base indisponible : l'issue est perdue — sans effet sur les
                    // livraisons, au pire une désactivation retardée.
                    match repo.record_delivery(&id, delivered, max_failures).await {
                        Ok(true) => warn!(
                            subscription = %id,
                            max_failures,
                            "abonnement webhook désactivé après des échecs consécutifs"
                        ),
                        Ok(false) => {}
                        Err(err) => warn!(
                            subscription = %id,
                            error = %err,
                            "issue de livraison webhook non enregistrée"
                        ),
                    }
                });
            }
        }
    })
}

/// Tâche de fond **purge des abonnements webhook désactivés** (ADR-0016
/// addendum 2026-09-23 « Purge des abonnements désactivés ») : un abonnement
/// désactivé après `max_failures` échecs consécutifs (cf.
/// [`spawn_webhook_watcher`]) reste sinon compté dans le quota de 50 par clé
/// jusqu'à suppression manuelle. Purge **au démarrage puis toutes les
/// [`WEBHOOK_PURGE_INTERVAL`]** les abonnements désactivés depuis plus de
/// `purge_days` jours. Best-effort : une erreur est journalisée (`warn`) sans
/// jamais interrompre la boucle ni paniquer — un cycle manqué est rattrapé au
/// suivant.
fn spawn_webhook_purge<R>(repo: R, purge_days: u32) -> JoinHandle<()>
where
    R: SubscriptionRepository + 'static,
{
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(WEBHOOK_PURGE_INTERVAL);
        // Comme le poller (audit 2026-08) : un tick manqué (machine suspendue…)
        // ne doit pas déclencher une rafale de purges consécutives.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let cutoff = OffsetDateTime::now_utc() - Duration::days(i64::from(purge_days));
            match repo.purge_disabled(cutoff).await {
                Ok(n) if n > 0 => info!(purged = n, "abonnements webhook désactivés purgés"),
                Ok(_) => {}
                Err(err) => warn!(
                    error = %err,
                    "purge des abonnements webhook désactivés impossible"
                ),
            }
        }
    })
}

/// Auto-réparation quotidienne de la collecte (ADR-0003 addendum 2026-09-25) :
/// réimporte les `days` derniers jours nationaux par **un** export de masse du
/// jeu temps réel (1 appel ODRÉ par jour), puis recalcule les rollups récents.
/// Comble d'elle-même une panne de la source de quelques jours dès son retour
/// (panne ODRÉ du 2026-08-25 au 2026-09-03 : 8 jours perdus). Upsert
/// conditionnel au millésime : une mesure consolidée n'est jamais écrasée.
/// Un échec est journalisé et retenté le lendemain ; `days = 0` la désactive
/// (tâche inerte, pour garder une supervision uniforme).
///
/// `regional` (`CARBONFR_SELF_HEAL_REGIONAL`, défaut désactivée, item PERF-3,
/// ADR-0003 addendum 2026-09-26) étend, sur la **même fenêtre**, une seconde
/// tranche d'export de masse sur le jeu régional — journalisée à part, échec
/// non bloquant comme le national. Coût quand activée : +1 export/jour sur le
/// jeu régional temps réel. **À activer en prod seulement une fois le quota
/// ODRÉ réel visible** (PROD-3, jauges `carbonfr_odre_quota_*`) : livrée
/// désactivée par défaut.
fn spawn_self_heal<A, R>(archive: A, repo: R, days: u32, regional: bool) -> JoinHandle<()>
where
    A: Eco2mixArchive + 'static,
    R: IntensityRepository + Clone + 'static,
{
    tokio::spawn(async move {
        if days == 0 {
            info!("auto-réparation de la collecte désactivée (CARBONFR_SELF_HEAL_DAYS=0)");
            std::future::pending::<()>().await;
        }
        let window = Duration::days(i64::from(days));
        let backfill = BackfillHistory::new(archive, repo.clone(), window);
        tokio::time::sleep(SELF_HEAL_START_DELAY).await;
        let mut ticker = tokio::time::interval(SELF_HEAL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let now = OffsetDateTime::now_utc();
            let Some(range) = TimeRange::new(now - window, now) else {
                continue;
            };

            // Tranche nationale (toujours active tant que `days > 0`).
            let mut written_total = 0usize;
            match backfill.execute(range).await {
                Ok(report) => {
                    info!(
                        days,
                        read = report.read,
                        written = report.written,
                        "auto-réparation de la collecte terminée"
                    );
                    written_total += report.written;
                }
                Err(err) => warn!(
                    error = %err,
                    "auto-réparation de la collecte impossible (nouvel essai dans 24 h)"
                ),
            }

            // Tranche régionale (PERF-3, désactivée par défaut) : même fenêtre,
            // journalisée à part ; un échec ne bloque ni la boucle ni la tranche
            // nationale déjà écrite (best-effort, comme le national ci-dessus).
            if regional {
                match backfill.execute_regional(range).await {
                    Ok(report) => {
                        info!(
                            days,
                            read = report.read,
                            written = report.written,
                            "auto-réparation régionale de la collecte terminée"
                        );
                        written_total += report.written;
                    }
                    Err(err) => warn!(
                        error = %err,
                        "auto-réparation régionale de la collecte impossible (nouvel essai dans 24 h)"
                    ),
                }
            }

            if written_total > 0
                && let Err(err) = repo.refresh_rollups().await
            {
                warn!(error = %err, "auto-réparation : recalcul des rollups impossible");
            }
        }
    })
}

/// Attend **SIGINT (Ctrl-C) ou SIGTERM** pour un arrêt propre. SIGTERM est le
/// signal envoyé par systemd/Docker à l'arrêt orchestré — sans lui, l'arrêt
/// gracieux ne s'enclencherait pas en production.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => {
                error!(error = %err, "écoute de SIGTERM impossible");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    info!("arrêt demandé, fermeture en cours");
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    // `CARBONFR_LOG_FORMAT=json` → logs structurés (agrégation Loki/journald) ;
    // sinon format texte lisible (défaut dev).
    if std::env::var("CARBONFR_LOG_FORMAT").as_deref() == Ok("json") {
        builder.json().init();
    } else {
        builder.init();
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn backfill_source_defaults_to_consolidated() {
        assert_eq!(
            parse_backfill_source(None).unwrap(),
            ArchiveSource::Consolidated
        );
        assert_eq!(
            parse_backfill_source(Some("")).unwrap(),
            ArchiveSource::Consolidated
        );
        assert_eq!(
            parse_backfill_source(Some("consolidated")).unwrap(),
            ArchiveSource::Consolidated
        );
        assert_eq!(
            parse_backfill_source(Some("realtime")).unwrap(),
            ArchiveSource::Realtime
        );
        assert!(parse_backfill_source(Some("tr")).is_err());
    }

    #[test]
    fn backfill_scope_defaults_to_national() {
        assert_eq!(parse_backfill_scope(None).unwrap(), BackfillScope::National);
        assert_eq!(
            parse_backfill_scope(Some("")).unwrap(),
            BackfillScope::National
        );
        assert_eq!(
            parse_backfill_scope(Some("national")).unwrap(),
            BackfillScope::National
        );
        assert_eq!(
            parse_backfill_scope(Some("regional")).unwrap(),
            BackfillScope::Regional
        );
        assert_eq!(
            parse_backfill_scope(Some("all")).unwrap(),
            BackfillScope::All
        );
        let err = parse_backfill_scope(Some("europe")).unwrap_err();
        assert!(err.to_string().contains("CARBONFR_BACKFILL_SCOPE"));
    }

    use super::{
        ArchiveSource, BackfillScope, INGEST_RECENT_DEFAULT_WINDOW, parse_backfill_scope,
        parse_backfill_source, parse_poll_secs, parse_poll_window_hours, parse_self_heal_days,
        parse_webhook_max_failures, parse_webhook_purge_days, revocation_target,
        spawn_webhook_watcher,
    };

    /// Le watcher enregistre l'issue de chaque livraison avec le seuil configuré
    /// (ADR-0016, addendum 2026-09) — c'est ce qui alimente la désactivation
    /// automatique d'un abonnement dont l'endpoint est mort.
    #[tokio::test]
    async fn webhook_watcher_records_failed_delivery_with_threshold() {
        use std::sync::{Arc, Mutex};

        use async_trait::async_trait;
        use carbonfr_core::domain::{
            CarbonIntensity, IntensityUpdate, Methodology, Region, Subscription, ThresholdDirection,
        };
        use carbonfr_core::ports::{
            Notifier, RepositoryError, SourceError, SubscriptionRepository, WebhookDelivery,
        };

        type Recorded = Arc<Mutex<Vec<(String, bool, u32)>>>;

        #[derive(Clone, Default)]
        struct Repo {
            recorded: Recorded,
        }
        #[async_trait]
        impl SubscriptionRepository for Repo {
            async fn create(&self, _: &Subscription, _: usize) -> Result<bool, RepositoryError> {
                Ok(true)
            }
            async fn list_for_owner(&self, _: &str) -> Result<Vec<Subscription>, RepositoryError> {
                Ok(Vec::new())
            }
            async fn delete(&self, _: &str, _: &str) -> Result<bool, RepositoryError> {
                Ok(false)
            }
            async fn active(&self) -> Result<Vec<Subscription>, RepositoryError> {
                Ok(vec![Subscription {
                    id: "wh-1".to_string(),
                    owner_key_hash: "h".to_string(),
                    region: Region::National,
                    threshold: 50.0,
                    direction: ThresholdDirection::Below,
                    callback_url: "https://hooks.example.com/c".to_string(),
                    secret: "s".to_string(),
                    disabled_at: None,
                }])
            }
            async fn record_delivery(
                &self,
                id: &str,
                delivered: bool,
                max: u32,
            ) -> Result<bool, RepositoryError> {
                self.recorded
                    .lock()
                    .unwrap()
                    .push((id.to_string(), delivered, max));
                Ok(false)
            }
            async fn purge_disabled(
                &self,
                _disabled_before: time::OffsetDateTime,
            ) -> Result<u64, RepositoryError> {
                Ok(0)
            }
        }

        #[derive(Clone)]
        struct DeadEndpoint;
        #[async_trait]
        impl Notifier for DeadEndpoint {
            async fn deliver(&self, _: &WebhookDelivery) -> Result<(), SourceError> {
                Err(SourceError::Unavailable("endpoint mort".to_string()))
            }
        }

        let (tx, rx) = tokio::sync::broadcast::channel(8);
        let repo = Repo::default();
        let watcher = spawn_webhook_watcher(rx, repo.clone(), DeadEndpoint, 7);
        let update = |g: f64, minutes: i64| IntensityUpdate {
            region: Region::National,
            at: time::OffsetDateTime::UNIX_EPOCH + time::Duration::minutes(minutes),
            intensity: CarbonIntensity::new(g).unwrap(),
            methodology: Methodology::rte_direct(),
        };
        // 60 → 40 : franchissement « sous 50 » → une livraison, qui échoue.
        tx.send(update(60.0, 0)).unwrap();
        tx.send(update(40.0, 15)).unwrap();
        for _ in 0..200 {
            if !repo.recorded.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        watcher.abort();
        assert_eq!(
            *repo.recorded.lock().unwrap(),
            vec![("wh-1".to_string(), false, 7)]
        );
    }

    #[test]
    fn webhook_max_failures_default_explicit_and_invalid() {
        assert_eq!(
            parse_webhook_max_failures(None).unwrap(),
            carbonfr_core::domain::DEFAULT_WEBHOOK_MAX_CONSECUTIVE_FAILURES
        );
        assert_eq!(parse_webhook_max_failures(Some(" 3 ")).unwrap(), 3);
        let err = parse_webhook_max_failures(Some("0")).unwrap_err();
        assert!(err.to_string().contains("CARBONFR_WEBHOOK_MAX_FAILURES"));
        assert!(parse_webhook_max_failures(Some("-1")).is_err());
        assert!(parse_webhook_max_failures(Some("dix")).is_err());
    }

    #[test]
    fn webhook_purge_days_default_explicit_and_invalid() {
        assert_eq!(
            parse_webhook_purge_days(None).unwrap(),
            carbonfr_core::domain::DEFAULT_WEBHOOK_PURGE_DAYS
        );
        assert_eq!(parse_webhook_purge_days(Some(" 7 ")).unwrap(), 7);
        let err = parse_webhook_purge_days(Some("0")).unwrap_err();
        assert!(err.to_string().contains("CARBONFR_WEBHOOK_PURGE_DAYS"));
        assert!(parse_webhook_purge_days(Some("-1")).is_err());
        assert!(parse_webhook_purge_days(Some("trente")).is_err());
    }

    /// `revoke-key` accepte la clé en clair (hachée comme à la délivrance) ou son
    /// empreinte (casse normalisée), et refuse tout le reste plutôt que de
    /// chercher une empreinte qui ne peut pas exister.
    #[test]
    fn revocation_target_accepts_key_or_fingerprint() {
        let key = "cfr_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let hash = carbonfr_adapter_http::key_fingerprint(key);
        assert_eq!(revocation_target(key).unwrap(), hash);
        assert_eq!(revocation_target(&format!("  {key}\n")).unwrap(), hash);
        assert_eq!(revocation_target(&hash).unwrap(), hash);
        assert_eq!(revocation_target(&hash.to_ascii_uppercase()).unwrap(), hash);
        assert!(revocation_target("").is_err());
        assert!(revocation_target("projet-test").is_err());
        assert!(revocation_target(&hash[..63]).is_err());
        assert!(revocation_target(&format!("{}z", &hash[..63])).is_err());
    }

    /// Audit 2026-08 : `CARBONFR_POLL_SECS=0` était accepté à la config puis
    /// faisait paniquer le poller (`tokio::time::interval` refuse une période
    /// nulle). La validation échoue désormais **au parse**, avec un message clair.
    #[test]
    fn poll_secs_zero_is_rejected_at_parse() {
        let err = parse_poll_secs(Some("0")).unwrap_err();
        assert!(err.to_string().contains("CARBONFR_POLL_SECS"));
    }

    #[test]
    fn poll_secs_invalid_is_rejected() {
        assert!(parse_poll_secs(Some("quinze")).is_err());
        assert!(parse_poll_secs(Some("-60")).is_err());
    }

    #[test]
    fn poll_secs_default_and_explicit() {
        assert_eq!(parse_poll_secs(None).unwrap(), 900);
        assert_eq!(parse_poll_secs(Some("60")).unwrap(), 60);
    }

    /// Même logique que `CARBONFR_POLL_SECS=0` (audit 2026-08) : une fenêtre
    /// nulle ferait tourner le poller sans jamais rien ingérer, silencieusement.
    #[test]
    fn poll_window_hours_zero_is_rejected_at_parse() {
        let err = parse_poll_window_hours(Some("0")).unwrap_err();
        assert!(err.to_string().contains("CARBONFR_POLL_WINDOW_HOURS"));
    }

    #[test]
    fn poll_window_hours_invalid_is_rejected() {
        assert!(parse_poll_window_hours(Some("trois")).is_err());
        assert!(parse_poll_window_hours(Some("-3")).is_err());
    }

    /// Au-delà de 24 h, `range()` pagine : le coût en appels ODRÉ par cycle
    /// ne serait plus constant.
    #[test]
    fn poll_window_hours_is_capped_at_24() {
        assert!(parse_poll_window_hours(Some("24")).is_ok());
        assert!(parse_poll_window_hours(Some("25")).is_err());
    }

    #[test]
    fn self_heal_days_default_disable_and_cap() {
        assert_eq!(parse_self_heal_days(None).unwrap(), 7);
        assert_eq!(parse_self_heal_days(Some("0")).unwrap(), 0);
        assert_eq!(parse_self_heal_days(Some(" 3 ")).unwrap(), 3);
        assert!(parse_self_heal_days(Some("8")).is_err());
        assert!(parse_self_heal_days(Some("sept")).is_err());
    }

    #[test]
    fn poll_window_hours_default_and_explicit() {
        assert_eq!(
            parse_poll_window_hours(None).unwrap(),
            INGEST_RECENT_DEFAULT_WINDOW
        );
        assert_eq!(
            parse_poll_window_hours(Some(" 6 ")).unwrap(),
            time::Duration::hours(6)
        );
    }
}
