//! Cas d'usage : lire l'intensité carbone courante d'une région.

use crate::domain::{Measurement, Region};
use crate::ports::IntensityRepository;

use super::ApplicationError;

/// Sert la dernière mesure connue (meilleur millésime) depuis le read-model,
/// pour une méthodologie donnée. C'est ce qui alimente `/v1/intensity/now` :
/// l'API lit la base, elle ne tape jamais RTE en direct (ADR-0003).
pub struct GetCurrentIntensity<R: IntensityRepository> {
    repository: R,
    methodology_id: String,
}

impl<R: IntensityRepository> GetCurrentIntensity<R> {
    pub fn new(repository: R, methodology_id: impl Into<String>) -> Self {
        Self {
            repository,
            methodology_id: methodology_id.into(),
        }
    }

    /// Renvoie la dernière mesure, ou [`ApplicationError::NotFound`] si aucune
    /// donnée n'est disponible pour cette région et cette méthodologie.
    pub async fn execute(&self, region: Region) -> Result<Measurement, ApplicationError> {
        self.repository
            .latest(region, &self.methodology_id)
            .await?
            .ok_or(ApplicationError::NotFound(region))
    }

    /// Dernière mesure de **chaque** région disposant d'une donnée — national
    /// puis les 12 régions, dans l'ordre de `Region::METROPOLITAN` — en un seul
    /// appel au port ([`IntensityRepository::latest_all`]). Vide si aucune
    /// région n'a de donnée : contrairement à [`execute`](Self::execute), qui
    /// échoue en `NotFound` pour une région précise, c'est l'adapter qui décide
    /// de la réponse (liste vide). Alimente `GET /v1/intensity/now/all` (même
    /// patron que `BackfillHistory::execute_regional` : une variante « toutes
    /// régions » sur le même cas d'usage).
    pub async fn execute_all(&self) -> Result<Vec<Measurement>, ApplicationError> {
        Ok(self.repository.latest_all(&self.methodology_id).await?)
    }
}
