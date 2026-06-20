//! DTO de réponse : projection du domaine en JSON (la sérialisation vit ici,
//! jamais dans `core`). L'unité canonique est exposée explicitement.

use carbonfr_core::domain::{
    COST_REFERENCE_DISCLAIMER, CostEstimate, CrossBorderSnapshot, ForecastPoint, GenerationMix,
    GreenWindow, IntensityStats, Measurement, Neighbor, PriceBreakdown, RenewableModel,
    RollupBucket, VisitStats, WeatherForecast,
};
use serde::{Deserialize, Serialize};
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

/// Attribution Open-Meteo (licence CC-BY 4.0) : crédit + lien + mention de
/// transformation (moyenne nationale), comme l'exige la licence.
const WEATHER_ATTRIBUTION: &str = "Open-Meteo (CC-BY 4.0, https://open-meteo.com) — moyenne nationale 7 points, donnée transformée";

/// Un créneau météo (vent à 100 m, irradiance), moyenne nationale.
#[derive(Serialize, ToSchema)]
struct WeatherPoint {
    /// Instant prévu (RFC 3339, UTC).
    valid_at: String,
    /// Instant de production de la prévision (run).
    run_at: String,
    /// Vent à 100 m (km/h), moyenne nationale.
    wind_kmh: f64,
    /// Rayonnement solaire incident (W/m²), moyenne nationale.
    irradiance_wm2: f64,
}

impl WeatherPoint {
    fn from_forecast(f: &WeatherForecast) -> Result<Self, time::error::Format> {
        Ok(Self {
            valid_at: to_rfc3339(f.valid_at)?,
            run_at: to_rfc3339(f.run_at)?,
            wind_kmh: f.wind,
            irradiance_wm2: f.irradiance,
        })
    }
}

/// Réponse de `GET /v1/weather` — météo nationale courante (ADR-0012/0018).
#[derive(Serialize, ToSchema)]
pub(crate) struct WeatherResponse {
    /// Attribution de la source (licence CC-BY 4.0).
    source: &'static str,
    valid_at: String,
    run_at: String,
    wind_kmh: f64,
    irradiance_wm2: f64,
}

impl WeatherResponse {
    pub(crate) fn from_forecast(f: &WeatherForecast) -> Result<Self, time::error::Format> {
        Ok(Self {
            source: WEATHER_ATTRIBUTION,
            valid_at: to_rfc3339(f.valid_at)?,
            run_at: to_rfc3339(f.run_at)?,
            wind_kmh: f.wind,
            irradiance_wm2: f.irradiance,
        })
    }
}

/// Réponse de `GET /v1/weather/date` — série météo historique.
#[derive(Serialize, ToSchema)]
pub(crate) struct WeatherHistoryResponse {
    source: &'static str,
    from: String,
    to: String,
    count: usize,
    points: Vec<WeatherPoint>,
}

impl WeatherHistoryResponse {
    pub(crate) fn new(
        from: OffsetDateTime,
        to: OffsetDateTime,
        forecasts: &[WeatherForecast],
    ) -> Result<Self, time::error::Format> {
        let points = forecasts
            .iter()
            .map(WeatherPoint::from_forecast)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            source: WEATHER_ATTRIBUTION,
            from: to_rfc3339(from)?,
            to: to_rfc3339(to)?,
            count: points.len(),
            points,
        })
    }
}

/// Attribution de la dérivation renouvelable : production **estimée** (notre
/// modèle, ADR-0018) à partir de la météo Open-Meteo (CC-BY 4.0).
const RENEWABLE_ATTRIBUTION: &str = "Production estimée par carbon-fr (modèle ADR-0018) à partir de la météo Open-Meteo (CC-BY 4.0) — valeurs modélisées, non mesurées";

/// Capacités effectives calibrées (transparence du modèle).
#[derive(Serialize, ToSchema)]
struct RenewableModelInfo {
    wind_capacity_mw: f64,
    solar_capacity_mw: f64,
}

