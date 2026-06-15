//! Cas d'usage : trouver le créneau le plus bas-carbone à venir.

use time::{Duration, OffsetDateTime};

use crate::domain::{GreenWindow, Region, WindowEstimator, greenest_window};
use crate::ports::ForecastModel;

use super::ApplicationError;

/// Détermine, sur une prévision d'`horizon` à partir de `from`, le créneau
/// contigu de durée `window` minimisant l'intensité carbone moyenne.
///
/// C'est de la **valeur créée** (ADR / ARCHITECTURE §2) : la prévision
/// d'intensité n'existe pas à la source, elle est produite par l'adapter
/// branché derrière [`ForecastModel`]. Le choix du créneau, lui, est une
/// fonction pure du domaine ([`greenest_window`]).
pub struct FindGreenestWindow<F: ForecastModel> {
    forecast: F,
}

impl<F: ForecastModel> FindGreenestWindow<F> {
    pub fn new(forecast: F) -> Self {
        Self { forecast }
    }

    /// `from` : début de l'horizon ; `horizon` : profondeur de prévision ;
    /// `window` : durée du créneau recherché ; `estimator` : central (`expected`)
    /// ou prudent (`upper`). La prévision porte sur la série
    /// `(region, methodology_id)`.
    pub async fn execute(
        &self,
        region: Region,
        methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
        window: Duration,
        estimator: WindowEstimator,
    ) -> Result<GreenWindow, ApplicationError> {
        let points = self
            .forecast
            .forecast(region, methodology_id, from, horizon)
            .await?;
        greenest_window(&points, window, estimator).ok_or(ApplicationError::InsufficientSeries)
    }
}
