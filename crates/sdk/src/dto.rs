//! DTO de réponse (et un DTO de requête, [`CreateWebhookRequest`]) du contrat
//! `/v1`, dupliqués depuis les schémas `components.schemas` de
//! `crates/adapter-http/tests/openapi.snapshot.json` (ADR-0031 décision 5) —
//! aucune dépendance à `carbonfr-core`/`carbonfr-adapter-http`.
//!
//! **Champs publics, aucun `#[non_exhaustive]`** (ADR-0030 §3 : « Les structs
//! à champs publics n'en reçoivent jamais [`#[non_exhaustive]`] […], y
//! compris pour le futur SDK Rust » — cette clause visait explicitement ces
//! types) : ce sont des types de données, pas des invariants à protéger par
//! construction, et doivent rester constructibles par littéral côté
//! appelant (tests, code applicatif). Conséquence assumée : ajouter un champ
//! public à l'une d'elles sera un bump *minor*, jamais un
//! `#[non_exhaustive]` posé après coup (casserait justement la construction
//! par littéral que cette doctrine protège).
//!
//! **Horodatages** : `time::OffsetDateTime` (RFC 3339, `time::serde::rfc3339`
//! / `rfc3339::option`) pour tout champ réellement RFC 3339 (`timestamp`,
//! `from`/`to`, `start`/`end`, `valid_at`/`run_at`, `at`, `disabled_at`).
//! **Restent `String`, délibérément** (pas des instants RFC 3339) :
//! `vintage` (`HistoryPoint`/`IntensityResponse` : étiquette de révision, ex.
//! `définitif` — distinct de `CostReferenceEntry::vintage`, un millésime
//! entier), `PriceResponse::vintage` (millésime réglementaire, ex. `2026-H2`,
//! pas une date), `RulesetInfo::hourly_switchover` (date **sans** heure, ex.
//! `2030-01-01`, confirmé par `tests/fixtures/eligibility_rulesets.json` —
//! `time::serde::rfc3339` échouerait dessus) et `VisitStatsResponse::since`
//! (`ISO YYYY-MM-DD`, même raison).
//!
//! **Champs `String` plutôt qu'un enum dupliqué** pour tout catalogue
//! **non** listé par `enums_to_duplicate` dans `sdk-spec.json` (`filiere`,
//! `technology`/`source`/`perimeter`/`basis` du LCOE, `pillar`/`basis` d'un
//! signal d'éligibilité…) : ADR-0030 §3 classe ces catalogues internes à
//! `core` comme `#[non_exhaustive]` (voués à grandir), mais la mission I6 ne
//! les a pas mis dans le périmètre des enums à dupliquer — les garder en
//! `String` évite d'inventer côté SDK une liste fermée que le serveur ne
//! promet pas.
//!
//! **Entiers** : `format: int32` → `u32`, `format: int64` → `u64` (compteurs
//! sur un historique long : `StatsBucket`/`StatsResponse::count`,
//! `VisitStatsResponse::total`/`unique`), pas de `format` déclaré → `u32`
//! (bornés en pratique : nombre d'entrées d'un catalogue, de créneaux…).
//! `MethodologyInfo::id` reste délibérément `String` (pas [`Methodology`]) :
//! c'est un catalogue de **découverte**, qui peut lister une méthode
//! `planned` non encore sélectionnable — le typer figerait la désérialisation
//! le jour où une 3ᵉ méthode y apparaît avant que `Methodology` ne la
//! connaisse.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::enums::{
    EligibilityFramework, EligibilitySignalProvenance, EligibilitySignalVerdict, Estimator,
    FlowDirection, Interval, Methodology, ThresholdDirection, WebhookStatus,
};
use crate::region::Region;

// --- Intensité (`/v1/intensity/*`) -----------------------------------------

/// Valeur d'intensité carbone (`IntensityResponse::intensity`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IntensityValue {
    pub value: f64,
    pub unit: String,
}

