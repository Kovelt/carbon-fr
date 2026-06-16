//! Cas d'usage : intensité `acv-ademe@2` *consumption-based* (ADR-0010).
//!
//! Calcul **à la lecture** (ADR-0010 §6) : on ne stocke pas de ligne `@2` dans la
//! table primaire. On lit le **meilleur millésime** du mix FR (déjà stocké) et le
//! **contexte d'import** au même horodatage, puis on applique le calculateur de
//! domaine. La cohérence aux révisions est ainsi automatique.

use time::Duration;

use crate::domain::{
    AcvAdemeConsumption, EmissionFactors, Granularity, IntensityStats, Measurement,
    MethodologyCalculator, MethodologyContext, Region, RollupBucket, TD_LOSS_FACTOR_V1, TimeRange,
    bucketize, derive_consumption_series, summarize,
};
use crate::ports::{CrossBorderRepository, IntensityRepository};

use super::ApplicationError;

/// Sert l'intensité `acv-ademe@2` (imports valorisés à l'intensité du voisin +
/// pertes T&D). National uniquement en v1 (ADR-0010 §8).
pub struct GetConsumptionIntensity<R: IntensityRepository, C: CrossBorderRepository> {
    repository: R,
    cross_border: C,
}

impl<R: IntensityRepository, C: CrossBorderRepository> GetConsumptionIntensity<R, C> {
    pub fn new(repository: R, cross_border: C) -> Self {
        Self {
            repository,
            cross_border,
        }
    }

    /// Calcule l'intensité consommation courante. [`ApplicationError::NotFound`]
    /// si le mix ou le contexte d'import manque pour le dernier horodatage.
    pub async fn execute(&self, region: Region) -> Result<Measurement, ApplicationError> {
        // Le mix vit sur la mesure `acv-ademe@1` (dérivée et stockée à l'ingestion).
        let base = self
            .repository
            .latest(region, "acv-ademe")
            .await?
            .ok_or(ApplicationError::NotFound(region))?;
        let mix = base.mix.ok_or(ApplicationError::NotFound(region))?;

        let snapshot = self
            .cross_border
            .flows_at(base.at)
            .await?
            .ok_or(ApplicationError::NotFound(region))?;

        let factors = EmissionFactors::acv_ademe_v1();
        let ctx = MethodologyContext {
            mix: &mix,
            cross_border: Some(&snapshot.flows),
            factors: &factors,
            td_loss: TD_LOSS_FACTOR_V1,
            published: None,
        };
        let calculator = AcvAdemeConsumption;
        let intensity = calculator
            .intensity(&ctx)
            .ok_or(ApplicationError::NotFound(region))?;

        Ok(Measurement {
            at: base.at,
            region,
            intensity,
            methodology: calculator.methodology(),
            vintage: base.vintage,
            mix: Some(mix),
        })
    }

    /// Série `acv-ademe@2` sur `range` : dérivée à la lecture en joignant le mix
    /// stocké (`acv-ademe@1`) au contexte d'import (ADR-0010 §6). Les créneaux
    /// sans contexte d'import disponible sont omis.
    pub async fn history(
        &self,
        region: Region,
        range: TimeRange,
    ) -> Result<Vec<Measurement>, ApplicationError> {
        let mix = self.repository.range(region, "acv-ademe", range).await?;
        // On élargit la borne basse d'1 h pour capter le contexte d'import « au
        // plus proche ≤ » des tout premiers créneaux de l'intervalle.
        let flow_range =
            TimeRange::new(range.start() - Duration::hours(1), range.end()).unwrap_or(range);
        let snapshots = self.cross_border.flows_range(flow_range).await?;
        Ok(derive_consumption_series(
            &mix,
            &snapshots,
            &EmissionFactors::acv_ademe_v1(),
            TD_LOSS_FACTOR_V1,
        ))
    }

    /// Résumé (moyenne/min/max/effectif) `acv-ademe@2` sur `range`, ou `None` si
    /// aucun créneau n'est calculable.
    pub async fn summary(
        &self,
        region: Region,
        range: TimeRange,
    ) -> Result<Option<IntensityStats>, ApplicationError> {
        Ok(summarize(&self.history(region, range).await?))
    }

