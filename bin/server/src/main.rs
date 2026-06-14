//! # carbonfr-server — composition root
//!
//! Le seul composant qui connaît les implémentations concrètes des ports et les
//! assemble (ADR-0002). Deux modes selon la sous-commande :
//!
//! - (aucune) : sert l'API et lance le **poller** (temps réel).
//! - `backfill` : rapatrie l'historique par **export de masse** (ADR-0003),
//!   puis s'arrête.
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
//! | `RUST_LOG`                   | `info`         | filtre de logs (`tracing`)        |

use std::net::SocketAddr;

use anyhow::Context;
use carbonfr_adapter_http::{AppState, router};
use carbonfr_adapter_odre::OdreClient;
use carbonfr_adapter_postgres::PgIntensityRepository;
use carbonfr_core::application::{BackfillHistory, IngestLatest};
use carbonfr_core::domain::{Region, TimeRange};
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
        Some(other) => {
            anyhow::bail!(
                "sous-commande inconnue : « {other} » (attendu : `backfill`, ou aucune pour servir l'API)"
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

    let app = router(AppState::new(repo));
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
    let backfill = BackfillHistory::new(archive, repo, window);

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
    Ok(())
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
    R: IntensityRepository + 'static,
{
    let ingest = IngestLatest::new(source, repo);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            match ingest.execute(Region::National).await {
                Ok(written) => info!(written, "ingestion ODRÉ (national)"),
                Err(err) => warn!(error = %err, "échec d'ingestion ODRÉ"),
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