/// Réponse de `GET /v1/intensity/now`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct IntensityResponse {
    pub region: Region,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub intensity: IntensityValue,
    pub methodology: Methodology,
    pub methodology_version: u32,
    /// Étiquette de révision de la mesure (ADR-0006) — pas un horodatage,
    /// voir le commentaire de module.
    pub vintage: String,
}

/// Un point de `GET /v1/intensity/date` (`HistoryResponse::data`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HistoryPoint {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub intensity: f64,
    pub vintage: String,
}

/// Réponse de `GET /v1/intensity/date`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HistoryResponse {
    pub region: Region,
    #[serde(with = "time::serde::rfc3339")]
    pub from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub to: OffsetDateTime,
    pub unit: String,
    pub methodology: Methodology,
    pub count: u32,
    pub data: Vec<HistoryPoint>,
}

/// Un seau agrégé de `GET /v1/intensity/stats?interval=` (`StatsResponse::intervals`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct StatsBucket {
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    pub average: f64,
    pub min: f64,
    pub max: f64,
    pub count: u64,
}

/// Réponse de `GET /v1/intensity/stats`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct StatsResponse {
    pub region: Region,
    #[serde(with = "time::serde::rfc3339")]
    pub from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub to: OffsetDateTime,
    pub unit: String,
    pub methodology: Methodology,
    pub average: f64,
    pub min: f64,
    pub max: f64,
    pub count: u64,
    /// Pas d'agrégation demandé (`?interval=`), `None` si absent de la requête.
    pub interval: Option<Interval>,
    /// Série agrégée par seau, `None` si `interval` n'a pas été demandé.
    pub intervals: Option<Vec<StatsBucket>>,
}

/// Un créneau (`GET /v1/schedule/slots`, `GET /v1/intensity/below`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SlotBody {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub intensity: f64,
}

/// Réponse d'une liste de créneaux (`GET /v1/schedule/slots`,
/// `GET /v1/intensity/below`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SlotsResponse {
    pub region: Region,
    pub methodology: Methodology,
    /// Identité versionnée du modèle de prévision (ex. `climatology@1`).
    pub model: String,
    pub estimator: Estimator,
    pub unit: String,
    pub count: u32,
    pub slots: Vec<SlotBody>,
}

// --- Prévision (`/v1/intensity/forecast`, `/v1/intensity/greenest-window`) -

/// Un point de `GET /v1/intensity/forecast` (`ForecastResponse::data`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ForecastPointBody {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub expected: f64,
    pub lower: f64,
    pub upper: f64,
}

/// Réponse de `GET /v1/intensity/forecast`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ForecastResponse {
    pub region: Region,
    pub methodology: Methodology,
    /// Identité versionnée du modèle de prévision (ex. `climatology@1`).
    pub model: String,
    #[serde(with = "time::serde::rfc3339")]
    pub from: OffsetDateTime,
    pub horizon_hours: u32,
    pub unit: String,
    pub count: u32,
    pub data: Vec<ForecastPointBody>,
}

/// Un signal d'un créneau (`EligibilitySlotBody::signals`, ADR-0025/0026).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EligibilitySignalBody {
    /// Pilier évalué (`renewable-share`, `surplus-price`,
    /// `low-carbon-intensity`) — `String`, pas dans le périmètre des enums
    /// dupliqués (voir le commentaire de module).
    pub pillar: String,
    pub verdict: EligibilitySignalVerdict,
    /// `regulatory`, `indicative-non-regulatory` ou `user-override` — `String`,
    /// même raison que `pillar`.
    pub basis: String,
    pub provenance: Option<EligibilitySignalProvenance>,
    /// Pourquoi le pilier est `indeterminate` (`missing-data`,
    /// `beyond-calibrated-horizon`…), absent sinon.
    pub reason: Option<String>,
    pub threshold: Option<f64>,
    pub value: Option<f64>,
    pub value_lower: Option<f64>,
    pub value_upper: Option<f64>,
}

/// Verdict d'éligibilité d'un créneau (`EligibilityBody::slots`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EligibilitySlotBody {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub eligible: bool,
    pub intensity: f64,
    pub intensity_lower: f64,
    pub intensity_upper: f64,
    /// Score de classement **interne au cadre** — jamais comparable entre
    /// `framework`s (utiliser `intensity` pour ça).
    pub score: f64,
    pub signals: Vec<EligibilitySignalBody>,
}

