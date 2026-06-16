//! DTO de réponse : projection du domaine en JSON (la sérialisation vit ici,
//! jamais dans `core`). L'unité canonique est exposée explicitement.

use carbonfr_core::domain::{
    ForecastPoint, GenerationMix, GreenWindow, IntensityStats, Measurement, RollupBucket,
    VisitStats,
};
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

/// Réponse de `GET /v1/intensity/forecast` : la série **prévue** sur l'horizon.
///
/// Le champ `model` (identité versionnée du modèle, ex. `climatology@1`) marque
/// explicitement ces points comme des **prévisions** — pas des observations
/// (ADR-0011). Chaque point porte une **estimation centrale encadrée**
/// (`expected`/`lower`/`upper`) ; pas de `vintage` : une prévision n'est pas une
/// mesure révisée et n'est jamais persistée.
#[derive(Serialize, ToSchema)]
pub(crate) struct ForecastResponse {
    region: String,
    methodology: String,
    /// Identité versionnée du modèle de prévision (ex. `climatology@1`).
    model: String,
    /// Début de l'horizon (RFC 3339).
    from: String,
    /// Profondeur de l'horizon, en heures.
    horizon_hours: u32,
    unit: &'static str,
    count: usize,
    data: Vec<ForecastPointBody>,
}

#[derive(Serialize, ToSchema)]
struct ForecastPointBody {
    timestamp: String,
    /// Estimation centrale (gCO₂eq/kWh).
    expected: f64,
    /// Borne basse de l'intervalle d'incertitude.
    lower: f64,
    /// Borne haute de l'intervalle d'incertitude.
    upper: f64,
}

impl ForecastResponse {
    pub(crate) fn new(
        region: &str,
        methodology: &str,
        model: &str,
        from: OffsetDateTime,
        horizon_hours: u32,
        points: &[ForecastPoint],
    ) -> Result<Self, time::error::Format> {
        let data = points
            .iter()
            .map(|p| {
                Ok(ForecastPointBody {
                    timestamp: to_rfc3339(p.at)?,
                    expected: p.expected.value(),
                    lower: p.lower.value(),
                    upper: p.upper.value(),
                })
            })
            .collect::<Result<Vec<_>, time::error::Format>>()?;

        Ok(Self {
            region: region.to_string(),
            methodology: methodology.to_string(),
            model: model.to_string(),
            from: to_rfc3339(from)?,
            horizon_hours,
            unit: "gCO2eq/kWh",
            count: data.len(),
            data,
        })
    }
}

/// Réponse de `GET /v1/intensity/greenest-window` : le créneau le plus
/// bas-carbone sur l'horizon prévu (ADR-0009).
#[derive(Serialize, ToSchema)]
pub(crate) struct GreenestWindowResponse {
    region: String,
    methodology: String,
    /// Identité versionnée du modèle de prévision (ex. `climatology@1`).
    model: String,
    /// Début du créneau (RFC 3339).
    start: String,
    /// Fin du créneau (RFC 3339, exclue).
    end: String,
    unit: &'static str,
    /// Intensité carbone moyenne prévue sur le créneau.
    average_intensity: f64,
}

