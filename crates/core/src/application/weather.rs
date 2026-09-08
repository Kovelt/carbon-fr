//! Cas d'usage : exposition de la **météo** ingérée (vent à 100 m, irradiance ;
//! moyenne nationale), ADR-0012 / ADR-0018.
//!
//! La donnée est déjà ingérée par le poller (port `WeatherForecastSource` →
//! store) pour le modèle ML et la dérivation renouvelable ; on la **sert** telle
//! quelle. Source Open-Meteo (CC-BY 4.0) — l'attribution vit dans l'adapter HTTP.
//!
//! Le store contient des prévisions `(run_at, valid_at)` : pour un instant donné
//! on retient la **dernière prévision** (run le plus récent) du créneau le plus
//! proche ; pour une série, une valeur par `valid_at` (run le plus récent).

use time::{Duration, OffsetDateTime};

use crate::domain::{Region, TimeRange, WeatherForecast};
use crate::ports::WeatherRepository;

use super::ApplicationError;

/// Sert la météo nationale (vent/irradiance) depuis le store de prévisions.
pub struct GetWeather<W: WeatherRepository> {
    weather: W,
}

impl<W: WeatherRepository> GetWeather<W> {
    pub fn new(weather: W) -> Self {
        Self { weather }
    }

    /// Météo la plus proche de `reference` (dernier run connu du créneau le plus
    /// proche). [`ApplicationError::NotFound`] si le store est vide autour.
    pub async fn latest(
        &self,
        reference: OffsetDateTime,
    ) -> Result<WeatherForecast, ApplicationError> {
        let window = TimeRange::new(
            reference - Duration::hours(3),
            reference + Duration::hours(3),
        )
        .ok_or(ApplicationError::NotFound(Region::National))?;
        // `weather_latest` rend déjà une valeur par échéance (dernier run, dédup
        // côté base — audit F19) ; on ne garde que la plus proche de `reference`.
        self.weather
            .weather_latest(window)
            .await?
            .into_iter()
            .min_by_key(|f| (f.valid_at - reference).whole_seconds().abs())
            .ok_or(ApplicationError::NotFound(Region::National))
    }

    /// Série météo sur un intervalle de `valid_at`, une valeur par créneau
    /// (dernier run connu), triée par `valid_at` croissant.
    pub async fn series(&self, valid: TimeRange) -> Result<Vec<WeatherForecast>, ApplicationError> {
        Ok(self.weather.weather_latest(valid).await?)
    }
}
