//! Handlers axum : chaque endpoint câble un cas d'usage du `core` et projette
//! le résultat en DTO JSON.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use carbonfr_core::application::{GetCurrentIntensity, GetIntensityHistory, GetIntensityStats};
use carbonfr_core::domain::{Granularity, Region, TimeRange};
use carbonfr_core::ports::{IntensityRepository, VisitCounter};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};
use utoipa::IntoParams;

use crate::AppState;
use crate::dto::{
    HistoryResponse, IntensityResponse, MixResponse, StatsResponse, VisitStatsResponse,
};
use crate::error::{ApiError, ErrorBody};

/// Fenêtre maximale d'une requête d'historique (protège le serveur d'une
/// extraction démesurée). Au-delà → 400.
const MAX_HISTORY_SPAN: Duration = Duration::days(366);

/// Paramètre de requête commun : `?region=<slug>`, national par défaut.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct RegionQuery {
    /// Slug de région (ex. `bretagne`). National par défaut.
    region: Option<String>,
    /// Méthodologie : `rte-direct` (national) ou `acv-ademe`. Défaut `rte-direct`.
    methodology: Option<String>,
}

impl RegionQuery {
    fn resolve(&self) -> Result<Region, ApiError> {
        resolve_region(&self.region)
    }
}

/// Résout un slug de région optionnel en [`Region`] (national par défaut, 400
/// si inconnu).
fn resolve_region(slug: &Option<String>) -> Result<Region, ApiError> {
    match slug {
        None => Ok(Region::National),
        Some(slug) => Region::from_slug(slug)
            .ok_or_else(|| ApiError::bad_request(format!("région inconnue : {slug}"))),
    }
}

/// Méthodologie demandée (`?methodology=`), ou celle par défaut de l'état.
fn resolve_methodology(requested: &Option<String>, default: &str) -> String {
    requested.clone().unwrap_or_else(|| default.to_string())
}

/// Parse un horodatage RFC 3339 fourni en paramètre, ou 400.
fn parse_timestamp(name: &str, raw: &str) -> Result<OffsetDateTime, ApiError> {
    OffsetDateTime::parse(raw, &Rfc3339)
        .map_err(|_| ApiError::bad_request(format!("`{name}` : horodatage RFC 3339 invalide")))
}

/// `GET /v1/intensity/now` — dernière intensité carbone connue.
#[utoipa::path(
    get,
    path = "/v1/intensity/now",
    params(RegionQuery),
    responses(
        (status = 200, description = "Dernière mesure", body = IntensityResponse),
        (status = 400, description = "Région ou méthodologie invalide", body = ErrorBody),
        (status = 404, description = "Aucune donnée", body = ErrorBody),
    ),
    tag = "intensité"
)]
pub(crate) async fn intensity_now<R>(
    State(state): State<AppState<R>>,
    Query(query): Query<RegionQuery>,
) -> Result<Json<IntensityResponse>, ApiError>
where
    R: IntensityRepository + Clone + Send + Sync + 'static,
{
    let region = query.resolve()?;
    let methodology = resolve_methodology(&query.methodology, &state.methodology);
    let use_case = GetCurrentIntensity::new(state.repo.clone(), methodology);
    let measurement = use_case.execute(region).await?;
    Ok(Json(IntensityResponse::from_measurement(&measurement)?))
}

/// `GET /v1/mix` — mix de production de la dernière mesure.
#[utoipa::path(
    get,
    path = "/v1/mix",
    params(RegionQuery),
    responses(
        (status = 200, description = "Mix de production (MW)", body = MixResponse),
        (status = 400, description = "Région ou méthodologie invalide", body = ErrorBody),
        (status = 404, description = "Aucune donnée / mix indisponible", body = ErrorBody),
    ),
    tag = "mix"
)]
pub(crate) async fn mix<R>(
    State(state): State<AppState<R>>,
    Query(query): Query<RegionQuery>,
) -> Result<Json<MixResponse>, ApiError>
where
    R: IntensityRepository + Clone + Send + Sync + 'static,
{
    let region = query.resolve()?;
    let methodology = resolve_methodology(&query.methodology, &state.methodology);
    let use_case = GetCurrentIntensity::new(state.repo.clone(), methodology);
    let measurement = use_case.execute(region).await?;
    let mix = measurement.mix.as_ref().ok_or_else(|| {
        ApiError::not_found(format!(
            "mix de production indisponible pour la région {}",
            region.slug()
        ))
    })?;
    Ok(Json(MixResponse::from_measurement(&measurement, mix)?))
}