/// Meilleur créneau éligible (`EligibilityBody::best_eligible`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EligibleSlotBody {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub intensity: f64,
    pub intensity_lower: f64,
    pub intensity_upper: f64,
    pub score: f64,
}

/// Overlay d'éligibilité électrolyseur (`GreenestWindowResponse::eligibility`,
/// présent seulement si `?eligibility=` a été fourni).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EligibilityBody {
    pub framework: EligibilityFramework,
    pub ruleset_version: String,
    pub ruleset_status: String,
    /// `true` si des overrides utilisateur (`surplus_price_eur_mwh`…) ont été
    /// appliqués au ruleset servi.
    pub overridden: bool,
    /// Zone de dépôt : toujours `FR` (jamais une sous-région).
    pub bidding_zone: String,
    pub disclaimer: String,
    pub window_eligible: bool,
    pub count_eligible: u32,
    pub count_indeterminate: u32,
    pub slots: Vec<EligibilitySlotBody>,
    pub best_eligible: Option<EligibleSlotBody>,
    /// Identité versionnée du modèle de part renouvelable prévue
    /// (`share-clim@1`), présente seulement si au moins un créneau est en
    /// provenance `forecast`.
    pub share_model: Option<String>,
}

/// Réponse de `GET /v1/intensity/greenest-window`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GreenestWindowResponse {
    pub region: Region,
    pub methodology: Methodology,
    /// Identité versionnée du modèle de prévision (ex. `climatology@1`).
    pub model: String,
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
    pub unit: String,
    pub average_intensity: f64,
    /// Overlay d'éligibilité électrolyseur (présent uniquement si
    /// `?eligibility=` a été fourni) — annotation additive, la fenêtre verte
    /// elle-même est inchangée.
    pub eligibility: Option<EligibilityBody>,
}

// --- Scheduling (`/v1/schedule`, `/v1/schedule/slots`) ---------------------

/// Économie carbone d'un créneau planifié vs « maintenant » (ADR-0014).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SavingsBody {
    pub now: f64,
    pub scheduled: f64,
    pub intensity_delta: f64,
    pub reduction_percent: f64,
    /// Économie absolue (gCO₂eq), seulement si `energy_kwh` a été fourni à la
    /// requête.
    pub absolute_saved_g: Option<f64>,
}

/// Réponse de `GET /v1/schedule`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ScheduleResponse {
    pub region: Region,
    pub methodology: Methodology,
    pub model: String,
    pub estimator: Estimator,
    pub unit: String,
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
    pub average_intensity: f64,
    pub savings: SavingsBody,
}

// --- Mix de production (`/v1/mix`) -----------------------------------------

/// Mix de production par filière (`MixResponse::mix`), en MW.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct MixBody {
    pub nucleaire: f64,
    pub gaz: f64,
    pub charbon: f64,
    pub fioul: f64,
    pub hydraulique: f64,
    pub eolien: f64,
    pub solaire: f64,
    pub bioenergies: f64,
    pub pompage: f64,
    pub echanges: f64,
    /// Thermique fossile agrégé — **mix régional uniquement**, `None` au
    /// national.
    pub thermique: Option<f64>,
}

/// Réponse de `GET /v1/mix`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MixResponse {
    pub region: Region,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub unit: String,
    pub mix: MixBody,
}

// --- Échanges transfrontaliers (`/v1/exchanges*`) ---------------------------

/// Une frontière (`ExchangesResponse::exchanges`) : flux net signé FR↔voisin
/// + intensité carbone du voisin.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ExchangeEntry {
    /// Code du voisin (`be`, `de-lu`, `es`, `it-north`, `ch`, `gb`).
    pub country: String,
    pub country_name: String,
    /// Flux net (MW) : `> 0` = la France **importe**, `< 0` = exporte.
    pub flow_mw: f64,
    pub direction: FlowDirection,
    pub intensity: IntensityValue,
}

