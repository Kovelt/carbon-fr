//! Cas d'usage : décomposition du **prix de l'électricité** ancrée sur le TRV
//! (ADR-0023) — énergie (spot day-ahead) + acheminement (TURPE) + taxes
//! (accise + TVA) + résidu commercialisation, avec contexte (mix par filière +
//! technologie marginale estimée).
//!
//! Calcul **à la lecture** : on lit le mix national stocké (déjà ingéré) et le
//! prix spot au même horodatage, puis on applique la construction réglementaire
//! versionnée (constantes de domaine, ADR-0023 §2). National uniquement (le TRV
//! et la zone de marché spot sont nationaux).

use time::Duration;

use crate::domain::{
    PriceBreakdown, Region, TimeRange, TrvReference, price_breakdown, price_series,
};
use crate::ports::{IntensityRepository, SpotPriceRepository};

use super::ApplicationError;

/// Méthodologie de la mesure servant d'ancre (horodatage + mix). `rte-direct`
/// est le national canonique, aligné sur `/v1/intensity/now` et `/v1/mix`.
const ANCHOR_METHODOLOGY: &str = "rte-direct";

/// Fraîcheur maximale du prix spot servi en « courant ». Le day-ahead est au pas
/// quart d'heure (MTU 15 min, validé live 2026-06-20) et publié chaque jour ;
/// au-delà de cette tolérance, le prix le plus
/// récent disponible est considéré **périmé** (ENTSO-E muet) → `NotFound` plutôt
/// que servir un prix d'il y a plusieurs jours comme s'il était courant.
const MAX_SPOT_STALENESS: Duration = Duration::hours(6);

/// Sert la décomposition de prix TRV (national). Les valeurs réglementaires
/// (TURPE, accise/TVA, résidu) sont versionnées par période de validité.
pub struct GetElectricityPrice<R: IntensityRepository, P: SpotPriceRepository> {
    repository: R,
    spot: P,
}

impl<R: IntensityRepository, P: SpotPriceRepository> GetElectricityPrice<R, P> {
    pub fn new(repository: R, spot: P) -> Self {
        Self { repository, spot }
    }

    /// Décomposition courante, ancrée sur la dernière mesure nationale.
    /// [`ApplicationError::NotFound`] si le mix ou le prix spot manque.
    pub async fn current(&self, region: Region) -> Result<PriceBreakdown, ApplicationError> {
        let base = self
            .repository
            .latest(region, ANCHOR_METHODOLOGY)
            .await?
            .ok_or(ApplicationError::NotFound(region))?;
        let mix = base.mix.ok_or(ApplicationError::NotFound(region))?;

        let spot = self
            .spot
            .price_at(base.at)
            .await?
            .ok_or(ApplicationError::NotFound(region))?;

        // Fraîcheur : si le dernier prix spot disponible est trop ancien (ENTSO-E
        // muet), on ne le sert pas comme « courant » — `NotFound` plutôt qu'un
        // prix périmé présenté comme actuel.
        if base.at - spot.at > MAX_SPOT_STALENESS {
            return Err(ApplicationError::NotFound(region));
        }

        Ok(price_breakdown(
            base.at,
            region,
            &spot,
            &mix,
            &TrvReference::trv_2026(),
        ))
    }