impl GreenestWindowResponse {
    pub(crate) fn new(
        region: &str,
        methodology: &str,
        model: &str,
        window: &GreenWindow,
    ) -> Result<Self, time::error::Format> {
        Ok(Self {
            region: region.to_string(),
            methodology: methodology.to_string(),
            model: model.to_string(),
            start: to_rfc3339(window.start)?,
            end: to_rfc3339(window.end)?,
            unit: "gCO2eq/kWh",
            average_intensity: window.average.value(),
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

/// Réponse de `GET /v1/stats` et `POST /v1/stats/visit` : compteur de visiteurs.
#[derive(Serialize, ToSchema)]
pub(crate) struct VisitStatsResponse {
    /// Visiteurs uniques (clés distinctes).
    unique: u64,
    /// Visiteur-jours cumulés.
    total: u64,
    /// Premier jour comptabilisé (ISO `YYYY-MM-DD`), absent si aucun.
    #[serde(skip_serializing_if = "Option::is_none")]
    since: Option<String>,
}

impl From<&VisitStats> for VisitStatsResponse {
    fn from(stats: &VisitStats) -> Self {
        Self {
            unique: stats.unique,
            total: stats.total,
            since: stats.since.map(|day| day.to_string()),
        }
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

/// Une méthodologie disponible (catalogue de `GET /v1/methodologies`).
#[derive(Serialize, ToSchema)]
pub(crate) struct MethodologyInfo {
    /// Identifiant stable (`rte-direct`, `acv-ademe`).
    id: &'static str,
    /// Version de la méthode (la version fait partie de son identité, ADR-0005).
    version: u32,
    /// Périmètre de calcul.
    basis: &'static str,
    /// Couverture géographique servie.
    scope: &'static str,
    /// `true` si c'est la méthode servie par défaut quand `?methodology=` est absent.
    default: bool,
    /// `served` = interrogeable aujourd'hui ; `planned` = spécifiée mais pas
    /// encore servie (dépend d'une source à brancher).
    status: &'static str,
    /// ADR de référence.
    adr: &'static str,
    description: &'static str,
}

/// Réponse de `GET /v1/methodologies` — catalogue des méthodes + versions.
#[derive(Serialize, ToSchema)]
pub(crate) struct MethodologiesResponse {
    methodologies: Vec<MethodologyInfo>,
}

impl MethodologiesResponse {
    /// Catalogue statique des méthodes (ADR-0005/0008/0010). Le défaut est
    /// `rte-direct` (comparabilité directe à éCO2mix).
    pub(crate) fn catalog() -> Self {
        Self {
            methodologies: vec![
                MethodologyInfo {
                    id: "rte-direct",
                    version: 1,
                    basis: "combustion directe de la production FR (estimation RTE)",
                    scope: "national",
                    default: true,
                    status: "served",
                    adr: "ADR-0005",
                    description: "Reprise du taux_co2 publié par RTE (éCO2mix). \
                                  Émissions de la seule production française, hors cycle de vie.",
                },
                MethodologyInfo {
                    id: "acv-ademe",
                    version: 1,
                    basis: "cycle de vie, basée production",
                    scope: "national + 12 régions",
                    default: false,
                    status: "served",
                    adr: "ADR-0008",
                    description: "Facteurs cycle de vie ADEME (Base Carbone) pondérés par le \
                                  mix de production. Imports exclus (production locale).",
                },
                MethodologyInfo {
                    id: "acv-ademe",
                    version: 2,
                    basis: "cycle de vie, basée consommation (imports inclus)",
                    scope: "national",
                    default: false,
                    status: "served",
                    adr: "ADR-0010",
                    description: "Empreinte de l'électricité réellement consommée : imports \
                                  valorisés à l'intensité du voisin (ENTSO-E) + pertes T&D. \
                                  Servie via ?methodology=acv-ademe&version=2 (national), si le \
                                  contexte d'import a été ingéré.",
                },
            ],
        }
    }
}

/// Un facteur d'émission par filière (entrée de `GET /v1/factors`).
#[derive(Serialize, ToSchema)]
pub(crate) struct FactorEntry {
    /// Filière (`nucleaire`, `gaz`, …).
    filiere: &'static str,
    /// Facteur cycle de vie (gCO₂eq/kWh).
    factor: f64,
}

/// Réponse de `GET /v1/factors` — table des facteurs d'une méthode (vérifiabilité).
#[derive(Serialize, ToSchema)]
pub(crate) struct FactorsResponse {
    methodology: String,
    methodology_version: u32,
    unit: &'static str,
    source: &'static str,
    factors: Vec<FactorEntry>,
    /// Facteur de pertes T&D appliqué (uplift consommation), `null` hors méthode
    /// consommation.
    td_loss_factor: Option<f64>,
}

impl FactorsResponse {
    /// Table des facteurs `acv-ademe` (commune à `@1` et `@2`), avec le facteur
    /// de pertes T&D pour la version consommation (`@2`).
    pub(crate) fn acv_ademe(version: u32) -> Self {
        let f = carbonfr_core::domain::EmissionFactors::acv_ademe_v1();
        let factors = vec![
            FactorEntry {
                filiere: "nucleaire",
                factor: f.nucleaire,
            },
            FactorEntry {
                filiere: "gaz",
                factor: f.gaz,
            },
            FactorEntry {
                filiere: "charbon",
                factor: f.charbon,
            },
            FactorEntry {
                filiere: "fioul",
                factor: f.fioul,
            },
            FactorEntry {
                filiere: "hydraulique",
                factor: f.hydraulique,
            },
            FactorEntry {
                filiere: "eolien",
                factor: f.eolien,
            },
            FactorEntry {
                filiere: "solaire",
                factor: f.solaire,
            },
            FactorEntry {
                filiere: "bioenergies",
                factor: f.bioenergies,
            },
            FactorEntry {
                filiere: "thermique",
                factor: f.thermique,
            },
        ];
        let td_loss_factor = (version >= 2).then_some(carbonfr_core::domain::TD_LOSS_FACTOR_V1);
        Self {
            methodology: "acv-ademe".to_string(),
            methodology_version: version,
            unit: "gCO2eq/kWh",
            source: "Base Carbone ADEME (cf. ADR-0008 ; pertes T&D ADR-0010)",
            factors,
            td_loss_factor,
        }
    }
}

/// Un créneau de scheduling (résultat de `lowest-k` ou `below`).
#[derive(Serialize, ToSchema)]
pub(crate) struct SlotBody {
    timestamp: String,
    /// Intensité prévue du créneau (gCO₂eq/kWh), selon l'estimateur.
    intensity: f64,
}

/// Réponse d'une liste de créneaux (`/v1/schedule/slots`, `/v1/intensity/below`).
#[derive(Serialize, ToSchema)]
pub(crate) struct SlotsResponse {
    region: String,
    methodology: String,
    /// Identité versionnée du modèle de prévision (ex. `climatology@1`).
    model: String,
    /// Estimateur appliqué : `central` ou `prudent`.
    estimator: &'static str,
    unit: &'static str,
    count: usize,
    slots: Vec<SlotBody>,
}

impl SlotsResponse {
    pub(crate) fn new(
        region: &str,
        methodology: &str,
        model: &str,
        estimator: &'static str,
        slots: &[carbonfr_core::domain::ScheduleSlot],
    ) -> Result<Self, time::error::Format> {
        let slots = slots
            .iter()
            .map(|s| {
                Ok(SlotBody {
                    timestamp: to_rfc3339(s.at)?,
                    intensity: s.intensity.value(),
                })
            })
            .collect::<Result<Vec<_>, time::error::Format>>()?;
        Ok(Self {
            region: region.to_string(),
            methodology: methodology.to_string(),
            model: model.to_string(),
            estimator,
            unit: "gCO2eq/kWh",
            count: slots.len(),
            slots,
        })
    }
}

/// Économie carbone d'un créneau planifié vs « maintenant » (ADR-0014).
#[derive(Serialize, ToSchema)]
pub(crate) struct SavingsBody {
    /// Intensité « maintenant » (gCO₂eq/kWh).
    now: f64,
    /// Intensité du créneau planifié (gCO₂eq/kWh).
    scheduled: f64,
    /// Réduction d'intensité (gCO₂eq/kWh) : `now − scheduled`.
    intensity_delta: f64,
    /// Réduction relative en pourcentage.
    reduction_percent: f64,
    /// Économie absolue (gCO₂eq) si l'énergie du job (`energy_kwh`) est fournie.
    #[serde(skip_serializing_if = "Option::is_none")]
    absolute_saved_g: Option<f64>,
}

/// Réponse de `GET /v1/schedule` : créneau retenu + économie vs maintenant.
#[derive(Serialize, ToSchema)]
pub(crate) struct ScheduleResponse {
    region: String,
    methodology: String,
    model: String,
    estimator: &'static str,
    unit: &'static str,
    /// Début du créneau planifié (RFC 3339).
    start: String,
    /// Fin du créneau planifié (RFC 3339, exclue).
    end: String,
    /// Intensité moyenne prévue sur le créneau.
    average_intensity: f64,
    savings: SavingsBody,
}

impl ScheduleResponse {
    pub(crate) fn new(
        region: &str,
        methodology: &str,
        model: &str,
        estimator: &'static str,
        scheduled: &carbonfr_core::application::ScheduledWindow,
    ) -> Result<Self, time::error::Format> {
        let s = &scheduled.savings;
        Ok(Self {
            region: region.to_string(),
            methodology: methodology.to_string(),
            model: model.to_string(),
            estimator,
            unit: "gCO2eq/kWh",
            start: to_rfc3339(scheduled.window.start)?,
            end: to_rfc3339(scheduled.window.end)?,
            average_intensity: scheduled.window.average.value(),
            savings: SavingsBody {
                now: s.now.value(),
                scheduled: s.scheduled.value(),
                intensity_delta: s.intensity_delta,
                reduction_percent: s.fraction * 100.0,
                absolute_saved_g: s.absolute_g,
            },
        })
    }
}