/// Réponse de `GET /v1/exchanges`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ExchangesResponse {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub net_flow_mw: f64,
    pub direction: FlowDirection,
    pub imports_mw: f64,
    pub exports_mw: f64,
    pub exchanges: Vec<ExchangeEntry>,
}

/// Réponse de `GET /v1/exchanges/date`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ExchangesHistoryResponse {
    #[serde(with = "time::serde::rfc3339")]
    pub from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub to: OffsetDateTime,
    pub count: u32,
    pub snapshots: Vec<ExchangesResponse>,
}

// --- Météo (`/v1/weather*`) --------------------------------------------------

/// Un créneau météo (`WeatherHistoryResponse::points`), vent à 100 m +
/// irradiance, moyenne nationale.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WeatherPoint {
    #[serde(with = "time::serde::rfc3339")]
    pub valid_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub run_at: OffsetDateTime,
    pub wind_kmh: f64,
    pub irradiance_wm2: f64,
}

/// Réponse de `GET /v1/weather`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WeatherResponse {
    /// Attribution de la source (Open-Meteo, licence CC-BY 4.0).
    pub source: String,
    #[serde(with = "time::serde::rfc3339")]
    pub valid_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub run_at: OffsetDateTime,
    pub wind_kmh: f64,
    pub irradiance_wm2: f64,
}

/// Réponse de `GET /v1/weather/date`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WeatherHistoryResponse {
    pub source: String,
    #[serde(with = "time::serde::rfc3339")]
    pub from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub to: OffsetDateTime,
    pub count: u32,
    pub points: Vec<WeatherPoint>,
}

// --- Renouvelable estimé (`/v1/renewable`) ----------------------------------

/// Capacités effectives calibrées (`RenewableResponse::model`, transparence
/// du modèle).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct RenewableModelInfo {
    pub wind_capacity_mw: f64,
    pub solar_capacity_mw: f64,
}

/// Réponse de `GET /v1/renewable`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RenewableResponse {
    /// Attribution (météo Open-Meteo CC-BY 4.0 ; valeurs modélisées).
    pub source: String,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub wind_mw: f64,
    pub solar_mw: f64,
    pub wind_capacity_factor: f64,
    pub solar_capacity_factor: f64,
    pub model: RenewableModelInfo,
}

// --- Méthodologies et facteurs (`/v1/methodologies`, `/v1/factors`) --------

/// Une méthodologie du catalogue (`MethodologiesResponse::methodologies`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MethodologyInfo {
    /// Identifiant stable (`rte-direct`, `acv-ademe`…) — `String` et pas
    /// [`Methodology`] : catalogue de découverte, peut lister une méthode
    /// `planned` non encore sélectionnable (voir le commentaire de module).
    pub id: String,
    pub version: u32,
    pub basis: String,
    pub scope: String,
    /// `true` si servie par défaut quand `?methodology=` est absent.
    pub default: bool,
    /// `served` (interrogeable) ou `planned` (spécifiée, pas encore servie).
    pub status: String,
    pub adr: String,
    pub description: String,
}

/// Réponse de `GET /v1/methodologies`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MethodologiesResponse {
    pub methodologies: Vec<MethodologyInfo>,
}

/// Un facteur d'émission par filière (`FactorsResponse::factors`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FactorEntry {
    /// Filière (`nucleaire`, `gaz`…) — `String`, pas dans le périmètre des
    /// enums dupliqués.
    pub filiere: String,
    /// Facteur cycle de vie (gCO₂eq/kWh).
    pub factor: f64,
}

/// Réponse de `GET /v1/factors`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FactorsResponse {
    pub methodology: Methodology,
    pub methodology_version: u32,
    pub unit: String,
    pub source: String,
    /// Facteur de pertes T&D appliqué, `None` hors méthode consommation.
    pub td_loss_factor: Option<f64>,
    pub factors: Vec<FactorEntry>,
}

// --- Prix (`/v1/price*`, `/v1/cost-reference`) ------------------------------

