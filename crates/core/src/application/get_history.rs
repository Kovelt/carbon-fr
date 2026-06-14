//! Cas d'usage : lire l'historique d'intensité d'une région sur un intervalle.

use crate::domain::{Measurement, Region, TimeRange};
use crate::ports::IntensityRepository;

use super::ApplicationError;

/// Sert la série de mesures d'une région sur `range`, pour une méthodologie
/// donnée, depuis le read-model (triée par horodatage croissant). C'est ce qui
/// alimente `/v1/intensity/date` : l'API lit la base alimentée par le poller et
/// le backfill, jamais ODRÉ en direct (ADR-0003).
pub struct GetIntensityHistory<R: IntensityRepository> {
    repository: R,
    methodology_id: String,
}

impl<R: IntensityRepository> GetIntensityHistory<R> {
    pub fn new(repository: R, methodology_id: impl Into<String>) -> Self {
        Self {
            repository,
            methodology_id: methodology_id.into(),
        }
    }

    /// Renvoie les mesures de `range` (éventuellement vide). Le bornage de la
    /// largeur d'intervalle est une préoccupation de l'adapter entrant.
    pub async fn execute(
        &self,
        region: Region,
        range: TimeRange,
    ) -> Result<Vec<Measurement>, ApplicationError> {
        Ok(self
            .repository
            .range(region, &self.methodology_id, range)
            .await?)
    }
}
