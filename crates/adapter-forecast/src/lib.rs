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
    ClimatologyParams, Measurement, Region, TimeRange, climatology_forecast,
};
use carbonfr_core::ports::{ForecastError, ForecastModel, IntensityRepository};
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
}

impl<R> ClimatologyForecaster<R> {
    /// Construit avec les défauts calés (addendum ADR-0009) : 10 semaines
    /// d'historique ; pas 15 min ; τ = 2 semaines.
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            weeks: DEFAULT_WEEKS,
            params: ClimatologyParams::default(),
        }
    }

    /// Surcharge la profondeur d'historique (en semaines, au moins 1) et les
    /// paramètres du modèle.
    pub fn with_config(repo: R, weeks: u32, params: ClimatologyParams) -> Self {
        Self {
            repo,
            weeks: weeks.max(1) as i64,
            params,
        }
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
    ) -> Result<Vec<Measurement>, ForecastError> {
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
        climatology_forecast(&history, from, horizon, self.params)
            .filter(|points| !points.is_empty())
            .ok_or(ForecastError::NotEnoughData)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbonfr_core::domain::{
        CarbonIntensity, Granularity, IntensityStats, Methodology, RollupBucket, Vintage,
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
            .intensity
            .value();
        let day = out
            .iter()
            .find(|m| m.at.hour() == 14)
            .unwrap()
            .intensity
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
            out.iter().all(|m| m.intensity.value() < 100.0),
            "la valeur hors fenêtre (9999) ne doit pas influencer la prévision"
        );
    }
}
