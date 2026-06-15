//! # carbonfr-adapter-gbdt
//!
//! Modèle de prévision **ML** : arbres de gradient boosté (`gbdt`, pur Rust),
//! ADR-0012. Entraînement *et* inférence en Rust → projet rebuildable au
//! `cargo`, serveur mono-binaire.
//!
//! - [`build_training_examples`] + [`train_model`] : entraînement offline
//!   (`bin/train`) → artefact versionné ([`GbdtModel::save`]).
//! - [`GbdtForecaster`] : inférence derrière le port `ForecastModel`, chargeant
//!   l'artefact ([`GbdtModel::load`]).
//!
//! La *feature engineering* ([`features`]) est **partagée** entre les deux —
//! identité train/inférence garantie. Le `core` reste pur (la météo et les
//! features n'y entrent pas, ADR-0002/0012).

pub mod features;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use carbonfr_core::domain::{
    CarbonIntensity, ForecastPoint, HorizonBands, ModelVersion, Region, TimeRange,
};
use carbonfr_core::ports::{ForecastError, ForecastModel, IntensityRepository, WeatherRepository};
use gbdt::config::Config;
use gbdt::decision_tree::{Data, DataVec};
use gbdt::gradient_boost::GBDT;
use time::{Duration, OffsetDateTime};

use features::{FEATURE_SIZE, build_features, slot_climatology, week_slot};

/// Identité versionnée du modèle ML (ADR-0012), exposée par l'API.
pub const GBDT_ID: &str = "gbdt";
pub const GBDT_VERSION: u32 = 1;

/// Hyper-paramètres d'entraînement (défauts raisonnables pour ce volume).
#[derive(Debug, Clone, Copy)]
pub struct GbdtHyperParams {
    pub max_depth: u32,
    pub iterations: usize,
    pub shrinkage: f32,
}

impl Default for GbdtHyperParams {
    fn default() -> Self {
        Self {
            max_depth: 5,
            iterations: 120,
            shrinkage: 0.1,
        }
    }
}

/// Artefact GBDT entraîné (sérialisable en format natif `gbdt`).
#[derive(Clone)]
pub struct GbdtModel {
    inner: Arc<GBDT>,
}

impl GbdtModel {
    /// Charge un artefact depuis un chemin.
    pub fn load(path: &str) -> Result<Self, String> {
        GBDT::load_model(path)
            .map(|gbdt| Self {
                inner: Arc::new(gbdt),
            })
            .map_err(|e| format!("chargement du modèle GBDT « {path} » : {e}"))
    }

    /// Sauvegarde l'artefact vers un chemin.
    pub fn save(&self, path: &str) -> Result<(), String> {
        self.inner
            .save_model(path)
            .map_err(|e| format!("sauvegarde du modèle GBDT « {path} » : {e}"))
    }

    /// Prédiction pour un vecteur de features.
    fn predict_one(&self, feature: &[f32]) -> f64 {
        let dv: DataVec = vec![Data::new_test_data(feature.to_vec(), None)];
        self.inner.predict(&dv).first().copied().unwrap_or(0.0) as f64
    }
}

/// Construit les exemples `(features, label)` d'entraînement par *walk-forward* :
/// pour chaque origine, des cibles à divers horizons (pas `step`, jusqu'à
/// `max_horizon`). `intensity` indexe les intensités observées par horodatage ;
/// label = intensité **observée** à la cible.
pub fn build_training_examples(
    intensity: &HashMap<OffsetDateTime, f64>,
    weather: &HashMap<OffsetDateTime, (f64, f64)>,
    weeks: i64,
    origins: &[OffsetDateTime],
    step: Duration,
    max_horizon: Duration,
) -> Vec<(Vec<f32>, f32)> {
    let step_secs = step.whole_seconds();
    let history = Duration::days(weeks.max(1) * 7);
    let mut examples = Vec::new();
    for &origin in origins {
        // Ancre = dernière observation **avant** l'origine (grille dense) — même
        // convention qu'à l'inférence (l'origine n'est pas supposée publiée).
        let anchor_at = origin - step;
        // Climatologie de créneau sur la **fenêtre glissante** de l'origine —
        // **identique** à l'inférence (sinon la feature ne se transfère pas).
        let lo = origin - history;
        let trailing: HashMap<OffsetDateTime, f64> = intensity
            .iter()
            .filter_map(|(at, v)| {
                let at = *at;
                (at >= lo && at < origin).then_some((at, *v))
            })
            .collect();
        let slot_climo = slot_climatology(&trailing, step_secs);
        let mut target = origin + step;
        while target <= origin + max_horizon {
            let climo = slot_climo
                .get(&week_slot(target, step_secs))
                .copied()
                .unwrap_or(0.0);
            if let (Some(features), Some(&label)) = (
                build_features(
                    origin,
                    target,
                    anchor_at,
                    climo,
                    weather.get(&target).copied(),
                    intensity,
                ),
                intensity.get(&target),
            ) {
                examples.push((features, label as f32));
            }
            target += step;
        }
    }
    examples
}

