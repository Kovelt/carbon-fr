//! Cas d'usage : statistiques d'intensité (résumé et rollups) sur un intervalle.

use crate::domain::{Granularity, IntensityStats, Region, RollupBucket, TimeRange};
use crate::ports::IntensityRepository;

use super::ApplicationError;

/// Sert les statistiques d'intensité d'une région depuis le read-model : un
/// **résumé** exact (sur les mesures brutes) et, à la demande, une **série**
/// agrégée par pas (depuis les rollups). Alimente `/v1/intensity/stats`.
pub struct GetIntensityStats<R: IntensityRepository> {
    repository: R,
    methodology_id: String,
}

impl<R: IntensityRepository> GetIntensityStats<R> {
    pub fn new(repository: R, methodology_id: impl Into<String>) -> Self {
        Self {
            repository,
            methodology_id: methodology_id.into(),
        }
    }

    /// Résumé (moyenne/min/max/effectif) sur `range`, ou `None` si vide.
    pub async fn summary(
        &self,
        region: Region,
        range: TimeRange,
    ) -> Result<Option<IntensityStats>, ApplicationError> {
        Ok(self
            .repository
            .stats(region, &self.methodology_id, range)
            .await?)
    }

    /// Série agrégée par `granularity` sur `range` (depuis les rollups).
    pub async fn series(
        &self,
        region: Region,
        range: TimeRange,
        granularity: Granularity,
    ) -> Result<Vec<RollupBucket>, ApplicationError> {
        Ok(self
            .repository
            .rollup(region, &self.methodology_id, range, granularity)
            .await?)
    }
}