    /// Série de décompositions sur `range` (pour la primitive « cheapest +
    /// greenest window », ADR-0023). Jointure mix × prix spot au plus proche ≤ ;
    /// les créneaux sans prix spot antérieur disponible sont omis.
    pub async fn history(
        &self,
        region: Region,
        range: TimeRange,
    ) -> Result<Vec<PriceBreakdown>, ApplicationError> {
        let measurements = self
            .repository
            .range(region, ANCHOR_METHODOLOGY, range)
            .await?;
        // On élargit la borne basse de 24 h pour capter le prix « au plus proche
        // ≤ » des premiers créneaux (le spot day-ahead est au pas quart d'heure).
        let price_range =
            TimeRange::new(range.start() - Duration::hours(24), range.end()).unwrap_or(range);
        let spots = self.spot.price_range(price_range).await?;
        Ok(price_series(
            &measurements,
            &spots,
            &TrvReference::trv_2026(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        CarbonIntensity, GenerationMix, Measurement, Methodology, PriceComponentKind, SpotPrice,
        Vintage,
    };
    use crate::ports::{RepositoryError, SpotPriceRepository};
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
        with_mix: bool,
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
                intensity: CarbonIntensity::new(40.0).unwrap(),
                methodology: Methodology::rte_direct(),
                vintage: Vintage::Consolidated,
                mix: self.with_mix.then(mix),
            }))
        }
        async fn range(
            &self,
            _: Region,
            _: &str,
            _: TimeRange,
        ) -> Result<Vec<Measurement>, RepositoryError> {
            Ok(vec![])
        }
        async fn stats(
            &self,
            _: Region,
            _: &str,
            _: TimeRange,
        ) -> Result<Option<crate::domain::IntensityStats>, RepositoryError> {
            Ok(None)
        }
        async fn rollup(
            &self,
            _: Region,
            _: &str,
            _: TimeRange,
            _: crate::domain::Granularity,
        ) -> Result<Vec<crate::domain::RollupBucket>, RepositoryError> {
            Ok(vec![])
        }
        async fn refresh_rollups(&self) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    struct FakeSpot {
        price: Option<SpotPrice>,
    }

    #[async_trait]
    impl SpotPriceRepository for FakeSpot {
        async fn upsert_prices(&self, _: &[SpotPrice]) -> Result<usize, RepositoryError> {
            Ok(0)
        }
        async fn price_at(&self, _: OffsetDateTime) -> Result<Option<SpotPrice>, RepositoryError> {
            Ok(self.price)
        }
        async fn price_range(&self, _: TimeRange) -> Result<Vec<SpotPrice>, RepositoryError> {
            Ok(self.price.into_iter().collect())
        }
    }

    #[tokio::test]
    async fn current_decomposes_with_spot_energy() {
        let at = OffsetDateTime::UNIX_EPOCH;
        let repo = FakeRepo { at, with_mix: true };
        let spot = FakeSpot {
            price: Some(SpotPrice::new(at, 70.0).unwrap()),
        };
        let got = GetElectricityPrice::new(repo, spot)
            .current(Region::National)
            .await
            .unwrap();
        let energie = got
            .components
            .iter()
            .find(|c| c.kind == PriceComponentKind::Energie)
            .unwrap();
        assert_eq!(energie.amount_eur_mwh, 70.0);
        assert!(got.total_eur_mwh() > 70.0, "le total inclut TURPE + taxes");
    }

    #[tokio::test]
    async fn missing_spot_is_not_found() {
        let at = OffsetDateTime::UNIX_EPOCH;
        let repo = FakeRepo { at, with_mix: true };
        let spot = FakeSpot { price: None };
        let err = GetElectricityPrice::new(repo, spot)
            .current(Region::National)
            .await;
        assert!(matches!(err, Err(ApplicationError::NotFound(_))));
    }

    #[tokio::test]
    async fn missing_mix_is_not_found() {
        let at = OffsetDateTime::UNIX_EPOCH;
        let repo = FakeRepo {
            at,
            with_mix: false,
        };
        let spot = FakeSpot {
            price: Some(SpotPrice::new(at, 70.0).unwrap()),
        };
        let err = GetElectricityPrice::new(repo, spot)
            .current(Region::National)
            .await;
        assert!(matches!(err, Err(ApplicationError::NotFound(_))));
    }

    #[tokio::test]
    async fn stale_spot_is_not_found() {
        // Dernier prix disponible vieux de 7 h (> tolérance) → pas servi comme courant.
        let at = OffsetDateTime::UNIX_EPOCH + time::Duration::days(100);
        let repo = FakeRepo { at, with_mix: true };
        let spot = FakeSpot {
            price: Some(SpotPrice::new(at - time::Duration::hours(7), 70.0).unwrap()),
        };
        let err = GetElectricityPrice::new(repo, spot)
            .current(Region::National)
            .await;
        assert!(matches!(err, Err(ApplicationError::NotFound(_))));
    }
}