/// Entraîne un GBDT sur des exemples `(features, label)`. `None` si aucun exemple.
pub fn train_model(examples: &[(Vec<f32>, f32)], params: GbdtHyperParams) -> Option<GbdtModel> {
    if examples.is_empty() {
        return None;
    }
    let mut cfg = Config::new();
    cfg.set_feature_size(FEATURE_SIZE);
    cfg.set_max_depth(params.max_depth);
    cfg.set_iterations(params.iterations);
    cfg.set_shrinkage(params.shrinkage);
    cfg.set_loss("SquaredError");
    cfg.set_data_sample_ratio(1.0);
    cfg.set_feature_sample_ratio(1.0);
    cfg.set_debug(false);

    let mut data: DataVec = examples
        .iter()
        .map(|(feat, label)| Data::new_training_data(feat.clone(), 1.0, *label, None))
        .collect();

    let mut gbdt = GBDT::new(&cfg);
    gbdt.fit(&mut data);
    Some(GbdtModel {
        inner: Arc::new(gbdt),
    })
}

/// Modèle de prévision ML (`gbdt@1`) branché sur un repository d'intensité.
///
/// Charge l'artefact entraîné offline, lit l'historique récent (pour les lags)
/// via [`IntensityRepository`], construit les features et prédit. Intervalles via
/// des bandes par horizon (ADR-0011 §5), si fournies.
#[derive(Clone)]
pub struct GbdtForecaster<R, W> {
    repo: R,
    weather: W,
    model: GbdtModel,
    weeks: i64,
    step: Duration,
    bands: Option<HorizonBands>,
}

impl<R, W> GbdtForecaster<R, W> {
    /// Construit avec 10 semaines d'historique (lags) et un pas de 15 min.
    pub fn new(repo: R, weather: W, model: GbdtModel) -> Self {
        Self {
            repo,
            weather,
            model,
            weeks: 10,
            step: Duration::minutes(15),
            bands: None,
        }
    }

    /// Surcharge la profondeur d'historique et le pas.
    pub fn with_config(repo: R, weather: W, model: GbdtModel, weeks: u32, step: Duration) -> Self {
        Self {
            repo,
            weather,
            model,
            weeks: (weeks.max(1)) as i64,
            step,
            bands: None,
        }
    }

    /// Injecte les bandes d'incertitude par horizon (ADR-0011 §5).
    pub fn with_bands(mut self, bands: HorizonBands) -> Self {
        self.bands = Some(bands);
        self
    }
}

