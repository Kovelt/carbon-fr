//! DTO de réponse : projection du domaine en JSON (la sérialisation vit ici,
//! jamais dans `core`). L'unité canonique est exposée explicitement.

use carbonfr_core::domain::{GenerationMix, IntensityStats, Measurement, RollupBucket};
use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use utoipa::ToSchema;

fn to_rfc3339(at: OffsetDateTime) -> Result<String, time::error::Format> {
    at.format(&Rfc3339)
}

/// Réponse de `GET /v1/intensity/now`.
#[derive(Serialize, ToSchema)]
pub(crate) struct IntensityResponse {
    region: String,
    timestamp: String,
    intensity: IntensityValue,
    methodology: String,
    methodology_version: u32,
    vintage: &'static str,
}

#[derive(Serialize, ToSchema)]
struct IntensityValue {
    value: f64,
    unit: &'static str,
}

impl IntensityResponse {
    pub(crate) fn from_measurement(m: &Measurement) -> Result<Self, time::error::Format> {
        Ok(Self {
            region: m.region.slug().to_string(),
            timestamp: to_rfc3339(m.at)?,
            intensity: IntensityValue {
                value: m.intensity.value(),
                unit: "gCO2eq/kWh",
            },
            methodology: m.methodology.id.clone(),
            methodology_version: m.methodology.version,
            vintage: m.vintage.code(),
        })
    }
}

/// Réponse de `GET /v1/intensity/date` : la série sur l'intervalle demandé.
#[derive(Serialize, ToSchema)]
pub(crate) struct HistoryResponse {
    region: String,
    from: String,
    to: String,
    unit: &'static str,
    methodology: String,
    count: usize,
    data: Vec<HistoryPoint>,
}

#[derive(Serialize, ToSchema)]
struct HistoryPoint {
    timestamp: String,
    intensity: f64,
    vintage: &'static str,
}

impl HistoryResponse {
    pub(crate) fn new(
        region: &str,
        from: OffsetDateTime,
        to: OffsetDateTime,
        methodology: &str,
        measurements: &[Measurement],
    ) -> Result<Self, time::error::Format> {
        let data = measurements
            .iter()
            .map(|m| {
                Ok(HistoryPoint {
                    timestamp: to_rfc3339(m.at)?,
                    intensity: m.intensity.value(),
                    vintage: m.vintage.code(),
                })
            })
            .collect::<Result<Vec<_>, time::error::Format>>()?;

        Ok(Self {
            region: region.to_string(),
            from: to_rfc3339(from)?,
            to: to_rfc3339(to)?,
            unit: "gCO2eq/kWh",
            methodology: methodology.to_string(),
            count: data.len(),
            data,
        })
    }
}

/// Réponse de `GET /v1/mix`.
#[derive(Serialize, ToSchema)]
pub(crate) struct MixResponse {
    region: String,
    timestamp: String,
    unit: &'static str,
    mix: MixBody,
}

#[derive(Serialize, ToSchema)]
struct MixBody {
    nucleaire: f64,
    gaz: f64,
    charbon: f64,
    fioul: f64,
    hydraulique: f64,
    eolien: f64,
    solaire: f64,
    bioenergies: f64,
    pompage: f64,
    echanges: f64,
    /// Thermique fossile agrégé (mix régional uniquement ; omis au national).
    #[serde(skip_serializing_if = "Option::is_none")]
    thermique: Option<f64>,
}

impl MixResponse {
    pub(crate) fn from_measurement(
        m: &Measurement,
        mix: &GenerationMix,
    ) -> Result<Self, time::error::Format> {
        Ok(Self {
            region: m.region.slug().to_string(),
            timestamp: to_rfc3339(m.at)?,
            unit: "MW",
            mix: MixBody {
                nucleaire: mix.nucleaire,
                gaz: mix.gaz,
                charbon: mix.charbon,
                fioul: mix.fioul,
                hydraulique: mix.hydraulique,
                eolien: mix.eolien,
                solaire: mix.solaire,
                bioenergies: mix.bioenergies,
                pompage: mix.pompage,
                echanges: mix.echanges,
                thermique: mix.thermique,
            },
        })
    }
}

/// Réponse de `GET /v1/intensity/stats` : résumé sur l'intervalle, et série
/// agrégée par pas si `interval` est fourni.
#[derive(Serialize, ToSchema)]
pub(crate) struct StatsResponse {
    region: String,
    from: String,
    to: String,
    unit: &'static str,
    methodology: String,
    average: f64,
    min: f64,
    max: f64,
    count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    intervals: Option<Vec<StatsBucket>>,
}

#[derive(Serialize, ToSchema)]
struct StatsBucket {
    start: String,
    average: f64,
    min: f64,
    max: f64,
    count: u64,
}

impl StatsResponse {
    pub(crate) fn new(
        region: &str,
        from: OffsetDateTime,
        to: OffsetDateTime,
        methodology: &str,
        summary: &IntensityStats,
        interval: Option<&'static str>,
        buckets: Option<&[RollupBucket]>,
    ) -> Result<Self, time::error::Format> {
        let intervals = buckets
            .map(|buckets| {
                buckets
                    .iter()
                    .map(|b| {
                        Ok(StatsBucket {
                            start: to_rfc3339(b.start)?,
                            average: b.stats.average.value(),
                            min: b.stats.min.value(),
                            max: b.stats.max.value(),
                            count: b.stats.count,
                        })
                    })
                    .collect::<Result<Vec<_>, time::error::Format>>()
            })
            .transpose()?;

        Ok(Self {
            region: region.to_string(),
            from: to_rfc3339(from)?,
            to: to_rfc3339(to)?,
            unit: "gCO2eq/kWh",
            methodology: methodology.to_string(),
            average: summary.average.value(),
            min: summary.min.value(),
            max: summary.max.value(),
            count: summary.count,
            interval,
            intervals,
        })
    }
}
