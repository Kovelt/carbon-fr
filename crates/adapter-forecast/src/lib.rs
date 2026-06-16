//! # carbonfr-adapter-forecast
//!
//! Adapter **sortant** : implémentation de [`ForecastModel`] par **climatologie**
//! (`climatology@1`, ADR-0009).
//!
//! Cet adapter ne porte **aucune logique métier** : il se contente de l'IO de
//! lecture — récupérer les `N` dernières semaines de la série `(region,
//! methodology_id)` via [`IntensityRepository`] — puis de **déléguer au calcul
//! pur** du domaine ([`climatology_forecast`]). La formule, elle, vit dans
//! `core` (testable sans IO).

use async_trait::async_trait;
use carbonfr_core::domain::{
    ClimatologyParams, EmissionFactors, ForecastPoint, HorizonBands, Region, TD_LOSS_FACTOR_V1,
    TimeRange, acv_ademe_forecast, climatology_forecast,
};
use carbonfr_core::ports::{
    CrossBorderRepository, ForecastError, ForecastModel, IntensityRepository,
};
use time::{Duration, OffsetDateTime};

/// Profondeur d'historique par défaut alimentant la climatologie.
/// **10 semaines glissantes** — valeur calée par backtest (addendum ADR-0009).
const DEFAULT_WEEKS: i64 = 10;

/// Modèle de prévision `climatology@1` (ADR-0009) branché sur un repository.
///
/// Générique sur `R: IntensityRepository` → dispatch statique, zéro coût (comme
/// les cas d'usage du `core`). La *composition root* y câble le repository
/// Postgres concret. `Clone` quand `R` l'est (le pool Postgres l'est, à coût
/// négligeable) — requis pour le partage dans l'état de l'API.
#[derive(Clone)]
pub struct ClimatologyForecaster<R> {
    repo: R,
    weeks: i64,
    params: ClimatologyParams,
    /// Bandes d'incertitude par horizon (ADR-0011 §5), calibrées par backtest.
    /// `None` → intervalle de repli par dispersion de créneau.
    bands: Option<HorizonBands>,
}

impl<R> ClimatologyForecaster<R> {
    /// Construit avec les défauts calés (addendum ADR-0009) : 10 semaines
    /// d'historique ; pas 15 min ; τ = 2 semaines. Intervalles non calibrés
    /// (repli par créneau) tant que [`with_bands`](Self::with_bands) n'est pas
    /// appelé.
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            weeks: DEFAULT_WEEKS,
            params: ClimatologyParams::default(),
            bands: None,
        }
    }

    /// Surcharge la profondeur d'historique (en semaines, au moins 1) et les
    /// paramètres du modèle.
    pub fn with_config(repo: R, weeks: u32, params: ClimatologyParams) -> Self {
        Self {
            repo,
            weeks: weeks.max(1) as i64,
            params,
            bands: None,
        }
    }

    /// Injecte les bandes d'incertitude par horizon (calibrées par backtest,
    /// ADR-0011) : les intervalles s'élargiront alors avec l'horizon.
    pub fn with_bands(mut self, bands: HorizonBands) -> Self {
        self.bands = Some(bands);
        self
    }
}

#[async_trait]
impl<R: IntensityRepository> ForecastModel for ClimatologyForecaster<R> {
    async fn forecast(
        &self,
        region: Region,
        methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError> {
        // Fenêtre d'historique : [from − N semaines, from). Semi-ouverte → exclut
        // `from` et le futur : on ne nourrit la climatologie que d'observations
        // passées (la plus récente sert d'ancre de persistance).
        let history_start = from - Duration::days(self.weeks * 7);
        let window = TimeRange::new(history_start, from)
            .ok_or_else(|| ForecastError::Unavailable("fenêtre d'historique invalide".into()))?;

        let history = self
            .repo
            .range(region, methodology_id, window)
            .await
            .map_err(|e| ForecastError::Unavailable(e.to_string()))?;

        // None (historique vide / paramètres invalides) ou série vide → on ne
        // peut pas prévoir.
        climatology_forecast(&history, from, horizon, self.params, self.bands.as_ref())
            .filter(|points| !points.is_empty())
            .ok_or(ForecastError::NotEnoughData)
    }
}

/// Modèle de prévision **`acv-ademe@2`** (consumption-based, ADR-0013) :
/// climatologie des **entrées** (mix + contexte d'import) puis application du
/// calculateur pur `AcvAdeme`.
///
/// Cet adapter ne porte aucune logique métier : il lit l'historique du mix
/// (`acv-ademe@1`, via [`IntensityRepository`]) et du contexte d'import (via
/// [`CrossBorderRepository`]), puis **délègue au calcul pur** du domaine
/// ([`acv_ademe_forecast`]). **National** uniquement (ADR-0013 §8).
#[derive(Clone)]
pub struct AcvAdemeForecaster<R, C> {
    repo: R,
    cross_border: C,
    weeks: i64,
    params: ClimatologyParams,
    /// Bandes d'incertitude par horizon (ADR-0011 §5), calibrées par backtest
    /// `acv-ademe` ; `None` → repli sur la dispersion par créneau.
    bands: Option<HorizonBands>,
}

impl<R, C> AcvAdemeForecaster<R, C> {
    /// Construit avec les défauts calés (10 semaines d'historique, ADR-0009).
    pub fn new(repo: R, cross_border: C) -> Self {
        Self {
            repo,
            cross_border,
            weeks: DEFAULT_WEEKS,
            params: ClimatologyParams::default(),
            bands: None,
        }
    }

