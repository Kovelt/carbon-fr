//! # carbonfr-server — composition root
//!
//! Le seul composant qui connaît les implémentations concrètes des ports et les
//! assemble (ADR-0002). Trois modes selon la sous-commande :
//!
//! - (aucune) : sert l'API et lance le **poller** (temps réel).
//! - `backfill` : rapatrie l'historique par **export de masse** (ADR-0003),
//!   puis s'arrête.
//! - `backtest` : évalue le modèle de prévision `climatology@1` sur l'historique
//!   (walk-forward), imprime MAE/RMSE (modèle vs persistance), puis s'arrête
//!   (ADR-0009).
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
//! | `RUST_LOG`                   | `info`         | filtre de logs (`tracing`)        |

use std::net::SocketAddr;

use anyhow::Context;
use carbonfr_adapter_forecast::ClimatologyForecaster;
use carbonfr_adapter_http::{AppState, ForecastState, router};
use carbonfr_adapter_odre::OdreClient;
use carbonfr_adapter_postgres::PgIntensityRepository;
use carbonfr_core::application::{BackfillHistory, BacktestForecast, BacktestReport, IngestLatest};
use carbonfr_core::domain::{
    CLIMATOLOGY_ID, CLIMATOLOGY_VERSION, ClimatologyParams, ErrorMetrics, Region, TimeRange,
};
use carbonfr_core::ports::{Eco2mixSource, IntensityRepository};
use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, Month, OffsetDateTime};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    match std::env::args().nth(1).as_deref() {
        None => run_server().await,
        Some("backfill") => run_backfill().await,
        Some("backtest") => run_backtest().await,
        Some("backtest-sweep") => run_backtest_sweep().await,
        Some(other) => {
            anyhow::bail!(
                "sous-commande inconnue : « {other} » (attendu : `backfill`, `backtest`, `backtest-sweep`, ou aucune pour servir l'API)"
            )
        }
    }
}

/// Mode service : poller temps réel + API HTTP.
async fn run_server() -> anyhow::Result<()> {
    let config = ServerConfig::from_env()?;

    let repo = connect_repo(&config.database_url).await?;

    // Poller unique : un seul composant tape ODRÉ, l'API sert depuis la base.
    let source = OdreClient::new().context("initialisation du client ODRÉ")?;
    let poller = spawn_poller(source, repo.clone(), config.poll_interval);

    // Prévision (ADR-0009) : modèle climatology@1 alimenté par le même
    // repository. Son identité versionnée est annoncée au client.
    let forecaster = ClimatologyForecaster::new(repo.clone());
    let model = format!("{CLIMATOLOGY_ID}@{CLIMATOLOGY_VERSION}");
    let forecast_state = ForecastState::new(forecaster, model);

    let mut state = AppState::new(repo);
    if let Some(salt) = config.visit_salt {
        state = state.with_visit_salt(salt);
    }
    let app = router(state, forecast_state);
    let listener = TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("écoute sur {}", config.bind))?;
    info!(addr = %config.bind, "API à l'écoute");

    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serveur HTTP");

    poller.abort();
    serve_result
}

/// Mode backfill : rapatriement de l'historique national, puis arrêt.
async fn run_backfill() -> anyhow::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").context("la variable DATABASE_URL est requise")?;
    let repo = connect_repo(&database_url).await?;
    let archive = OdreClient::new().context("initialisation du client ODRÉ")?;

    let (range, window) = backfill_params()?;
    let backfill = BackfillHistory::new(archive, repo.clone(), window);

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

    // Rollups à jour après le backfill massif.
    repo.refresh_rollups()
        .await
        .context("rafraîchissement des rollups")?;
    info!("rollups rafraîchis");
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

/// Ouvre le pool PostgreSQL et applique les migrations.
async fn connect_repo(database_url: &str) -> anyhow::Result<PgIntensityRepository> {
    let repo = PgIntensityRepository::connect(database_url)
        .await
        .context("connexion à PostgreSQL")?;
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

        Ok(Self {
            database_url,
            bind,
            poll_interval: std::time::Duration::from_secs(poll_secs),
            visit_salt: std::env::var("CARBONFR_VISIT_SALT").ok(),
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
fn spawn_poller<S, R>(source: S, repo: R, interval: std::time::Duration) -> JoinHandle<()>
where
    S: Eco2mixSource + 'static,
    R: IntensityRepository + Clone + 'static,
{
    let ingest = IngestLatest::new(source, repo.clone());
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;

            // National (rte-direct + acv-ademe dérivée) puis les 12 régions
            // (acv-ademe). Une région en échec ne bloque pas les autres.
            let mut written = 0usize;
            for region in std::iter::once(Region::National).chain(Region::METROPOLITAN) {
                match ingest.execute(region).await {
                    Ok(n) => written += n,
                    Err(err) => {
                        warn!(region = region.slug(), error = %err, "échec d'ingestion ODRÉ")
                    }
                }
            }
            info!(written, "ingestion ODRÉ (national + régions)");

            // Rollups rafraîchis une fois par cycle si la donnée a changé.
            if written > 0
                && let Err(err) = repo.refresh_rollups().await
            {
                warn!(error = %err, "échec du rafraîchissement des rollups");
            }
        }
    })
}

/// Attend Ctrl-C (SIGINT) pour un arrêt propre.
async fn shutdown_signal() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        error!(error = %err, "écoute du signal d'arrêt impossible");
        return;
    }
    info!("arrêt demandé, fermeture en cours");
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
