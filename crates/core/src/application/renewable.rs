//! Cas d'usage : **calibration** du modèle de dérivation renouvelable (ADR-0018)
//! sur l'historique récent, pour servir `/v1/renewable`.
//!
//! Lit la production réelle (mix national) + la météo, joint par heure, et cale
//! les capacités effectives sur **tous** les échantillons (pas de split : ici on
//! ne mesure pas l'erreur — `BacktestRenewable` le fait —, on veut le meilleur
//! modèle de service). Lancé au démarrage du serveur (le parc installé évolue →
//! on recale périodiquement plutôt que de figer une constante).

use std::collections::BTreeMap;

use crate::domain::{Region, RenewableModel, RenewableSample, TimeRange, calibrate_renewable};
use crate::ports::{IntensityRepository, WeatherRepository};

use super::ApplicationError;

/// Calibre le [`RenewableModel`] sur la production réelle récente.
pub struct CalibrateRenewable<R: IntensityRepository, W: WeatherRepository> {
    repository: R,
    weather: W,
}

impl<R: IntensityRepository, W: WeatherRepository> CalibrateRenewable<R, W> {
    pub fn new(repository: R, weather: W) -> Self {
        Self {
            repository,
            weather,
        }
    }

    /// Calibre sur `range`. [`ApplicationError::InsufficientSeries`] si la jointure
    /// météo×production est trop maigre pour caler (assise dégénérée).
    pub async fn execute(&self, range: TimeRange) -> Result<RenewableModel, ApplicationError> {
        let measurements = self
            .repository
            .range(Region::National, "rte-direct", range)
            .await?;
        let weather = self.weather.weather_range(range).await?;

        // Météo indexée par heure (dernier run gagne — tri croissant par run_at).
        let mut by_hour: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
        for w in &weather {
            by_hour.insert(
                w.valid_at.unix_timestamp().div_euclid(3600),
                (w.wind, w.irradiance),
            );
        }

        let mut samples: Vec<RenewableSample> = Vec::new();
        for m in &measurements {
            let Some(mix) = m.mix.as_ref() else { continue };
            if let Some(&(wind_kmh, irradiance_wm2)) =
                by_hour.get(&m.at.unix_timestamp().div_euclid(3600))
            {
                samples.push(RenewableSample {
                    wind_kmh,
                    irradiance_wm2,
                    eolien_mw: mix.eolien,
                    solaire_mw: mix.solaire,
                });
            }
        }

        calibrate_renewable(&samples, RenewableModel::v1_uncalibrated())
            .ok_or(ApplicationError::InsufficientSeries)
    }
}