/// Réponse de `GET /v1/renewable` — production renouvelable **estimée** depuis la
/// météo courante (modèle calibré, ADR-0018) + facteur de charge.
#[derive(Serialize, ToSchema)]
pub(crate) struct RenewableResponse {
    /// Attribution (météo Open-Meteo CC-BY 4.0 ; valeurs modélisées).
    source: &'static str,
    /// Instant de la météo utilisée (RFC 3339, UTC).
    at: String,
    /// Éolien estimé (MW).
    wind_mw: f64,
    /// Solaire estimé (MW).
    solar_mw: f64,
    /// Facteur de charge éolien (0–1) : part de la capacité installée réalisée.
    wind_capacity_factor: f64,
    /// Facteur de charge solaire (0–1).
    solar_capacity_factor: f64,
    /// Capacités effectives calibrées (transparence).
    model: RenewableModelInfo,
}

impl RenewableResponse {
    pub(crate) fn build(
        model: &RenewableModel,
        w: &WeatherForecast,
    ) -> Result<Self, time::error::Format> {
        let wind_mw = model.estimate_wind_mw(w.wind);
        let solar_mw = model.estimate_solar_mw(w.irradiance);
        let solar_cf = if model.solar_capacity_mw > 0.0 {
            solar_mw / model.solar_capacity_mw
        } else {
            0.0
        };
        Ok(Self {
            source: RENEWABLE_ATTRIBUTION,
            at: to_rfc3339(w.valid_at)?,
            wind_mw,
            solar_mw,
            wind_capacity_factor: model.wind_capacity_factor(w.wind),
            solar_capacity_factor: solar_cf,
            model: RenewableModelInfo {
                wind_capacity_mw: model.wind_capacity_mw,
                solar_capacity_mw: model.solar_capacity_mw,
            },
        })
    }
}

/// Réponse de `GET /v1/exchanges` — échanges transfrontaliers (ADR-0017).
///
/// Données déjà ingérées pour `acv-ademe@2` (flux ENTSO-E par frontière +
/// intensité du voisin), exposées au pas quart d'heure. Convention de signe :
/// **`> 0` = import vers la France**, `< 0` = export.
#[derive(Serialize, ToSchema)]
pub(crate) struct ExchangesResponse {
    /// Horodatage du snapshot (RFC 3339, UTC), aligné sur `/v1/intensity/now`.
    timestamp: String,
    /// Solde net FR (MW) : `> 0` = la France importe, `< 0` = exporte.
    net_flow_mw: f64,
    /// Sens du solde : `import` | `export` | `balanced`.
    direction: &'static str,
    /// Total importé (MW) — somme des frontières entrantes.
    imports_mw: f64,
    /// Total exporté (MW) — somme des frontières sortantes.
    exports_mw: f64,
    /// Détail par frontière.
    exchanges: Vec<ExchangeEntry>,
}

/// Une frontière : flux net signé FR↔voisin + intensité carbone du voisin.
#[derive(Serialize, ToSchema)]
struct ExchangeEntry {
    /// Code du voisin (`be`, `de-lu`, `es`, `it-north`, `ch`, `gb`).
    country: String,
    /// Nom lisible du voisin.
    country_name: &'static str,
    /// Flux net (MW) : `> 0` = la France **importe** de ce pays, `< 0` = exporte.
    flow_mw: f64,
    /// Sens FR↔pays pour la flèche : `import` | `export` | `balanced`.
    direction: &'static str,
    /// Intensité carbone (cycle de vie ADEME) du voisin au même instant.
    intensity: IntensityValue,
}

