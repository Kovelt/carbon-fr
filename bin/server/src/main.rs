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
//! - `backtest-sweep` : balaie une grille N × τ, classe par RMSE.
//! - `backtest-bands` : calibre et imprime les bandes d'incertitude par horizon
//!   (ADR-0011).
//! - `train` : entraîne le modèle ML GBDT (ADR-0012) → artefact, et compare
//!   `gbdt@1` à `climatology@1` au backtest (garde de promotion).
//! - `mint-key` : délivre une clé API tier gratuit (ADR-0015) — stocke son
//!   empreinte, affiche la clé une seule fois.
//!
//! ## Configuration (variables d'environnement)
//!
//! | Variable                     | Défaut         | Rôle                              |
//! |------------------------------|----------------|-----------------------------------|
//! | `DATABASE_URL`               | — (requis)     | DSN PostgreSQL                    |
//! | `CARBONFR_BIND`              | `0.0.0.0:8080` | adresse d'écoute de l'API         |
//! | `CARBONFR_POLL_SECS`         | `900` (15 min) | période d'ingestion ODRÉ          |
//! | `CARBONFR_BACKFILL_FROM`     | `2012-01-01T00:00:00Z` | début du backfill (RFC 3339) |
//! | `CARBONFR_BACKFILL_TO`       | maintenant     | fin du backfill (RFC 3339)        |
//! | `CARBONFR_BACKFILL_WINDOW_DAYS` | `90`        | largeur de tranche d'export       |
//! | `CARBONFR_BACKTEST_FROM`/`_TO` | 30 derniers jours | fenêtre de test (RFC 3339)   |
//! | `CARBONFR_BACKTEST_REGION`   | `national`     | région évaluée (slug)             |
//! | `CARBONFR_BACKTEST_METHODOLOGY` | `rte-direct` | méthodologie évaluée             |
//! | `CARBONFR_BACKTEST_ORIGIN_STEP_HOURS` | `24`  | espacement des origines           |
//! | `CARBONFR_BACKTEST_STEP_MINUTES` | `15`       | pas natif (30 pour le jeu consolidé) |
//! | `CARBONFR_BACKTEST_WEEKS`/`_TAU_HOURS` | grilles | `backtest-sweep` : N et τ balayés |
//! | `CARBONFR_BACKTEST_HORIZON_HOURS` | `24`      | `backtest-bands` : horizon calibré |
//! | `CARBONFR_BACKTEST_BAND_QUANTILE` | `0.1`     | `backtest-bands` : quantile de bord |
//! | `CARBONFR_FORECAST_CALIBRATE_WEEKS` | `8`     | auto-calibration au démarrage (0 = off) |
//! | `CARBONFR_RENEWABLE_CALIBRATE_WEEKS` | `52`   | calibration `/v1/renewable` au démarrage (0 = off) |
//! | `CARBONFR_TRAIN_FROM`/`_TO`  | 120 j av. test | `train` : période d'entraînement   |
//! | `CARBONFR_TRAIN_ORIGIN_STEP_HOURS` | `6`      | `train` : espacement des origines  |
//! | `CARBONFR_GBDT_MODEL`        | `gbdt.model`   | `train` : chemin de l'artefact GBDT |
//! | `CARBONFR_RATELIMIT_ENABLED` | `0` (off)      | tier hébergé : auth+quota (ADR-0015) |
//! | `CARBONFR_RATELIMIT_ANON_PER_MIN` | `60`      | quota anonyme (req/min)            |
//! | `CARBONFR_RATELIMIT_FREE_PER_MIN` | `600`     | quota clé gratuite (req/min)        |
//! | `CARBONFR_KEY_LABEL`         | `` (vide)      | `mint-key` : libellé de la clé      |
//! | `CARBONFR_TRUST_PROXY`       | `0` (off)      | faire confiance à `X-Forwarded-For` (derrière un reverse proxy) |
//! | `CARBONFR_DB_MAX_CONNECTIONS` | `20`         | taille du pool PostgreSQL           |
//! | `CARBONFR_VISIT_SALT`        | `carbon-fr` (⚠ requis si TRUST_PROXY) | sel du hachage des IP visiteurs |
//! | `CARBONFR_LOG_FORMAT`        | (texte)        | `json` pour des logs structurés (prod) |
//! | `RUST_LOG`                   | `info`         | filtre de logs (`tracing`)        |

mod metrics;

use std::net::SocketAddr;