    /// Injecte les bandes d'incertitude par horizon (ADR-0013 §6) : les
    /// intervalles `@2` s'élargiront alors avec l'horizon.
    pub fn with_bands(mut self, bands: HorizonBands) -> Self {
        self.bands = Some(bands);
        self
    }
}

#[async_trait]
impl<R, C> ForecastModel for AcvAdemeForecaster<R, C>
where
    R: IntensityRepository,
    C: CrossBorderRepository,
{
    async fn forecast(
        &self,
        region: Region,
        _methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError> {
        if region != Region::National {
            return Err(ForecastError::Unavailable(
                "acv-ademe@2 (consommation) n'est prévu qu'au national".into(),
            ));
        }
        let history_start = from - Duration::days(self.weeks * 7);
        let window = TimeRange::new(history_start, from)
            .ok_or_else(|| ForecastError::Unavailable("fenêtre d'historique invalide".into()))?;

        // Mix FR : porté par les mesures `acv-ademe@1` ; contexte d'import : store
        // ENTSO-E. Les deux **tels que disponibles** (anti-fuite, ADR-0013 §7).
        let mix_history = self
            .repo
            .range(region, "acv-ademe", window)
            .await
            .map_err(|e| ForecastError::Unavailable(e.to_string()))?;
        let flow_history = self
            .cross_border
            .flows_range(window)
            .await
            .map_err(|e| ForecastError::Unavailable(e.to_string()))?;

        acv_ademe_forecast(
            &mix_history,
            &flow_history,
            from,
            horizon,
            self.params,
            &EmissionFactors::acv_ademe_v1(),
            TD_LOSS_FACTOR_V1,
            self.bands.as_ref(),
        )
        .filter(|points| !points.is_empty())
        .ok_or(ForecastError::NotEnoughData)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbonfr_core::domain::{
        CarbonIntensity, Granularity, HorizonBands, IntensityStats, Measurement, Methodology,
        RollupBucket, Vintage,
    };
    use carbonfr_core::ports::RepositoryError;

    /// Repository en mémoire : seul `range` est significatif (filtre par région,
    /// méthodologie et fenêtre), le reste satisfait le trait sans IO.
    struct FakeRepo {
        points: Vec<Measurement>,
    }

    #[async_trait]
    impl IntensityRepository for FakeRepo {
        async fn upsert_many(&self, _m: &[Measurement]) -> Result<usize, RepositoryError> {
            Ok(0)
        }

        async fn latest(
            &self,
            _region: Region,
            _methodology_id: &str,
        ) -> Result<Option<Measurement>, RepositoryError> {
            Ok(None)
        }

        async fn range(
            &self,
            region: Region,
            methodology_id: &str,
            range: TimeRange,
        ) -> Result<Vec<Measurement>, RepositoryError> {
            let mut got: Vec<Measurement> = self
                .points
                .iter()
                .filter(|m| {
                    m.region == region && m.methodology.id == methodology_id && range.contains(m.at)
                })
                .cloned()
                .collect();
            got.sort_by_key(|m| m.at);
            Ok(got)
        }

        async fn stats(
            &self,
            _region: Region,
            _methodology_id: &str,
            _range: TimeRange,
        ) -> Result<Option<IntensityStats>, RepositoryError> {
            Ok(None)
        }

        async fn rollup(
            &self,
            _region: Region,
            _methodology_id: &str,
            _range: TimeRange,
            _granularity: Granularity,
        ) -> Result<Vec<RollupBucket>, RepositoryError> {
            Ok(vec![])
        }

        async fn refresh_rollups(&self) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    fn point(at: OffsetDateTime, region: Region, methodology: &str, g: f64) -> Measurement {
        Measurement {
            at,
            region,
            intensity: CarbonIntensity::new(g).unwrap(),
            methodology: Methodology::new(methodology, 1),
            vintage: Vintage::Tr,
            mix: None,
        }
    }

    /// Motif horaire (creux la nuit, pointe l'après-midi) — pour vérifier que la
    /// climatologie traverse bien l'adapter.
    fn hourly_pattern(t: OffsetDateTime) -> f64 {
        match t.hour() {
            0..=5 => 20.0,
            12..=17 => 80.0,
            _ => 50.0,
        }
    }

    fn seed_pattern(end: OffsetDateTime, step: Duration, count: usize) -> Vec<Measurement> {
        (0..count)
            .map(|i| {
                let at = end - step * ((count - i) as i32);
                point(at, Region::National, "rte-direct", hourly_pattern(at))
            })
            .collect()
    }

    #[tokio::test]
    async fn forecasts_from_repository_history() {
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let step = Duration::hours(1);
        let repo = FakeRepo {
            points: seed_pattern(from, step, 14 * 24),
        };
        let forecaster = ClimatologyForecaster::with_config(
            repo,
            8,
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        );

        let out = forecaster
            .forecast(Region::National, "rte-direct", from, Duration::hours(24))
            .await
            .unwrap();

        assert_eq!(out.len(), 24);
        let night = out
            .iter()
            .find(|m| m.at.hour() == 3)
            .unwrap()
            .expected
            .value();
        let day = out
            .iter()
            .find(|m| m.at.hour() == 14)
            .unwrap()
            .expected
            .value();
        assert!(night < day, "nuit {night} doit être < jour {day}");
    }

    #[tokio::test]
    async fn empty_history_is_not_enough_data() {
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let repo = FakeRepo { points: vec![] };
        let forecaster = ClimatologyForecaster::new(repo);
        let err = forecaster
            .forecast(Region::National, "rte-direct", from, Duration::hours(24))
            .await
            .unwrap_err();
        assert!(matches!(err, ForecastError::NotEnoughData));
    }

    #[tokio::test]
    async fn filters_by_region_and_methodology() {
        // L'historique n'existe que pour (National, rte-direct) : prévoir une
        // autre méthodologie ne trouve rien → NotEnoughData.
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let step = Duration::hours(1);
        let repo = FakeRepo {
            points: seed_pattern(from, step, 14 * 24),
        };
        let forecaster = ClimatologyForecaster::new(repo);
        let err = forecaster
            .forecast(Region::National, "acv-ademe", from, Duration::hours(24))
            .await
            .unwrap_err();
        assert!(matches!(err, ForecastError::NotEnoughData));
    }

    #[tokio::test]
    async fn history_window_excludes_data_older_than_n_weeks() {
        // Fenêtre = 1 semaine. Une observation extrême vieille de 10 jours est
        // hors fenêtre : elle ne doit pas polluer la prévision.
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let step = Duration::hours(1);
        let mut points: Vec<Measurement> = (0..7 * 24)
            .map(|i| point(from - step * (i + 1), Region::National, "rte-direct", 50.0))
            .collect();
        points.push(point(
            from - Duration::days(10),
            Region::National,
            "rte-direct",
            9999.0,
        ));

        let forecaster = ClimatologyForecaster::with_config(
            FakeRepo { points },
            1,
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        );
        let out = forecaster
            .forecast(Region::National, "rte-direct", from, Duration::hours(6))
            .await
            .unwrap();
        assert!(
            out.iter().all(|m| m.expected.value() < 100.0),
            "la valeur hors fenêtre (9999) ne doit pas influencer la prévision"
        );
    }

    #[tokio::test]
    async fn injected_bands_drive_the_interval() {
        // Historique plat à 40 → sans bandes, l'intervalle est dégénéré (40,40).
        // Avec des bandes calibrées (résidus −10..+20), il s'ouvre autour de 40.
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let step = Duration::minutes(15);
        let history: Vec<Measurement> = (1..=8 * 7 * 96)
            .map(|i: i32| point(from - step * i, Region::National, "rte-direct", 40.0))
            .collect();

        let residuals: Vec<f64> = (-10..=20).map(|x| x as f64).collect();
        let bands = HorizonBands::from_residuals(
            step,
            &[
                residuals.clone(),
                residuals.clone(),
                residuals.clone(),
                residuals,
            ],
            0.1,
        );

        let forecaster = ClimatologyForecaster::with_config(
            FakeRepo { points: history },
            8,
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        )
        .with_bands(bands);

        let out = forecaster
            .forecast(Region::National, "rte-direct", from, Duration::hours(1))
            .await
            .unwrap();

        let p = &out[0];
        assert!((p.expected.value() - 40.0).abs() < 1.0);
        // L'intervalle vient des bandes (résidus signés), pas dégénéré.
        assert!(
            p.lower.value() < p.expected.value(),
            "lower = {}",
            p.lower.value()
        );
        assert!(
            p.upper.value() > p.expected.value(),
            "upper = {}",
            p.upper.value()
        );
    }
}
