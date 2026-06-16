//! Cas d'usage : exposition des **échanges transfrontaliers** — flux signés par
//! frontière + intensité carbone (cycle de vie) du voisin (ADR-0017).
//!
//! La donnée est déjà ingérée par le poller (port `CrossBorderSource` → store)
//! pour `acv-ademe@2` ; ce cas d'usage la **sert** telle quelle, sans recalcul.
//! Le « maintenant » est ancré sur la dernière mesure nationale, pour un
//! horodatage cohérent avec `/v1/intensity/now` et `/v1/mix`.

use crate::domain::{CrossBorderSnapshot, Region, TimeRange};
use crate::ports::{CrossBorderRepository, IntensityRepository};

use super::ApplicationError;

/// Sert les échanges transfrontaliers (flux net signé par frontière + intensité
/// du voisin), au pas quart d'heure.
pub struct GetCrossBorderExchanges<R: IntensityRepository, C: CrossBorderRepository> {
    repository: R,
    cross_border: C,
}

impl<R: IntensityRepository, C: CrossBorderRepository> GetCrossBorderExchanges<R, C> {
    pub fn new(repository: R, cross_border: C) -> Self {
        Self {
            repository,
            cross_border,
        }
    }

    /// Dernier snapshot d'échanges, **aligné sur la dernière mesure nationale**
    /// (même horodatage que `/v1/intensity/now`). [`ApplicationError::NotFound`]
    /// si aucune mesure ou aucun contexte d'import n'est disponible.
    pub async fn latest(&self) -> Result<CrossBorderSnapshot, ApplicationError> {
        let base = self
            .repository
            .latest(Region::National, "rte-direct")
            .await?
            .ok_or(ApplicationError::NotFound(Region::National))?;
        self.cross_border
            .flows_at(base.at)
            .await?
            .ok_or(ApplicationError::NotFound(Region::National))
    }

    /// Série des snapshots d'échanges sur un intervalle (triés par `at`).
    pub async fn range(
        &self,
        range: TimeRange,
    ) -> Result<Vec<CrossBorderSnapshot>, ApplicationError> {
        Ok(self.cross_border.flows_range(range).await?)
    }
}
