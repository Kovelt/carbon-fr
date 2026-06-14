//! Cas d'usage : ingérer la dernière mesure d'une région (cœur du poller).

use crate::domain::Region;
use crate::ports::{Eco2mixSource, IntensityRepository};

use super::ApplicationError;

/// Récupère la dernière mesure depuis la source et la persiste via l'upsert
/// conditionnel au millésime (ADR-0003, ADR-0006). C'est l'opération que le
/// poller exécute périodiquement.
pub struct IngestLatest<S: Eco2mixSource, R: IntensityRepository> {
    source: S,
    repository: R,
}

impl<S: Eco2mixSource, R: IntensityRepository> IngestLatest<S, R> {
    pub fn new(source: S, repository: R) -> Self {
        Self { source, repository }
    }

    /// Retourne le nombre de mesures écrites (0 si la donnée déjà stockée
    /// était d'un millésime supérieur).
    pub async fn execute(&self, region: Region) -> Result<usize, ApplicationError> {
        let measurement = self.source.latest(region).await?;
        let written = self
            .repository
            .upsert_many(std::slice::from_ref(&measurement))
            .await?;
        Ok(written)
    }
}