impl ExchangesResponse {
    pub(crate) fn from_snapshot(s: &CrossBorderSnapshot) -> Result<Self, time::error::Format> {
        let imports_mw = s.flows.imports_mw();
        let exports_mw = s.flows.exports_mw();
        let exchanges = s
            .flows
            .flows
            .iter()
            .map(|f| ExchangeEntry {
                country: f.neighbor.slug().to_string(),
                country_name: neighbor_name(f.neighbor),
                flow_mw: f.flow_mw,
                direction: flow_direction(f.flow_mw),
                intensity: IntensityValue {
                    value: f.neighbor_intensity.value(),
                    unit: "gCO2eq/kWh",
                },
            })
            .collect();
        Ok(Self {
            timestamp: to_rfc3339(s.at)?,
            net_flow_mw: imports_mw - exports_mw,
            direction: flow_direction(imports_mw - exports_mw),
            imports_mw,
            exports_mw,
            exchanges,
        })
    }
}

/// Sens d'un flux signé. Zone morte de 1 MW autour de zéro → `balanced`.
fn flow_direction(mw: f64) -> &'static str {
    if mw > 1.0 {
        "import"
    } else if mw < -1.0 {
        "export"
    } else {
        "balanced"
    }
}

/// Nom lisible (FR) d'un voisin électrique.
fn neighbor_name(n: Neighbor) -> &'static str {
    match n {
        Neighbor::Belgium => "Belgique",
        Neighbor::Germany => "Allemagne",
        Neighbor::Spain => "Espagne",
        Neighbor::Italy => "Italie",
        Neighbor::Switzerland => "Suisse",
        Neighbor::GreatBritain => "Royaume-Uni",
    }
}

/// Réponse de `GET /v1/exchanges/date` — série historique des échanges (ADR-0017).
#[derive(Serialize, ToSchema)]
pub(crate) struct ExchangesHistoryResponse {
    from: String,
    to: String,
    /// Nombre de snapshots renvoyés (pas quart d'heure).
    count: usize,
    /// Snapshots triés par horodatage croissant.
    snapshots: Vec<ExchangesResponse>,
}