/// Paramètres de `GET /v1/intensity/date`.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct HistoryQuery {
    /// Début de l'intervalle (RFC 3339, inclus). Requis.
    from: Option<String>,
    /// Fin de l'intervalle (RFC 3339, exclu). Requis.
    to: Option<String>,
    /// Slug de région. National par défaut.
    region: Option<String>,
    /// Méthodologie. Défaut `rte-direct`.
    methodology: Option<String>,
}

/// `GET /v1/intensity/date?from=&to=&region=` — série historique sur un
/// intervalle `[from, to)` (RFC 3339), national par défaut.
#[utoipa::path(
    get,
    path = "/v1/intensity/date",
    params(HistoryQuery),
    responses(
        (status = 200, description = "Série chronologique", body = HistoryResponse),
        (status = 400, description = "Paramètre invalide ou fenêtre > 366 jours", body = ErrorBody),
    ),
    tag = "intensité"
)]
pub(crate) async fn intensity_date<R>(
    State(state): State<AppState<R>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<HistoryResponse>, ApiError>
where
    R: IntensityRepository + Clone + Send + Sync + 'static,
{
    let region = resolve_region(&query.region)?;

    let from_raw = query
        .from
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("paramètre `from` requis (RFC 3339)"))?;
    let to_raw = query
        .to
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("paramètre `to` requis (RFC 3339)"))?;

    let from = parse_timestamp("from", from_raw)?;
    let to = parse_timestamp("to", to_raw)?;

    if to - from > MAX_HISTORY_SPAN {
        return Err(ApiError::bad_request(
            "fenêtre trop large (maximum 366 jours)",
        ));
    }
    let range = TimeRange::new(from, to)
        .ok_or_else(|| ApiError::bad_request("`to` doit être strictement postérieur à `from`"))?;

    let methodology = resolve_methodology(&query.methodology, &state.methodology);
    let use_case = GetIntensityHistory::new(state.repo.clone(), methodology.clone());
    let measurements = use_case.execute(region, range).await?;

    Ok(Json(HistoryResponse::new(
        region.slug(),
        from,
        to,
        &methodology,
        &measurements,
    )?))
}

/// Paramètres de `GET /v1/intensity/stats`.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct StatsQuery {
    /// Début de l'intervalle (RFC 3339, inclus). Requis.
    from: Option<String>,
    /// Fin de l'intervalle (RFC 3339, exclu). Requis.
    to: Option<String>,
    /// Slug de région. National par défaut.
    region: Option<String>,
    /// Pas d'agrégation de la série : `hour` ou `day`. Optionnel.
    interval: Option<String>,
    /// Méthodologie. Défaut `rte-direct`.
    methodology: Option<String>,
}

