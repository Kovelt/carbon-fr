//! Handlers axum : chaque endpoint câble un cas d'usage du `core` et projette
//! le résultat en DTO JSON.

use axum::Json;
use axum::extract::{Query, State};
use carbonfr_core::application::GetCurrentIntensity;
use carbonfr_core::domain::Region;
use carbonfr_core::ports::IntensityRepository;
use serde::Deserialize;

use crate::AppState;
use crate::dto::{IntensityResponse, MixResponse};
use crate::error::ApiError;

/// Paramètre de requête commun : `?region=<slug>`, national par défaut.
#[derive(Deserialize)]
pub(crate) struct RegionQuery {
    region: Option<String>,
}

impl RegionQuery {
    fn resolve(&self) -> Result<Region, ApiError> {
        match &self.region {
            None => Ok(Region::National),
            Some(slug) => Region::from_slug(slug)
                .ok_or_else(|| ApiError::bad_request(format!("région inconnue : {slug}"))),
        }
    }
}

/// `GET /v1/intensity/now` — dernière intensité carbone connue.
pub(crate) async fn intensity_now<R>(
    State(state): State<AppState<R>>,
    Query(query): Query<RegionQuery>,
) -> Result<Json<IntensityResponse>, ApiError>
where
    R: IntensityRepository + Clone + Send + Sync + 'static,
{
    let region = query.resolve()?;
    let use_case = GetCurrentIntensity::new(state.repo.clone(), state.methodology.clone());
    let measurement = use_case.execute(region).await?;
    Ok(Json(IntensityResponse::from_measurement(&measurement)?))
}

/// `GET /v1/mix` — mix de production de la dernière mesure.
pub(crate) async fn mix<R>(
    State(state): State<AppState<R>>,
    Query(query): Query<RegionQuery>,
) -> Result<Json<MixResponse>, ApiError>
where
    R: IntensityRepository + Clone + Send + Sync + 'static,
{
    let region = query.resolve()?;
    let use_case = GetCurrentIntensity::new(state.repo.clone(), state.methodology.clone());
    let measurement = use_case.execute(region).await?;
    let mix = measurement.mix.as_ref().ok_or_else(|| {
        ApiError::not_found(format!(
            "mix de production indisponible pour la région {}",
            region.slug()
        ))
    })?;
    Ok(Json(MixResponse::from_measurement(&measurement, mix)?))
}

/// `GET /health` — sonde de disponibilité (hors contrat d'API versionné).
pub(crate) async fn health() -> &'static str {
    "ok"
}