impl ExchangesHistoryResponse {
    pub(crate) fn new(
        from: OffsetDateTime,
        to: OffsetDateTime,
        snapshots: &[CrossBorderSnapshot],
    ) -> Result<Self, time::error::Format> {
        let snapshots = snapshots
            .iter()
            .map(ExchangesResponse::from_snapshot)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            from: to_rfc3339(from)?,
            to: to_rfc3339(to)?,
            count: snapshots.len(),
            snapshots,
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

/// Avertissement sur la construction réglementaire (ADR-0023).
const PRICE_DISCLAIMER: &str = "Décomposition ancrée sur le Tarif Réglementé de Vente \
(empilement publié par la CRE), profil résidentiel BT ≤ 36 kVA option Base (6 kVA, ~2 400 kWh/an). \
La composante énergie est le prix spot day-ahead (ENTSO-E), factuel et horaire ; les composantes \
réglementaires sont des valeurs de référence versionnées (millésime 2026) sourcées : accise \
30,85 €/MWh (CRE délib. 2026-06 + BOFiP), TVA 20 % (BOFiP), commercialisation 18,11 €/MWh HT (CRE \
délib. 2026-06), acheminement dérivé du TURPE 7 (CRE délib. 2025-78). L'acheminement en €/MWh est \
une conversion dépendant du profil et de la ventilation horaire (≈ 78 €/MWh, plage 53–116). \
carbon-fr ne formule aucun jugement sur ces composantes.";

/// Réponse de `GET /v1/price` — décomposition complète du prix payé (ADR-0023).
///
/// On n'expose pas deux chiffres en regard : la **chaîne complète** est
/// décomposée, chaque composante sourcée. Le « prix réel de l'énergie » est la
/// composante `energie` (spot day-ahead).
#[derive(Serialize, ToSchema)]
pub(crate) struct PriceResponse {
    region: String,
    /// Horodatage (RFC 3339, UTC), aligné sur `/v1/intensity/now`.
    timestamp: String,
    /// Millésime de la construction réglementaire (TRV) appliquée.
    vintage: &'static str,
    /// Unité des montants `*_eur_mwh` (`EUR/MWh`). Les champs `*_eur_kwh` sont,
    /// eux, en €/kWh (confort d'usage).
    unit: &'static str,
    currency: &'static str,
    /// Total payé toutes taxes comprises (€/MWh).
    total_eur_mwh: f64,
    /// Total payé toutes taxes comprises (€/kWh) — confort d'usage.
    total_eur_kwh: f64,
    /// Décomposition par composante (chacune sourcée).
    components: Vec<PriceComponentBody>,
    /// Contexte explicatif (sans verdict) : mix + technologie marginale estimée.
    context: PriceContextBody,
    disclaimer: &'static str,
}

/// Une composante du prix (énergie, acheminement, accise, commercialisation, TVA).
#[derive(Serialize, ToSchema)]
struct PriceComponentBody {
    /// Identifiant stable (`energie`, `acheminement`, `accise`, `commercialisation`, `tva`).
    kind: &'static str,
    label: &'static str,
    amount_eur_mwh: f64,
    /// Source / fondement réglementaire de la composante.
    source: &'static str,
}

/// Contexte explicatif du prix (ADR-0023 §4).
#[derive(Serialize, ToSchema)]
struct PriceContextBody {
    /// Mix de production par filière au même instant (parts de production).
    mix: Vec<MixShareBody>,
    /// Technologie marginale **estimée** (ordre de mérite), ou `null`.
    marginal_technology: Option<MarginalTechnologyBody>,
}

#[derive(Serialize, ToSchema)]
struct MixShareBody {
    filiere: &'static str,
    label: &'static str,
    /// Part dans la production domestique, dans `[0, 1]`.
    share: f64,
    output_mw: f64,
}

#[derive(Serialize, ToSchema)]
struct MarginalTechnologyBody {
    filiere: &'static str,
    label: &'static str,
    /// Toujours `true` : valeur estimée par ordre de mérite, jamais mesurée.
    estimated: bool,
    /// Méthode d'estimation (transparence).
    method: &'static str,
}

impl PriceResponse {
    pub(crate) fn from_breakdown(b: &PriceBreakdown) -> Result<Self, time::error::Format> {
        let components = b
            .components
            .iter()
            .map(|c| PriceComponentBody {
                kind: c.kind.slug(),
                label: c.kind.label(),
                amount_eur_mwh: c.amount_eur_mwh,
                source: c.kind.source(),
            })
            .collect();
        let mix = b
            .context
            .shares
            .iter()
            .map(|s| MixShareBody {
                filiere: s.filiere.slug(),
                label: s.filiere.label(),
                share: s.share,
                output_mw: s.output_mw,
            })
            .collect();
        let marginal_technology = b.context.marginal.map(|m| MarginalTechnologyBody {
            filiere: m.filiere.slug(),
            label: m.filiere.label(),
            estimated: m.estimated,
            method: "ordre de mérite (coût marginal court terme) sur le mix en production",
        });
        let total = b.total_eur_mwh();
        Ok(Self {
            region: b.region.slug().to_string(),
            timestamp: to_rfc3339(b.at)?,
            vintage: b.vintage,
            unit: "EUR/MWh",
            currency: "EUR",
            total_eur_mwh: total,
            total_eur_kwh: total / 1000.0,
            components,
            context: PriceContextBody {
                mix,
                marginal_technology,
            },
            disclaimer: PRICE_DISCLAIMER,
        })
    }
}

/// Réponse de `GET /v1/price/date` — série de décompositions sur un intervalle.
///
/// Points **compacts** (horodatage + énergie + total) pour ne pas gonfler une
/// série dense ; la décomposition complète est servie par `/v1/price`. Alimente
/// la primitive « cheapest + greenest window » (ADR-0023).
#[derive(Serialize, ToSchema)]
pub(crate) struct PriceHistoryResponse {
    from: String,
    to: String,
    count: usize,
    unit: &'static str,
    currency: &'static str,
    points: Vec<PricePointBody>,
}

#[derive(Serialize, ToSchema)]
struct PricePointBody {
    timestamp: String,
    /// Composante énergie (spot day-ahead), la seule qui varie heure par heure.
    energie_eur_mwh: f64,
    /// Total payé toutes taxes comprises (€/MWh).
    total_eur_mwh: f64,
}

impl PriceHistoryResponse {
    pub(crate) fn new(
        from: OffsetDateTime,
        to: OffsetDateTime,
        breakdowns: &[PriceBreakdown],
    ) -> Result<Self, time::error::Format> {
        let mut points = Vec::with_capacity(breakdowns.len());
        for b in breakdowns {
            let energie = b
                .components
                .iter()
                .find(|c| c.kind == carbonfr_core::domain::PriceComponentKind::Energie)
                .map(|c| c.amount_eur_mwh)
                .unwrap_or(0.0);
            points.push(PricePointBody {
                timestamp: to_rfc3339(b.at)?,
                energie_eur_mwh: energie,
                total_eur_mwh: b.total_eur_mwh(),
            });
        }
        Ok(Self {
            from: to_rfc3339(from)?,
            to: to_rfc3339(to)?,
            count: points.len(),
            unit: "EUR/MWh",
            currency: "EUR",
            points,
        })
    }
}

/// Réponse de `GET /v1/cost-reference` — couche comparative LCOE (ADR-0024).
///
/// **Estimation** systématiquement étiquetée, en **fourchette** par filière
/// (jamais un chiffre unique), **jamais** mise en différence avec le prix de
/// marché. La note `disclaimer` est obligatoire (ADR-0024 §3).
#[derive(Serialize, ToSchema)]
pub(crate) struct CostReferenceResponse {
    unit: &'static str,
    currency: &'static str,
    /// Statut systématique de la couche : `estimation` (ADR-0024 §4).
    kind: &'static str,
    /// Note explicative neutre obligatoire (LCOE ≠ coût marginal ≠ prix payé).
    disclaimer: &'static str,
    count: usize,
    entries: Vec<CostReferenceEntry>,
}

/// Une estimation LCOE (source × technologie × périmètre × millésime).
#[derive(Serialize, ToSchema)]
struct CostReferenceEntry {
    technology: &'static str,
    technology_label: &'static str,
    source: &'static str,
    source_label: &'static str,
    source_attribution: &'static str,
    perimeter: &'static str,
    /// Libellé explicitant ce que le périmètre inclut/exclut (non comparable
    /// pilotable/variable).
    perimeter_label: &'static str,
    /// Nature de la grandeur : `accounting-amortized` (coût comptable d'un parc
    /// amorti) vs `prospective-lcoe` (moyen neuf). Évite la fausse commensurabilité.
    basis: &'static str,
    basis_label: &'static str,
    /// Millésime (année du rapport source).
    vintage: u32,
    /// Statut : toujours `estimation` (ADR-0024 §4).
    kind: &'static str,
    /// Fourchette (dispersion **publiée par la source**) — jamais un point unique.
    range: LcoeRangeBody,
    hypotheses: CostAssumptionsBody,
}

#[derive(Serialize, ToSchema)]
struct LcoeRangeBody {
    min: f64,
    median: f64,
    max: f64,
    unit: &'static str,
}

#[derive(Serialize, ToSchema)]
struct CostAssumptionsBody {
    /// Taux d'actualisation (WACC), `null` si non publié.
    discount_rate: Option<f64>,
    /// Durée de vie retenue (années), `null` si non publié.
    lifetime_years: Option<u32>,
    /// Facteur de charge, `null` si non publié.
    load_factor: Option<f64>,
}

impl CostReferenceResponse {
    pub(crate) fn from_entries(entries: &[CostEstimate]) -> Self {
        let entries = entries
            .iter()
            .map(|e| CostReferenceEntry {
                technology: e.key.technology.slug(),
                technology_label: e.key.technology.label(),
                source: e.key.source.slug(),
                source_label: e.key.source.label(),
                source_attribution: e.key.source.attribution(),
                perimeter: e.key.perimeter.slug(),
                perimeter_label: e.key.perimeter.label(),
                basis: e.basis.slug(),
                basis_label: e.basis.label(),
                vintage: e.key.vintage,
                kind: "estimation",
                range: LcoeRangeBody {
                    min: e.range.min,
                    median: e.range.median,
                    max: e.range.max,
                    unit: "EUR/MWh",
                },
                hypotheses: CostAssumptionsBody {
                    discount_rate: e.assumptions.discount_rate,
                    lifetime_years: e.assumptions.lifetime_years,
                    load_factor: e.assumptions.load_factor,
                },
            })
            .collect::<Vec<_>>();
        Self {
            unit: "EUR/MWh",
            currency: "EUR",
            kind: "estimation",
            disclaimer: COST_REFERENCE_DISCLAIMER,
            count: entries.len(),
            entries,
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

/// Données d'un événement SSE `intensity` (`GET /v1/intensity/stream`, ADR-0014).
#[derive(Serialize, ToSchema)]
pub(crate) struct StreamEventBody {
    region: String,
    timestamp: String,
    intensity: f64,
    methodology: String,
    methodology_version: u32,
    unit: &'static str,
}

impl StreamEventBody {
    pub(crate) fn from_update(
        u: &carbonfr_core::domain::IntensityUpdate,
    ) -> Result<Self, time::error::Format> {
        Ok(Self {
            region: u.region.slug().to_string(),
            timestamp: to_rfc3339(u.at)?,
            intensity: u.intensity.value(),
            methodology: u.methodology.id.clone(),
            methodology_version: u.methodology.version,
            unit: "gCO2eq/kWh",
        })
    }
}

/// Corps de `POST /v1/webhooks` (ADR-0016).
#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateWebhookRequest {
    /// Slug de région à surveiller. National par défaut.
    pub region: Option<String>,
    /// Seuil d'intensité (gCO₂eq/kWh).
    pub threshold: f64,
    /// Sens du franchissement : `below` ou `above`.
    pub direction: String,
    /// URL HTTPS de rappel (validée anti-SSRF).
    pub callback_url: String,
}

/// Réponse de création : inclut le **secret** (affiché une seule fois).
#[derive(Serialize, ToSchema)]
pub(crate) struct CreatedWebhookResponse {
    id: String,
    /// Secret de signature HMAC — **à conserver, non ré-affiché**.
    secret: String,
    region: String,
    threshold: f64,
    direction: &'static str,
    callback_url: String,
}

impl CreatedWebhookResponse {
    pub(crate) fn from_subscription(s: &carbonfr_core::domain::Subscription) -> Self {
        Self {
            id: s.id.clone(),
            secret: s.secret.clone(),
            region: s.region.slug().to_string(),
            threshold: s.threshold,
            direction: s.direction.code(),
            callback_url: s.callback_url.clone(),
        }
    }
}

/// Résumé d'un abonnement (sans le secret).
#[derive(Serialize, ToSchema)]
pub(crate) struct WebhookSummary {
    id: String,
    region: String,
    threshold: f64,
    direction: &'static str,
    callback_url: String,
}

/// Réponse de `GET /v1/webhooks`.
#[derive(Serialize, ToSchema)]
pub(crate) struct WebhookListResponse {
    count: usize,
    webhooks: Vec<WebhookSummary>,
}

impl WebhookListResponse {
    pub(crate) fn new(subs: &[carbonfr_core::domain::Subscription]) -> Self {
        let webhooks = subs
            .iter()
            .map(|s| WebhookSummary {
                id: s.id.clone(),
                region: s.region.slug().to_string(),
                threshold: s.threshold,
                direction: s.direction.code(),
                callback_url: s.callback_url.clone(),
            })
            .collect::<Vec<_>>();
        Self {
            count: webhooks.len(),
            webhooks,
        }
    }
}