use anyhow::Context;
use carbonfr_adapter_entsoe::EntsoeClient;
use carbonfr_adapter_forecast::{AcvAdemeForecaster, ClimatologyForecaster};
use carbonfr_adapter_gbdt::{
    GbdtForecaster, GbdtHyperParams, build_training_examples, train_model,
};
use carbonfr_adapter_http::{
    AppState, AuthConfig, AuthState, ForecastState, StreamState, enforce, key_fingerprint, router,
};
use carbonfr_adapter_meteo::OpenMeteoClient;
use carbonfr_adapter_odre::OdreClient;
use carbonfr_adapter_postgres::PgIntensityRepository;
use carbonfr_adapter_webhook::HttpNotifier;
use carbonfr_core::application::{
    AnalyzeRenewableSignal, BackfillHistory, BacktestConsumptionForecast, BacktestForecast,
    BacktestRenewable, BacktestReport, CalibrateRenewable, IngestLatest,
};
use carbonfr_core::domain::{
    ACV_FORECAST_ID, ACV_FORECAST_VERSION, CLIMATOLOGY_ID, CLIMATOLOGY_VERSION, ClimatologyParams,
    ErrorMetrics, IntensityUpdate, Region, TimeRange, hmac_sha256_hex, render_webhook_payload,
    should_fire,
};
use carbonfr_core::ports::{
    ApiKeyRepository, ApiTier, ConsumptionRepository, ConsumptionSource, CrossBorderRepository,
    CrossBorderSource, Eco2mixArchive, Eco2mixSource, IntensityRepository, Notifier,
    SubscriptionRepository, WeatherForecastSource, WeatherRepository, WebhookDelivery,
};
use metrics::Metrics;
use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, Month, OffsetDateTime};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
        Some("train") => run_train().await,
        Some("mint-key") => run_mint_key().await,
        Some(other) => {
            anyhow::bail!(
                "sous-commande inconnue : « {other} » (attendu : `backfill`, `backtest`, `backtest-acv`, `backtest-sweep`, `backtest-bands`, `backtest-renewable`, `analyze-renewable-signal`, `train`, `mint-key`, ou aucune pour servir l'API)"
            )
        }
    }
}

/// Mode service : poller temps réel + API HTTP.
async fn run_server() -> anyhow::Result<()> {
    let config = ServerConfig::from_env()?;

    let repo = connect_repo(&config.database_url).await?;

    // Poller unique : un seul composant tape les sources amont, l'API sert
    // depuis la base. ODRÉ (intensité + charge) et Open-Meteo (prévision météo).
    let source = OdreClient::new().context("initialisation du client ODRÉ")?;
    let weather = OpenMeteoClient::new().context("initialisation du client Open-Meteo")?;
    // ENTSO-E : optionnel (ADR-0010) — seulement si `CARBONFR_ENTSOE_TOKEN` est
    // défini. Sans token, `acv-ademe@2` reste calculable mais sans donnée d'import.
    let cross_border = match EntsoeClient::from_env() {
        Ok(client) => {
            info!("source d'import ENTSO-E configurée (acv-ademe@2 alimentée)");
            Some(client)
        }
        Err(err) => {
            info!(raison = %err, "ENTSO-E non configuré : acv-ademe@2 sans contexte d'import");
            None
        }
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
        metrics.clone(),
    );

    // Watcher de webhooks (ADR-0016) : s'abonne au même flux que le SSE, détecte
    // les franchissements de seuil et livre des notifications signées.
    let webhook_watcher =
        spawn_webhook_watcher(updates_tx.subscribe(), repo.clone(), HttpNotifier::new());

    // Prévision (ADR-0009) : modèle climatology@1 alimenté par le même
    // repository. Intervalles **calibrés** au démarrage par quantiles de résidus
    // par horizon (ADR-0011), repli sur la dispersion par créneau si l'historique
    // récent est insuffisant. Son identité versionnée est annoncée au client.
    let forecaster = build_calibrated_forecaster(repo.clone()).await;
    let model = format!("{CLIMATOLOGY_ID}@{CLIMATOLOGY_VERSION}");
    // Prévision `acv-ademe@2` (ADR-0013) : climatologie des entrées (mix + import)
    // + calculateur. Servie via `?methodology=acv-ademe&version=2`.
    let acv_forecaster = build_calibrated_acv_forecaster(repo.clone()).await;
    let acv_model = format!("{ACV_FORECAST_ID}@{ACV_FORECAST_VERSION}");
    let forecast_state = ForecastState::new(forecaster, model)
        .with_consumption(std::sync::Arc::new(acv_forecaster), acv_model);

    let renewable_model = build_calibrated_renewable_model(repo.clone()).await;
    let mut state = AppState::new(repo.clone())
        .with_trust_proxy(config.trust_proxy)
        .with_renewable_model(renewable_model);
    if let Some(salt) = config.visit_salt {
        state = state.with_visit_salt(salt);
    }
    let stream_state = StreamState::new(updates_tx);
    // `/metrics` (hors contrat `/v1`, comme `/health`) : exposition Prometheus en
    // texte, pas du JSON versionné → fusionnée ici plutôt que dans le routeur de
    // l'adapter. En prod, restreindre l'accès au scrapeur côté reverse proxy.
    let metrics_router = axum::Router::new()
        .route("/metrics", axum::routing::get(serve_metrics))
        .with_state(metrics);
    let mut app = router(state, forecast_state, stream_state).merge(metrics_router);

    // Tier hébergé (ADR-0015) : middleware clés API + quota, **opt-in**. Désactivé
    // par défaut → l'API reste anonyme et sans limite (parité self-hosting).
    if let Some(auth_state) = build_auth_state(repo.clone()) {
        info!("tier hébergé activé : auth par clé + quota par minute");
        app = app.layer(axum::middleware::from_fn_with_state(auth_state, enforce));
    }
    let listener = TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("écoute sur {}", config.bind))?;
    info!(addr = %config.bind, "API à l'écoute");

    // Supervision **fail-fast** : le serveur s'arrête sur signal (arrêt gracieux) ;
    // mais si le poller ou le watcher meurt (panique → boucle infinie terminée),
    // on sort en erreur plutôt que de continuer en silence (donnée gelée /
    // webhooks muets). Le superviseur (systemd `Restart=on-failure`) relance.
    let mut poller = poller;
    let mut webhook_watcher = webhook_watcher;
    let serve = async {
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await
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
    };

    poller.abort();
    webhook_watcher.abort();
    serve_result
}