#[async_trait]
impl<R: IntensityRepository, W: WeatherRepository> ForecastModel for GbdtForecaster<R, W> {
    async fn forecast(
        &self,
        region: Region,
        methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError> {
        let history_start = from - Duration::days(self.weeks * 7);
        let window = TimeRange::new(history_start, from)
            .ok_or_else(|| ForecastError::Unavailable("fenêtre d'historique invalide".into()))?;
        let history = self
            .repo
            .range(region, methodology_id, window)
            .await
            .map_err(|e| ForecastError::Unavailable(e.to_string()))?;

        // Ancre = observation la plus récente disponible (avant `from`).
        let anchor = match history.iter().max_by_key(|m| m.at) {
            Some(m) => m,
            None => return Err(ForecastError::NotEnoughData),
        };
        let anchor_at = anchor.at;
        let methodology = anchor.methodology.clone();
        let intensity: HashMap<OffsetDateTime, f64> = history
            .iter()
            .map(|m| (m.at, m.intensity.value()))
            .collect();
        let step_secs = self.step.whole_seconds();
        let slot_climo = slot_climatology(&intensity, step_secs);
        let model = ModelVersion::new(GBDT_ID, GBDT_VERSION);

        // Météo **telle que disponible à `from`** : pour chaque échéance, le run
        // le plus récent dont `run_at ≤ from` (anti-fuite).
        let weather_window = TimeRange::new(from, from + horizon)
            .ok_or_else(|| ForecastError::Unavailable("fenêtre météo invalide".into()))?;
        let weather_rows = self
            .weather
            .weather_range(weather_window)
            .await
            .map_err(|e| ForecastError::Unavailable(e.to_string()))?;
        let mut weather: HashMap<OffsetDateTime, (OffsetDateTime, f64, f64)> = HashMap::new();
        for w in weather_rows {
            if w.run_at > from {
                continue;
            }
            match weather.get(&w.valid_at) {
                Some((run, _, _)) if *run >= w.run_at => {}
                _ => {
                    weather.insert(w.valid_at, (w.run_at, w.wind, w.irradiance));
                }
            }
        }

        let mut points = Vec::new();
        let mut target = from;
        while target < from + horizon {
            let climo = slot_climo
                .get(&week_slot(target, step_secs))
                .copied()
                .unwrap_or(0.0);
            let weather_target = weather.get(&target).map(|&(_, wind, irr)| (wind, irr));
            if let Some(feature) =
                build_features(from, target, anchor_at, climo, weather_target, &intensity)
            {
                let expected = self.model.predict_one(&feature).max(0.0);
                let (low, high) = match self.bands.as_ref().and_then(|b| b.at(target - from)) {
                    Some((q_low, q_high)) => (
                        (expected + q_low).max(0.0),
                        (expected + q_high).max(expected),
                    ),
                    None => (expected, expected),
                };
                if let (Some(e), Some(l), Some(u)) = (
                    CarbonIntensity::new(expected),
                    CarbonIntensity::new(low),
                    CarbonIntensity::new(high),
                ) {
                    points.push(ForecastPoint::new(
                        target,
                        region,
                        e,
                        l,
                        u,
                        methodology.clone(),
                        model.clone(),
                    ));
                }
            }
            target += self.step;
        }

        if points.is_empty() {
            return Err(ForecastError::NotEnoughData);
        }
        Ok(points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Intensité périodique journalière → l'analogue d'il y a une semaine
    /// (feature `lag_week`) égale la cible : le GBDT doit l'apprendre.
    #[test]
    fn learns_periodic_signal_and_roundtrips() {
        let step = Duration::hours(1);
        let signal = |t: OffsetDateTime| 20.0 + (t.hour() as f64) * 1.5;
        let start = OffsetDateTime::UNIX_EPOCH + Duration::days(60);

        let mut intensity = HashMap::new();
        for k in 0..(4 * 7 * 24) {
            let at = start + step * k;
            intensity.insert(at, signal(at));
        }
        // Origines des semaines 3-4 (lags présents), toutes les 6 h.
        let origins: Vec<OffsetDateTime> = (2 * 7 * 24..4 * 7 * 24)
            .step_by(6)
            .map(|k| start + step * k)
            .collect();

        let slot_climo = slot_climatology(&intensity, step.whole_seconds());
        let weather = HashMap::new(); // pas de météo dans ce test unitaire
        let examples = build_training_examples(
            &intensity,
            &weather,
            2, // 2 semaines de fenêtre glissante
            &origins,
            step,
            Duration::hours(6),
        );
        assert!(!examples.is_empty());
        let model = train_model(&examples, GbdtHyperParams::default()).unwrap();

        let origin = start + step * (3 * 7 * 24);
        let target = origin + Duration::hours(3);
        let climo = slot_climo[&week_slot(target, step.whole_seconds())];
        let feature = build_features(origin, target, origin, climo, None, &intensity).unwrap();
        let pred = model.predict_one(&feature);
        let truth = signal(target);
        assert!((pred - truth).abs() < 5.0, "pred {pred} vs truth {truth}");

        // Roundtrip save/load → mêmes prédictions.
        let path = std::env::temp_dir().join("carbonfr_gbdt_roundtrip.model");
        let path = path.to_str().unwrap();
        model.save(path).unwrap();
        let loaded = GbdtModel::load(path).unwrap();
        assert!((loaded.predict_one(&feature) - pred).abs() < 1e-6);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn train_model_none_on_empty() {
        assert!(train_model(&[], GbdtHyperParams::default()).is_none());
    }
}