    /// Série `acv-ademe@2` agrégée par `granularity` sur `range`.
    pub async fn series(
        &self,
        region: Region,
        range: TimeRange,
        granularity: Granularity,
    ) -> Result<Vec<RollupBucket>, ApplicationError> {
        Ok(bucketize(&self.history(region, range).await?, granularity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        CarbonIntensity, CrossBorderFlow, CrossBorderFlows, CrossBorderSnapshot, GenerationMix,
        Methodology, Neighbor, Vintage,
    };
    use crate::ports::{CrossBorderRepository, IntensityRepository, RepositoryError};
    use async_trait::async_trait;
    use time::OffsetDateTime;

    fn mix() -> GenerationMix {
        GenerationMix {
            nucleaire: 40000.0,
            gaz: 1000.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 5000.0,
            eolien: 3000.0,
            solaire: 1000.0,
            bioenergies: 800.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        }
    }

    struct FakeRepo {
        at: OffsetDateTime,
    }

    #[async_trait]
    impl IntensityRepository for FakeRepo {
        async fn upsert_many(&self, _: &[Measurement]) -> Result<usize, RepositoryError> {
            Ok(0)
        }
        async fn latest(
            &self,
            region: Region,
            _methodology_id: &str,
        ) -> Result<Option<Measurement>, RepositoryError> {
            Ok(Some(Measurement {
                at: self.at,
                region,
                intensity: CarbonIntensity::new(20.0).unwrap(),
                methodology: Methodology::acv_ademe(),
                vintage: Vintage::Consolidated,
                mix: Some(mix()),
            }))
        }
        async fn range(
            &self,
            _: Region,
            _: &str,
            _: crate::domain::TimeRange,
        ) -> Result<Vec<Measurement>, RepositoryError> {
            Ok(vec![])
        }
        async fn stats(
            &self,
            _: Region,
            _: &str,
            _: crate::domain::TimeRange,
        ) -> Result<Option<crate::domain::IntensityStats>, RepositoryError> {
            Ok(None)
        }
        async fn rollup(
            &self,
            _: Region,
            _: &str,
            _: crate::domain::TimeRange,
            _: crate::domain::Granularity,
        ) -> Result<Vec<crate::domain::RollupBucket>, RepositoryError> {
            Ok(vec![])
        }
        async fn refresh_rollups(&self) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    struct FakeCross {
        snapshot: Option<CrossBorderSnapshot>,
    }

    #[async_trait]
    impl CrossBorderRepository for FakeCross {
        async fn upsert_flows(&self, _: &[CrossBorderSnapshot]) -> Result<usize, RepositoryError> {
            Ok(0)
        }
        async fn flows_at(
            &self,
            _: OffsetDateTime,
        ) -> Result<Option<CrossBorderSnapshot>, RepositoryError> {
            Ok(self.snapshot.clone())
        }
        async fn flows_range(
            &self,
            _: crate::domain::TimeRange,
        ) -> Result<Vec<CrossBorderSnapshot>, RepositoryError> {
            Ok(self.snapshot.clone().into_iter().collect())
        }
    }

    #[tokio::test]
    async fn carbon_import_raises_consumption_above_production() {
        let at = OffsetDateTime::UNIX_EPOCH;
        let repo = FakeRepo { at };
        let cross = FakeCross {
            snapshot: Some(CrossBorderSnapshot {
                at,
                flows: CrossBorderFlows::new(vec![CrossBorderFlow {
                    neighbor: Neighbor::Germany,
                    flow_mw: 5000.0,
                    neighbor_intensity: CarbonIntensity::new(400.0).unwrap(),
                }]),
            }),
        };
        let got = GetConsumptionIntensity::new(repo, cross)
            .execute(Region::National)
            .await
            .unwrap();
        assert_eq!(got.methodology, Methodology::acv_ademe_consumption());
        // Import charbon allemand → consommation au-dessus du mix FR (~quelques g).
        assert!(
            got.intensity.value() > 15.0,
            "got {}",
            got.intensity.value()
        );
    }

    #[tokio::test]
    async fn missing_import_context_is_not_found() {
        let at = OffsetDateTime::UNIX_EPOCH;
        let repo = FakeRepo { at };
        let cross = FakeCross { snapshot: None };
        let err = GetConsumptionIntensity::new(repo, cross)
            .execute(Region::National)
            .await;
        assert!(matches!(err, Err(ApplicationError::NotFound(_))));
    }
}
