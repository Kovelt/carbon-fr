//! # carbonfr-server — composition root
//!
//! Le seul composant qui connaît les implémentations concrètes des ports et les
//! assemble (ADR-0002) :
//!
//! 1. ouvre le pool PostgreSQL et applique les migrations ;
//! 2. lance le **poller unique** qui alimente la base depuis ODRÉ (ADR-0003) ;
//! 3. sert l'API HTTP versionnée `/v1`, qui lit depuis la base.
//!
//! ## Configuration (variables d'environnement)
//!
//! | Variable             | Défaut         | Rôle                                  |
//! |----------------------|----------------|---------------------------------------|
//! | `DATABASE_URL`       | — (requis)     | DSN PostgreSQL                        |
//! | `CARBONFR_BIND`      | `0.0.0.0:8080` | adresse d'écoute de l'API             |
//! | `CARBONFR_POLL_SECS` | `900` (15 min) | période d'ingestion ODRÉ              |
//! | `RUST_LOG`           | `info`         | filtre de logs (`tracing`)            |

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;
use carbonfr_adapter_http::{AppState, router};
use carbonfr_adapter_odre::OdreClient;
use carbonfr_adapter_postgres::PgIntensityRepository;
use carbonfr_core::application::IngestLatest;
use carbonfr_core::domain::Region;
use carbonfr_core::ports::{Eco2mixSource, IntensityRepository};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::from_env()?;

    // Adapter de persistance + schéma à jour.
    let repo = PgIntensityRepository::connect(&config.database_url)
        .await
        .context("connexion à PostgreSQL")?;
    repo.migrate().await.context("application des migrations")?;
    info!("base prête (migrations appliquées)");

    // Poller unique : un seul composant tape ODRÉ, l'API sert depuis la base.
    let source = OdreClient::new().context("initialisation du client ODRÉ")?;
    let poller = spawn_poller(source, repo.clone(), config.poll_interval);

    // API HTTP.
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

/// Configuration résolue depuis l'environnement.
struct Config {
    database_url: String,
    bind: SocketAddr,
    poll_interval: Duration,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .context("la variable DATABASE_URL est requise (DSN PostgreSQL)")?;

        let bind: SocketAddr = std::env::var("CARBONFR_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse()
            .context("CARBONFR_BIND : adresse d'écoute invalide")?;

        let poll_interval = std::env::var("CARBONFR_POLL_SECS")
            .ok()
            .map(|raw| raw.parse::<u64>())
            .transpose()
            .context("CARBONFR_POLL_SECS : durée invalide")?
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(900));

        Ok(Self {
            database_url,
            bind,
            poll_interval,
        })
    }
}

/// Démarre la tâche d'ingestion périodique. La première itération s'exécute
/// immédiatement. Une erreur d'ingestion est journalisée sans interrompre la
/// boucle (la donnée sera rattrapée à la prochaine itération ou au backfill).
fn spawn_poller<S, R>(source: S, repo: R, interval: Duration) -> JoinHandle<()>
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
