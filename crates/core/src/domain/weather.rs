//! Prévision météo nationale (ADR-0012) — entrée du futur modèle ML.
//!
//! La météo **n'est pas un concept carbone** : elle ne crée aucun type dans le
//! cœur métier au-delà de cet enregistrement d'entrée (frontière hexagonale,
//! ADR-0002/0012). Le point clé est l'**anti-fuite** : une prévision est datée
//! par `(run_at, valid_at)` — *produite à* `run_at`, *valable pour* `valid_at`.
//! À l'entraînement comme à l'inférence, on n'utilise que la prévision **telle
//! qu'elle était disponible** à l'instant de la prédiction (ADR-0012 §6).

use time::OffsetDateTime;

/// Prévision météo agrégée au niveau national, à un `valid_at` donné, telle que
/// produite à `run_at`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherForecast {
    /// Instant de **production** de la prévision (= instant d'ingestion).
    pub run_at: OffsetDateTime,
    /// Instant **prévu**.
    pub valid_at: OffsetDateTime,
    /// Vitesse du vent à 100 m (km/h), moyenne des points nationaux.
    pub wind: f64,
    /// Rayonnement solaire incident (W/m²), moyenne des points nationaux.
    pub irradiance: f64,
}