/// Une composante du prix (`PriceResponse::components`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PriceComponentBody {
    /// Identifiant stable (`energie`, `acheminement`, `accise`,
    /// `commercialisation`, `tva`) — `String`, pas dans le périmètre des
    /// enums dupliqués.
    pub kind: String,
    pub label: String,
    pub amount_eur_mwh: f64,
    /// Source / fondement réglementaire de la composante.
    pub source: String,
}

/// Part d'une filière dans le mix (`PriceContextBody::mix`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PriceMixShareBody {
    pub filiere: String,
    pub label: String,
    /// Part dans la production domestique, dans `[0, 1]`.
    pub share: f64,
    pub output_mw: f64,
}

/// Technologie marginale **estimée** (`PriceContextBody::marginal_technology`,
/// ordre de mérite domestique — jamais mesurée, `estimated` vaut toujours
/// `true`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MarginalTechnologyBody {
    pub filiere: String,
    pub label: String,
    pub estimated: bool,
    /// Méthode d'estimation (transparence).
    pub method: String,
}

/// Contexte explicatif du prix (`PriceResponse::context`, ADR-0023 §4).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PriceContextBody {
    pub mix: Vec<PriceMixShareBody>,
    pub marginal_technology: Option<MarginalTechnologyBody>,
}

/// Réponse de `GET /v1/price`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PriceResponse {
    pub region: Region,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    /// Millésime de la construction réglementaire en vigueur à cet
    /// horodatage (ex. `2026-H2`) — `String`, pas un instant RFC 3339 (voir
    /// le commentaire de module).
    pub vintage: String,
    pub unit: String,
    pub currency: String,
    pub total_eur_mwh: f64,
    pub total_eur_kwh: f64,
    pub components: Vec<PriceComponentBody>,
    pub context: PriceContextBody,
    pub disclaimer: String,
}

/// Un point compact de `GET /v1/price/date` (`PriceHistoryResponse::points`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PricePointBody {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub energie_eur_mwh: f64,
    pub total_eur_mwh: f64,
}

/// Réponse de `GET /v1/price/date`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PriceHistoryResponse {
    #[serde(with = "time::serde::rfc3339")]
    pub from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub to: OffsetDateTime,
    pub count: u32,
    pub unit: String,
    pub currency: String,
    pub points: Vec<PricePointBody>,
}

/// Hypothèses d'une estimation LCOE (`CostReferenceEntry::hypotheses`),
/// `None` = « non publié par la source », jamais une dimension retirée.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct CostAssumptionsBody {
    pub discount_rate: Option<f64>,
    pub lifetime_years: Option<u32>,
    pub load_factor: Option<f64>,
}

/// Fourchette d'une estimation LCOE (`CostReferenceEntry::range`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LcoeRangeBody {
    pub min: f64,
    pub median: f64,
    pub max: f64,
    pub unit: String,
}

/// Une estimation LCOE (source × technologie × périmètre × millésime,
/// `CostReferenceResponse::entries`, ADR-0024). Champs catalogue
/// (`technology`/`source`/`perimeter`/`basis`) en `String` : hors du
/// périmètre des enums dupliqués par cette mission.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CostReferenceEntry {
    pub technology: String,
    pub technology_label: String,
    pub source: String,
    pub source_label: String,
    pub source_attribution: String,
    /// `france` ou `monde` — les valeurs IRENA sont **mondiales**.
    pub geography: String,
    pub perimeter: String,
    pub perimeter_label: String,
    /// `accounting-amortized` (coût comptable) vs `prospective-lcoe`
    /// (moyen neuf).
    pub basis: String,
    pub basis_label: String,
    /// Nombre de sources distinctes pour cette filière (≥ 2 = dispersion
    /// inter-sources).
    pub technology_source_count: u32,
    /// Millésime (année du rapport source) — entier, distinct de
    /// `HistoryPoint::vintage`/`IntensityResponse::vintage` (étiquette de
    /// révision, pas un millésime).
    pub vintage: u32,
    /// Statut : toujours `estimation` (ADR-0024 §4).
    pub kind: String,
    pub range: LcoeRangeBody,
    pub hypotheses: CostAssumptionsBody,
}

