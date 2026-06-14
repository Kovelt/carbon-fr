//! La couche application : les **cas d'usage** (ports entrants).
//!
//! Chaque cas d'usage est générique sur les ports qu'il consomme (dispatch
//! statique, zéro coût) et orchestre le domaine. Aucune IO concrète ici : les
//! adapters fournissent les implémentations des ports (ADR-0002).

mod backfill;
mod find_greenest_window;
mod get_current;
mod get_history;
mod ingest_latest;

pub use backfill::{BackfillHistory, BackfillReport};
pub use find_greenest_window::FindGreenestWindow;
pub use get_current::GetCurrentIntensity;
pub use get_history::GetIntensityHistory;
pub use ingest_latest::IngestLatest;

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
