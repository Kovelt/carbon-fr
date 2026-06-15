//! Cas d'usage : **backtest** d'un modèle de prévision (ADR-0009).
//!
//! Évaluation *walk-forward* : pour une suite d'**origines** dans une fenêtre de
//! test, on prévoit l'horizon à partir de l'origine (le modèle ne voit que le
//! passé strict de l'origine — pas de fuite), puis on compare la prévision à
//! l'observé. On agrège MAE/RMSE global et par horizon, et on calcule une
//! **référence de persistance** (« demain = dernière valeur connue ») pour
//! situer le modèle. C'est ce qui permet d'annoncer une précision **mesurée**.

use std::collections::HashMap;

use time::{Duration, OffsetDateTime};

use crate::domain::{ErrorAccumulator, ErrorMetrics, Region, TimeRange};
use crate::ports::{ForecastError, ForecastModel, IntensityRepository};

use super::ApplicationError;

/// Erreurs d'un horizon donné : modèle vs référence de persistance.
#[derive(Debug, Clone, Copy)]
pub struct HorizonError {
    pub horizon: Duration,
    pub model: Option<ErrorMetrics>,
    pub persistence: Option<ErrorMetrics>,
}

/// Résultat d'un backtest.
#[derive(Debug, Clone)]
pub struct BacktestReport {
    /// Nombre d'origines effectivement évaluées (prévision non vide).
    pub origins: usize,
    /// Erreur globale du modèle (tous horizons confondus).
    pub model: Option<ErrorMetrics>,
    /// Erreur globale de la référence de persistance.
    pub persistence: Option<ErrorMetrics>,
    /// Détail par horizon demandé (h+1, h+6, h+24…).
    pub by_horizon: Vec<HorizonError>,
}

/// Backtest *walk-forward* d'un [`ForecastModel`] contre l'observé d'un
/// [`IntensityRepository`].
pub struct BacktestForecast<F: ForecastModel, R: IntensityRepository> {
    forecast: F,
    repository: R,
    methodology_id: String,
}

impl<F: ForecastModel, R: IntensityRepository> BacktestForecast<F, R> {
    pub fn new(forecast: F, repository: R, methodology_id: impl Into<String>) -> Self {
        Self {
            forecast,
            repository,
            methodology_id: methodology_id.into(),
        }
    }

    /// Évalue le modèle sur les origines `[test.start, test.end)` espacées de
    /// `origin_step`, aux horizons `checkpoints` (au pas natif `step`).
    ///
    /// L'horizon de prévision couvre le plus grand checkpoint (plus un pas, pour
    /// que ce dernier point existe). La persistance prédit, sur tout l'horizon,
    /// la dernière valeur observée **avant** l'origine.
    pub async fn execute(
        &self,
        region: Region,
        test: TimeRange,
        origin_step: Duration,
        step: Duration,
        checkpoints: &[Duration],
    ) -> Result<BacktestReport, ApplicationError> {
        let max_checkpoint = checkpoints
            .iter()
            .copied()
            .max()
            .unwrap_or_else(|| Duration::hours(24));
        // +step pour inclure le point au checkpoint maximal (horizon semi-ouvert).
        let horizon = max_checkpoint + step;

        let mut model = ErrorAccumulator::default();
        let mut persistence = ErrorAccumulator::default();
        let mut h_model = vec![ErrorAccumulator::default(); checkpoints.len()];
        let mut h_persistence = vec![ErrorAccumulator::default(); checkpoints.len()];
        let mut origins = 0usize;
        let step_secs = step.whole_seconds();

        let mut cursor = test.start();
        while cursor < test.end() {
            // L'origine est **alignée sur la grille du pas** (le quart d'heure
            // éCO2mix tombe sur :00/:15/:30/:45 UTC) : sans cet alignement, les
            // points prévus (origine + k·pas) ne coïncideraient pas avec les
            // horodatages observés et aucune paire ne serait comparée.
            let origin = align_down(cursor, step_secs);

            // Une origine sans assez d'historique (typique en début de fenêtre)
            // est **sautée**, pas fatale : seules les vraies erreurs (base,
            // prévision indisponible) interrompent le backtest.
            let predicted = match self
                .forecast
                .forecast(region, &self.methodology_id, origin, horizon)
                .await
            {
                Ok(points) => points,
                Err(ForecastError::NotEnoughData) => {
                    cursor += origin_step;
                    continue;
                }
                Err(other) => return Err(other.into()),
            };

            if !predicted.is_empty() {
                origins += 1;

                // Une seule lecture : le passé proche (pour l'ancre de
                // persistance) et l'observé de l'horizon.
                let observed = if let Some(range) =
                    TimeRange::new(origin - Duration::days(1), origin + horizon)
                {
                    self.repository
                        .range(region, &self.methodology_id, range)
                        .await?
                } else {
                    Vec::new()
                };

                let anchor = observed
                    .iter()
                    .filter(|m| m.at < origin)
                    .max_by_key(|m| m.at)
                    .map(|m| m.intensity.value());
                let actual: HashMap<OffsetDateTime, f64> = observed
                    .iter()
                    .filter(|m| m.at >= origin)
                    .map(|m| (m.at, m.intensity.value()))
                    .collect();

                for point in &predicted {
                    let Some(&truth) = actual.get(&point.at) else {
                        continue;
                    };
                    let forecast_value = point.expected.value();
                    model.observe(forecast_value, truth);
                    if let Some(anchor) = anchor {
                        persistence.observe(anchor, truth);
                    }

                    let offset = point.at - origin;
                    for (index, &checkpoint) in checkpoints.iter().enumerate() {
                        if offset == checkpoint {
                            h_model[index].observe(forecast_value, truth);
                            if let Some(anchor) = anchor {
                                h_persistence[index].observe(anchor, truth);
                            }
                        }
                    }
                }
            }

            cursor += origin_step;
        }

        let by_horizon = checkpoints
            .iter()
            .enumerate()
            .map(|(index, &horizon)| HorizonError {
                horizon,
                model: h_model[index].metrics(),
                persistence: h_persistence[index].metrics(),
            })
            .collect();

        Ok(BacktestReport {
            origins,
            model: model.metrics(),
            persistence: persistence.metrics(),
            by_horizon,
        })
    }
}

/// Aligne un instant **vers le bas** sur la grille du pas (multiples de
/// `step_secs` depuis l'époque UNIX), nanosecondes remises à zéro. Aligne les
/// origines sur la grille du quart d'heure éCO2mix.
fn align_down(at: OffsetDateTime, step_secs: i64) -> OffsetDateTime {
    if step_secs <= 0 {
        return at;
    }
    let secs = at.unix_timestamp();
    let aligned = secs - secs.rem_euclid(step_secs);
    OffsetDateTime::from_unix_timestamp(aligned).unwrap_or(at)
}