/// Réponse de `GET /v1/cost-reference` — couche comparative LCOE (ADR-0024).
/// **Estimation**, en fourchette, jamais mise en différence avec le prix de
/// marché (`disclaimer` obligatoire, ADR-0024 §3).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CostReferenceResponse {
    pub unit: String,
    pub currency: String,
    /// Statut systématique de la couche : `estimation`.
    pub kind: String,
    pub disclaimer: String,
    pub count: u32,
    pub entries: Vec<CostReferenceEntry>,
}

// --- Éligibilité (`/v1/eligibility/rulesets`) -------------------------------

/// Une entrée du catalogue (`RulesetsResponse::rulesets`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RulesetInfo {
    pub framework: EligibilityFramework,
    pub version: String,
    /// `served` (appliqué) ou `planned` (réservé, texte non adopté).
    pub status: String,
    pub adr: String,
    /// Granularité de corrélation temporelle (pilier `rfnbo`) ;
    /// `n/a (pilier rfnbo)` pour `low-carbon`.
    pub granularity: String,
    /// Seuil renouvelable de l'exception Article 4 (`rfnbo` uniquement).
    pub article4_renewable_threshold: Option<f64>,
    pub surplus_price_eur_mwh: Option<f64>,
    /// Date de bascule horaire (`rfnbo`), ex. `2030-01-01` — **date seule**,
    /// pas un instant RFC 3339 (voir le commentaire de module), `None` pour
    /// `low-carbon`.
    pub hourly_switchover: Option<String>,
    pub low_carbon_intensity_threshold_g_per_kwh: Option<f64>,
    pub low_carbon_intensity_is_indicative: bool,
    /// Consommation électrolyseur qui dérive le seuil (`low-carbon`
    /// uniquement).
    pub electrolyzer_kwh_per_kg: Option<f64>,
    pub legal_basis: String,
    pub description: String,
}

/// Réponse de `GET /v1/eligibility/rulesets`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RulesetsResponse {
    pub rulesets: Vec<RulesetInfo>,
    pub disclaimer: String,
}

// --- Statistiques de visite (`/v1/stats`, `POST /v1/stats/visit`) ----------

/// Réponse de `GET /v1/stats` et `POST /v1/stats/visit`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VisitStatsResponse {
    pub unique: u64,
    pub total: u64,
    /// Premier jour comptabilisé, `ISO YYYY-MM-DD` — **date seule**, pas un
    /// instant RFC 3339 (voir le commentaire de module), `None` si aucun.
    pub since: Option<String>,
}

// --- Webhooks (`/v1/webhooks*`) --------------------------------------------

/// Corps de `POST /v1/webhooks` (ADR-0016).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CreateWebhookRequest {
    /// Seuil d'intensité (gCO₂eq/kWh).
    pub threshold: f64,
    pub direction: ThresholdDirection,
    /// URL HTTPS de rappel (validée anti-SSRF côté serveur).
    pub callback_url: String,
    /// Région à surveiller. National par défaut si `None`.
    pub region: Option<Region>,
}

/// Réponse de création : inclut le **secret** (affiché une seule fois,
/// jamais ré-exposé par `GET /v1/webhooks`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreatedWebhookResponse {
    pub id: String,
    /// Secret de signature HMAC — à conserver, non ré-affiché.
    pub secret: String,
    pub region: Region,
    pub threshold: f64,
    pub direction: ThresholdDirection,
    pub callback_url: String,
}

/// Résumé d'un abonnement, sans le secret (`WebhookListResponse::webhooks`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WebhookSummary {
    pub id: String,
    pub region: Region,
    pub threshold: f64,
    pub direction: ThresholdDirection,
    pub callback_url: String,
    pub status: WebhookStatus,
    /// Instant de la désactivation automatique, `None` si actif.
    #[serde(with = "time::serde::rfc3339::option")]
    pub disabled_at: Option<OffsetDateTime>,
}

/// Réponse de `GET /v1/webhooks`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WebhookListResponse {
    pub count: u32,
    pub webhooks: Vec<WebhookSummary>,
}
