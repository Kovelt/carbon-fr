//! Cas d'usage : scheduling **carbon-aware** (ADR-0014 §1).
//!
//! Récupère une prévision via [`ForecastModel`] puis applique les primitives
//! **pures** du domaine (créneau sous échéance, divisible *lowest-k*, seuil,
//! économie). Aucun nouveau port : ce sont des conseils calculés sur la
//! prévision.

use time::{Duration, OffsetDateTime};

use crate::domain::{
    GreenWindow, Region, Savings, ScheduleSlot, WindowEstimator, greenest_window,
    greenest_window_before, lowest_slots, savings_vs_now, slots_below,
};
use crate::ports::ForecastModel;

use super::ApplicationError;

/// Réponse du créneau planifié : le créneau retenu + l'économie vs « maintenant ».
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScheduledWindow {
    pub window: GreenWindow,
    pub savings: Savings,
}

/// Scheduler carbon-aware : une façade sur la prévision proposant les primitives
/// orientées job (ADR-0014). Toutes opèrent sur la série `(region, methodology)`.
pub struct CarbonAwareScheduler<F: ForecastModel> {
    forecast: F,
}

impl<F: ForecastModel> CarbonAwareScheduler<F> {
    pub fn new(forecast: F) -> Self {
        Self { forecast }
    }

    async fn series(
        &self,
        region: Region,
        methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<crate::domain::ForecastPoint>, ApplicationError> {
        Ok(self
            .forecast
            .forecast(region, methodology_id, from, horizon)
            .await?)
    }

    /// Créneau contigu de `duration` minimisant l'intensité, borné par `deadline`
    /// (optionnel), + économie vs maintenant (absolue si `energy_kwh` fourni).
    #[allow(clippy::too_many_arguments)]
    pub async fn schedule_window(
        &self,
        region: Region,
        methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
        duration: Duration,
        deadline: Option<OffsetDateTime>,
        energy_kwh: Option<f64>,
        estimator: WindowEstimator,
    ) -> Result<ScheduledWindow, ApplicationError> {
        let points = self.series(region, methodology_id, from, horizon).await?;
        let window = match deadline {
            Some(deadline) => greenest_window_before(&points, duration, deadline, estimator),
            None => greenest_window(&points, duration, estimator),
        }
        .ok_or(ApplicationError::InsufficientSeries)?;
        let savings = savings_vs_now(&points, window.average, energy_kwh, estimator)
            .ok_or(ApplicationError::InsufficientSeries)?;
        Ok(ScheduledWindow { window, savings })
    }

    /// Les `k` créneaux les moins intenses (job divisible, interruptible).
    pub async fn lowest_slots(
        &self,
        region: Region,
        methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
        k: usize,
        estimator: WindowEstimator,
    ) -> Result<Vec<ScheduleSlot>, ApplicationError> {
        let points = self.series(region, methodology_id, from, horizon).await?;
        Ok(lowest_slots(&points, k, estimator))
    }

    /// Tous les créneaux d'intensité sous `threshold` (gCO₂eq/kWh) sur l'horizon.
    pub async fn slots_below(
        &self,
        region: Region,
        methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
        threshold: f64,
        estimator: WindowEstimator,
    ) -> Result<Vec<ScheduleSlot>, ApplicationError> {
        let points = self.series(region, methodology_id, from, horizon).await?;
        Ok(slots_below(&points, threshold, estimator))
    }
}