/// `GET /v1/intensity/stats?from=&to=&region=&interval=` — résumé (moyenne/min/
/// max) sur `[from, to)`, et série agrégée par pas si `interval=hour|day`.
#[utoipa::path(
    get,
    path = "/v1/intensity/stats",
    params(StatsQuery),
    responses(
        (status = 200, description = "Résumé (et série si interval)", body = StatsResponse),
        (status = 400, description = "Paramètre invalide", body = ErrorBody),
        (status = 404, description = "Aucune donnée sur l'intervalle", body = ErrorBody),
    ),
    tag = "intensité"
)]
pub(crate) async fn intensity_stats<R>(
    State(state): State<AppState<R>>,
    Query(query): Query<StatsQuery>,
) -> Result<Json<StatsResponse>, ApiError>
where
    R: IntensityRepository + Clone + Send + Sync + 'static,
{
    let region = resolve_region(&query.region)?;

    let from_raw = query
        .from
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("paramètre `from` requis (RFC 3339)"))?;
    let to_raw = query
        .to
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("paramètre `to` requis (RFC 3339)"))?;
    let from = parse_timestamp("from", from_raw)?;
    let to = parse_timestamp("to", to_raw)?;

    if to - from > MAX_HISTORY_SPAN {
        return Err(ApiError::bad_request(
            "fenêtre trop large (maximum 366 jours)",
        ));
    }
    let range = TimeRange::new(from, to)
        .ok_or_else(|| ApiError::bad_request("`to` doit être strictement postérieur à `from`"))?;

    let methodology = resolve_methodology(&query.methodology, &state.methodology);
    let use_case = GetIntensityStats::new(state.repo.clone(), methodology.clone());
    let summary = use_case.summary(region, range).await?.ok_or_else(|| {
        ApiError::not_found(format!(
            "aucune donnée sur l'intervalle pour la région {}",
            region.slug()
        ))
    })?;

    let (interval_label, buckets) = match query.interval.as_deref() {
        None => (None, None),
        Some(raw) => {
            let granularity = Granularity::from_label(raw)
                .ok_or_else(|| ApiError::bad_request("`interval` doit valoir `hour` ou `day`"))?;
            let series = use_case.series(region, range, granularity).await?;
            (Some(granularity.label()), Some(series))
        }
    };

    Ok(Json(StatsResponse::new(
        region.slug(),
        from,
        to,
        &methodology,
        &summary,
        interval_label,
        buckets.as_deref(),
    )?))
}

/// Adresse IP du client, lue des en-têtes posés par le reverse proxy
/// (`X-Forwarded-For` puis `X-Real-IP`, ADR-0007). `unknown` à défaut (accès
/// direct sans proxy) — toutes ces visites tombent alors dans un même seau.
fn client_ip(headers: &HeaderMap) -> String {
    for header in ["x-forwarded-for", "x-real-ip"] {
        if let Some(value) = headers.get(header).and_then(|v| v.to_str().ok()) {
            let first = value.split(',').next().unwrap_or("").trim();
            if !first.is_empty() {
                return first.to_string();
            }
        }
    }
    "unknown".to_string()
}

/// Clé visiteur anonyme : `SHA-256(sel | ip)`. L'IP n'est jamais stockée.
fn hash_visitor(salt: &str, ip: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b"|");
    hasher.update(ip.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// `GET /v1/stats` — statistiques de consultation.
#[utoipa::path(
    get,
    path = "/v1/stats",
    responses((status = 200, description = "Statistiques courantes", body = VisitStatsResponse)),
    tag = "opérations"
)]
pub(crate) async fn visit_stats<R>(
    State(state): State<AppState<R>>,
) -> Result<Json<VisitStatsResponse>, ApiError>
where
    R: VisitCounter + Clone + Send + Sync + 'static,
{
    let stats = state.repo.visit_stats().await?;
    Ok(Json((&stats).into()))
}

/// `POST /v1/stats/visit` — enregistre une visite (unique par IP/jour, IP
/// hachée jamais stockée) et renvoie les statistiques à jour.
#[utoipa::path(
    post,
    path = "/v1/stats/visit",
    responses((status = 200, description = "Statistiques à jour", body = VisitStatsResponse)),
    tag = "opérations"
)]
pub(crate) async fn record_visit<R>(
    State(state): State<AppState<R>>,
    headers: HeaderMap,
) -> Result<Json<VisitStatsResponse>, ApiError>
where
    R: VisitCounter + Clone + Send + Sync + 'static,
{
    let ip = client_ip(&headers);
    let visitor = hash_visitor(&state.visit_salt, &ip);
    let day = OffsetDateTime::now_utc().date();
    let stats = state.repo.record_visit(&visitor, day).await?;
    Ok(Json((&stats).into()))
}

/// `GET /health` — sonde de disponibilité (hors contrat d'API versionné).
#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Service disponible", body = String)),
    tag = "opérations"
)]
pub(crate) async fn health() -> &'static str {
    "ok"
}