/// `GET /metrics` — exposition Prometheus (text format 0.0.4). Hors du contrat
/// `/v1` (endpoint d'exploitation, comme `/health`).
async fn serve_metrics(
    axum::extract::State(metrics): axum::extract::State<Metrics>,
) -> impl axum::response::IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        metrics.render(),
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
    };
    let keys: std::sync::Arc<dyn ApiKeyRepository> = std::sync::Arc::new(repo);
    Some(AuthState::new(keys, config))
}

/// Mode `mint-key` : génère une clé API gratuite, en stocke l'empreinte, et
/// l'affiche **une seule fois** (ADR-0015). Libellé via `CARBONFR_KEY_LABEL`.
async fn run_mint_key() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let label = std::env::var("CARBONFR_KEY_LABEL").unwrap_or_default();

    let key = generate_api_key().context("génération de la clé")?;
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

/// Génère une clé aléatoire `cfr_<64 hex>` (32 octets de `/dev/urandom`).
fn generate_api_key() -> anyhow::Result<String> {
    use std::io::Read;
    let mut buf = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .context("ouverture de /dev/urandom")?
        .read_exact(&mut buf)
        .context("lecture d'entropie")?;
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("cfr_{hex}"))
}

/// Mode backfill : rapatriement de l'historique national, puis arrêt.
async fn run_backfill() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let archive = OdreClient::new().context("initialisation du client ODRÉ")?;

    let (range, window) = backfill_params()?;
    let backfill = BackfillHistory::new(archive.clone(), repo.clone(), window);

    info!(from = %range.start(), to = %range.end(), window_days = window.whole_days(), "backfill historique national démarré");
    let report = backfill
        .execute(range)
        .await
        .context("backfill historique")?;
    info!(
        read = report.read,
        written = report.written,
        windows = report.windows,
        "backfill terminé"
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
    // L'API Historical Forecast d'Open-Meteo ne couvre que **2016-01-01→** : on
    // borne le départ pour ne pas émettre de requêtes vouées au 400 sur tout
    // l'historique antérieur (sans ce garde-fou, 2012→2016 = ~49 tranches inutiles).
    let weather_min = OffsetDateTime::new_utc(
        time::Date::from_calendar_date(2016, time::Month::January, 1)
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
    // Météo prévue (archive backfillée) sur la période, indexée par échéance —
    // le `run_at = valid_at − 24 h` garantit l'anti-fuite (horizons ≤ 24 h).
    let weather_rows = repo
        .weather_range(read)
        .await
        .context("lecture de la météo")?;
    let mut weather: HashMap<OffsetDateTime, (OffsetDateTime, f64, f64)> = HashMap::new();
    for w in weather_rows {
        match weather.get(&w.valid_at) {
            Some((run, _, _)) if *run >= w.run_at => {}
            _ => {
                weather.insert(w.valid_at, (w.run_at, w.wind, w.irradiance));
            }
        }
    }
    let weather: HashMap<OffsetDateTime, (f64, f64)> = weather
        .into_iter()
        .map(|(v, (_, wind, irr))| (v, (wind, irr)))
        .collect();

    // Origines d'entraînement : du début +1 sem. à la fin −24 h.
    let mut origins = Vec::new();
    let mut o = train_from + Duration::weeks(1);
    while o < train_to - Duration::hours(24) {
        origins.push(o);
        o += Duration::hours(origin_step_hours);
    }
    let examples = build_training_examples(
        &intensity,
        &weather,
        10, // fenêtre glissante (semaines), identique à l'inférence
        &origins,
        params.step,
        Duration::hours(24),
    );
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
    visit_salt: Option<String>,
    trust_proxy: bool,
}

impl ServerConfig {
    fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .context("la variable DATABASE_URL est requise (DSN PostgreSQL)")?;

        let bind: SocketAddr = std::env::var("CARBONFR_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse()
            .context("CARBONFR_BIND : adresse d'écoute invalide")?;

        let poll_secs = std::env::var("CARBONFR_POLL_SECS")
            .ok()
            .map(|raw| raw.parse::<u64>())
            .transpose()
            .context("CARBONFR_POLL_SECS : durée invalide")?
            .unwrap_or(900);

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
            visit_salt,
            trust_proxy,
        })
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
#[allow(clippy::too_many_arguments)]
fn spawn_poller<S, W, C, R>(
    source: S,
    weather: W,
    cross_border: Option<C>,
    repo: R,
    updates: tokio::sync::broadcast::Sender<IntensityUpdate>,
    interval: std::time::Duration,
    metrics: Metrics,
) -> JoinHandle<()>
where
    S: Eco2mixSource + ConsumptionSource + Clone + 'static,
    W: WeatherForecastSource + 'static,
    C: CrossBorderSource + 'static,
    R: IntensityRepository
        + ConsumptionRepository
        + WeatherRepository
        + CrossBorderRepository
        + Clone
        + 'static,
{
    let ingest = IngestLatest::new(source.clone(), repo.clone());
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;

            // National (rte-direct + acv-ademe dérivée) puis les 12 régions
            // (acv-ademe). Une région en échec ne bloque pas les autres.
            let mut written = 0usize;
            for region in std::iter::once(Region::National).chain(Region::METROPOLITAN) {
                // Un appel ODRÉ par région : compté pour suivre le quota (50k/mois).
                metrics.add_upstream_odre(1);
                match ingest.execute(region).await {
                    Ok(n) => written += n,
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
            // sans conséquence (canal sans rétention forte).
            if let Ok(Some(m)) = repo.latest(Region::National, "rte-direct").await {
                metrics.set_last_measurement(m.at.unix_timestamp());
                let _ = updates.send(IntensityUpdate::from_measurement(&m));
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
                            Ok(n) => info!(flows = n, "ingestion contexte d'import (ENTSO-E)"),
                            Err(err) => {
                                warn!(error = %err, "échec d'écriture du contexte d'import")
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(err) => warn!(error = %err, "échec de récupération ENTSO-E"),
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
/// chemin d'évaluation.
fn spawn_webhook_watcher<R, N>(
    mut updates: tokio::sync::broadcast::Receiver<IntensityUpdate>,
    repo: R,
    notifier: N,
) -> JoinHandle<()>
where
    R: SubscriptionRepository + 'static,
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
                let id = sub.id.clone();
                tokio::spawn(async move {
                    let _permit = permit; // relâché à la fin de la livraison
                    if let Err(err) = notifier.deliver(&delivery).await {
                        warn!(subscription = %id, error = %err, "livraison webhook échouée");
                    } else {
                        info!(subscription = %id, "webhook livré");
                    }
                });
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
