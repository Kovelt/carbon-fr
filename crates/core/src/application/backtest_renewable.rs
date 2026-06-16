//! Backtest de la **dérivation renouvelable** (ADR-0018) : mesurer, sur la donnée
//! réelle, à quel point la météo (vent/irradiance) explique la production
//! éolien/solaire — l'évidence du moat, jamais supposée.
//!
//! Protocole **anti-surapprentissage** : on **calibre** les capacités effectives
//! sur les 70 % les plus anciens de la fenêtre, puis on **mesure l'erreur** sur
//! les 30 % les plus récents (jamais vus à la calibration). On compare à un
//! **baseline naïf** (production moyenne de la période de calibration) : si la
//! météo n'ajoute rien, le baseline gagne et le modèle n'est pas publié.

use crate::domain::{
    ErrorAccumulator, ErrorMetrics, Region, RenewableModel, RenewableSample, TimeRange,
    calibrate_renewable,
};
use crate::ports::{IntensityRepository, WeatherRepository};

use super::ApplicationError;

/// Bilan d'un backtest renouvelable : erreur (MW) du modèle vs baseline, pour
/// l'éolien et le solaire, sur l'échantillon de test.
#[derive(Debug, Clone, Copy)]
pub struct RenewableReport {
    pub wind: ErrorMetrics,
    pub wind_baseline: ErrorMetrics,
    pub solar: ErrorMetrics,
    pub solar_baseline: ErrorMetrics,
    /// Modèle calibré (capacités effectives retrouvées).
    pub model: RenewableModel,
    /// Échantillons de calibration / de test.
    pub train: usize,
    pub test: usize,
}

/// Backtest de la dérivation renouvelable : production réelle (mix national) vs
/// estimation depuis la météo.
pub struct BacktestRenewable<R: IntensityRepository, W: WeatherRepository> {
    repository: R,
    weather: W,
}

impl<R: IntensityRepository, W: WeatherRepository> BacktestRenewable<R, W> {
    pub fn new(repository: R, weather: W) -> Self {
        Self {
            repository,
            weather,
        }
    }

    /// Exécute le backtest sur `range` (national). [`ApplicationError::InsufficientSeries`]
    /// si la jointure météo×production ne produit pas assez d'échantillons.
    pub async fn execute(&self, range: TimeRange) -> Result<RenewableReport, ApplicationError> {
        // Production réelle : mix national `rte-direct` (éolien/solaire en MW).
        let measurements = self
            .repository
            .range(Region::National, "rte-direct", range)
            .await?;
        // Météo observée sur la même fenêtre.
        let weather = self.weather.weather_range(range).await?;

        // Index météo par **heure** (clé = heure Unix) : la dernière prévision
        // écrite pour une heure gagne (weather_range trié par run_at croissant).
        let mut by_hour: std::collections::BTreeMap<i64, (f64, f64)> =
            std::collections::BTreeMap::new();
        for w in &weather {
            let hour = w.valid_at.unix_timestamp().div_euclid(3600);
            by_hour.insert(hour, (w.wind, w.irradiance));
        }

        // Jointure : pour chaque mesure (avec mix), la météo de la même heure.
        let mut samples: Vec<RenewableSample> = Vec::new();
        for m in &measurements {
            let Some(mix) = m.mix.as_ref() else { continue };
            let hour = m.at.unix_timestamp().div_euclid(3600);
            if let Some(&(wind_kmh, irradiance_wm2)) = by_hour.get(&hour) {
                samples.push(RenewableSample {
                    wind_kmh,
                    irradiance_wm2,
                    eolien_mw: mix.eolien,
                    solaire_mw: mix.solaire,
                });
            }
        }

        // Découpe temporelle 70/30 (les mesures sont triées par `at` croissant).
        if samples.len() < 20 {
            return Err(ApplicationError::InsufficientSeries);
        }
        let split = samples.len() * 7 / 10;
        let (train, test) = samples.split_at(split);

        // Calibration sur le train.
        let model = calibrate_renewable(train, RenewableModel::v1_uncalibrated())
            .ok_or(ApplicationError::InsufficientSeries)?;

        // Baselines : production moyenne du train (prédicteur constant).
        let wind_mean = mean(train.iter().map(|s| s.eolien_mw));
        let solar_mean = mean(train.iter().map(|s| s.solaire_mw));

        // Évaluation sur le test (jamais vu à la calibration).
        let (mut wind, mut wind_base, mut solar, mut solar_base) = (
            ErrorAccumulator::default(),
            ErrorAccumulator::default(),
            ErrorAccumulator::default(),
            ErrorAccumulator::default(),
        );
        for s in test {
            wind.observe(model.estimate_wind_mw(s.wind_kmh), s.eolien_mw);
            wind_base.observe(wind_mean, s.eolien_mw);
            solar.observe(model.estimate_solar_mw(s.irradiance_wm2), s.solaire_mw);
            solar_base.observe(solar_mean, s.solaire_mw);
        }

        Ok(RenewableReport {
            wind: wind.metrics().ok_or(ApplicationError::InsufficientSeries)?,
            wind_baseline: wind_base
                .metrics()
                .ok_or(ApplicationError::InsufficientSeries)?,
            solar: solar
                .metrics()
                .ok_or(ApplicationError::InsufficientSeries)?,
            solar_baseline: solar_base
                .metrics()
                .ok_or(ApplicationError::InsufficientSeries)?,
            model,
            train: train.len(),
            test: test.len(),
        })
    }
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, n) = values.fold((0.0, 0u64), |(s, n), v| (s + v, n + 1));
    if n == 0 { 0.0 } else { sum / n as f64 }
}
