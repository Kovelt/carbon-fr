//! La couche application : les **cas d'usage** (ports entrants).
//!
//! Chaque cas d'usage est générique sur les ports qu'il consomme (dispatch
//! statique, zéro coût) et orchestre le domaine. Aucune IO concrète ici : les
//! adapters fournissent les implémentations des ports (ADR-0002).

mod analyze_renewable_signal;
mod backfill;
mod backtest;
mod backtest_renewable;
mod cross_border;
mod find_greenest_window;
mod get_consumption;
mod get_current;
mod get_history;
mod get_stats;
mod ingest_latest;
mod renewable;
mod schedule;
mod weather;

pub use analyze_renewable_signal::{AnalyzeRenewableSignal, RenewableSignalReport};
pub use backfill::{BackfillHistory, BackfillReport};
pub use backtest::{BacktestConsumptionForecast, BacktestForecast, BacktestReport, HorizonError};
pub use backtest_renewable::{BacktestRenewable, RenewableReport};
pub use cross_border::GetCrossBorderExchanges;
pub use find_greenest_window::FindGreenestWindow;
pub use get_consumption::GetConsumptionIntensity;
pub use get_current::GetCurrentIntensity;
pub use get_history::GetIntensityHistory;
pub use get_stats::GetIntensityStats;
pub use ingest_latest::IngestLatest;
pub use renewable::CalibrateRenewable;
pub use schedule::{CarbonAwareScheduler, ScheduledWindow};
pub use weather::GetWeather;

use thiserror::Error;

use crate::domain::Region;
use crate::ports::{ForecastError, RepositoryError, SourceError};

/// Erreur d'un cas d'usage : agrège les erreurs des ports et les conditions
/// métier (donnée absente, série insuffisante).
#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Source(#[from] SourceError),

    #[error(transparent)]
    Repository(#[from] RepositoryError),

    #[error(transparent)]
    Forecast(#[from] ForecastError),

    #[error("aucune donnée disponible pour la région {0}")]
    NotFound(Region),

    #[error("série insuffisante pour déterminer un créneau")]
    InsufficientSeries,
}
