//! Cas d'usage : ingérer la dernière mesure d'une région (cœur du poller).

use crate::domain::{Methodology, Region, derive_acv_ademe};
use crate::ports::{Eco2mixSource, IntensityRepository};

use super::ApplicationError;

/// Récupère la dernière mesure depuis la source et la persiste via l'upsert
/// conditionnel au millésime (ADR-0003, ADR-0006). C'est l'opération que le
/// poller exécute périodiquement.
///
/// Quand la mesure porte un mix de production, on **dérive et stocke aussi** la
/// mesure `acv-ademe` (cycle de vie, ADR-0008) au même horodatage.
pub struct IngestLatest<S: Eco2mixSource, R: IntensityRepository> {
    source: S,
    repository: R,
}

impl<S: Eco2mixSource, R: IntensityRepository> IngestLatest<S, R> {
    pub fn new(source: S, repository: R) -> Self {
        Self { source, repository }
    }

    /// Retourne le nombre de mesures écrites (0 si la donnée déjà stockée
    /// était d'un millésime supérieur). Compte la mesure source et, le cas
    /// échéant, sa dérivée `acv-ademe`.
    pub async fn execute(&self, region: Region) -> Result<usize, ApplicationError> {
        let measurement = self.source.latest(region).await?;
        let mut batch = vec![measurement];
        // Dérive `acv-ademe` sauf si la mesure l'est déjà (cas régional, où la
        // source ne fournit que l'intensité dérivée).
        if batch[0].methodology != Methodology::acv_ademe()
            && let Some(acv) = derive_acv_ademe(&batch[0])
        {
            batch.push(acv);
        }
        let written = self.repository.upsert_many(&batch).await?;
        Ok(written)
    }
}
