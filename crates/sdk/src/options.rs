//! Paramètres optionnels des 25 méthodes REST de `crate::methods` (le
//! flux SSE a les siens, `crate::stream::IntensityStreamOptions`) — un struct
//! `#[derive(Default)]` à champs publics par méthode qui a au moins un
//! paramètre réellement optionnel, sur le patron déjà posé par
//! `IntensityStreamOptions`.
//!
//! **Paramètre requis en pratique malgré `required: false` dans l'OpenAPI**
//! (`sdk-spec.json` `operations[].notes`, comportement runtime réel des
//! handlers + parité avec le SDK TS) : `threshold` de `below` et `count` de
//! `schedule_slots` sont des **arguments directs** de la méthode, pas des
//! champs `Option<…>` d'un struct d'options — ADR-0031 décision 5 (« suivre
//! le SDK TS ») et `CONTRIBUTING.md` (pas de valeur `None` qui produirait un
//! 400 systématique côté serveur). Même chose pour tout paramètre marqué
//! `required: true` sans ambiguïté (`from`/`to` des endpoints `*_date`,
//! d'intervalle ou d'historique).

use time::OffsetDateTime;

use crate::enums::{EligibilityFramework, Estimator, Interval, Methodology};
use crate::region::Region;

/// `GET /v1/intensity/now`.
#[derive(Debug, Clone, Default)]
pub struct IntensityNowOptions {
    /// Slug de région. National par défaut.
    pub region: Option<Region>,
    /// Méthodologie. Défaut `rte-direct`.
    pub methodology: Option<Methodology>,
    /// Version de la méthode (`acv-ademe` : `1` = production (défaut),
    /// `2` = consommation, national).
    pub version: Option<u32>,
}

/// `GET /v1/intensity/date` — `from`/`to` (requis) sont des arguments directs
/// de [`crate::CarbonFr::intensity_date`].
#[derive(Debug, Clone, Default)]
pub struct IntensityDateOptions {
    pub region: Option<Region>,
    /// Défaut `rte-direct`.
    pub methodology: Option<Methodology>,
    pub version: Option<u32>,
}

/// `GET /v1/intensity/stats` — `from`/`to` (requis) sont des arguments
/// directs de [`crate::CarbonFr::intensity_stats`].
#[derive(Debug, Clone, Default)]
pub struct IntensityStatsOptions {
    pub region: Option<Region>,
    /// Pas d'agrégation de la série renvoyée en plus du résumé.
    pub interval: Option<Interval>,
    pub methodology: Option<Methodology>,
    pub version: Option<u32>,
}

/// `GET /v1/intensity/below` — `threshold` (requis en pratique, cf. le
/// commentaire de module) est un argument direct de
/// [`crate::CarbonFr::below`].
#[derive(Debug, Clone, Default)]
pub struct BelowOptions {
    pub region: Option<Region>,
    pub methodology: Option<Methodology>,
    pub version: Option<u32>,
    /// Début de l'horizon. Défaut : maintenant.
    pub from: Option<OffsetDateTime>,
    /// Profondeur de l'horizon en heures (1..=72). Défaut 24.
    pub horizon_hours: Option<u32>,
    /// Défaut `central`.
    pub estimator: Option<Estimator>,
}

/// `GET /v1/intensity/forecast`.
#[derive(Debug, Clone, Default)]
pub struct ForecastOptions {
    pub region: Option<Region>,
    /// Méthodologie à prévoir. Défaut `rte-direct`.
    pub methodology: Option<Methodology>,
    pub from: Option<OffsetDateTime>,
    pub horizon_hours: Option<u32>,
    pub version: Option<u32>,
}

/// `GET /v1/intensity/greenest-window`.
#[derive(Debug, Clone, Default)]
pub struct GreenestWindowOptions {
    pub region: Option<Region>,
    pub methodology: Option<Methodology>,
    pub version: Option<u32>,
    pub from: Option<OffsetDateTime>,
    pub horizon_hours: Option<u32>,
    /// Durée du créneau recherché, en minutes. Défaut 60.
    pub window_minutes: Option<u32>,
    pub estimator: Option<Estimator>,
    /// Cadre d'éligibilité électrolyseur. Absent ⇒ réponse historique
    /// inchangée (`eligibility: None` dans la réponse) — axe **orthogonal**
    /// à `methodology`.
    pub eligibility: Option<EligibilityFramework>,
    /// Version de ruleset (ex. `rfnbo:2023-1184`). Défaut : ruleset servi du
    /// cadre. Sans effet si `eligibility` est `None`.
    pub eligibility_version: Option<String>,
    /// Override du seuil de surplus prix (€/MWh, ≥ 0).
    pub surplus_price_eur_mwh: Option<f64>,
    /// Override du seuil d'intensité bas-carbone (gCO₂eq/kWh, `]0, 1000]`).
    pub low_carbon_threshold_g_per_kwh: Option<f64>,
    /// Override de la consommation électrolyseur (kWh/kgH₂, `[10, 200]`) —
    /// recale le seuil bas-carbone dérivé (sauf si un seuil explicite est
    /// aussi fourni).
    pub electrolyzer_kwh_per_kg: Option<f64>,
}

/// `GET /v1/mix`.
#[derive(Debug, Clone, Default)]
pub struct MixOptions {
    pub region: Option<Region>,
    pub methodology: Option<Methodology>,
    pub version: Option<u32>,
}

/// `GET /v1/schedule`.
#[derive(Debug, Clone, Default)]
pub struct ScheduleOptions {
    pub region: Option<Region>,
    pub methodology: Option<Methodology>,
    pub version: Option<u32>,
    pub from: Option<OffsetDateTime>,
    pub horizon_hours: Option<u32>,
    /// Durée du job à planifier, en minutes. Défaut 60.
    pub duration_minutes: Option<u32>,
    /// Échéance de livraison. Sans elle, tout l'horizon est considéré.
    pub deadline: Option<OffsetDateTime>,
    /// Énergie du job (kWh) : si fournie, l'économie absolue (gCO₂eq) est
    /// calculée (`SavingsBody::absolute_saved_g`).
    pub energy_kwh: Option<f64>,
    pub estimator: Option<Estimator>,
}

/// `GET /v1/schedule/slots` — `count` (requis en pratique, cf. le commentaire
/// de module) est un argument direct de
/// [`crate::CarbonFr::schedule_slots`].
#[derive(Debug, Clone, Default)]
pub struct ScheduleSlotsOptions {
    pub region: Option<Region>,
    pub methodology: Option<Methodology>,
    pub version: Option<u32>,
    pub from: Option<OffsetDateTime>,
    pub horizon_hours: Option<u32>,
    pub estimator: Option<Estimator>,
}

/// `GET /v1/factors`.
#[derive(Debug, Clone, Default)]
pub struct FactorsOptions {
    /// Méthodologie (seule `acv-ademe` publie une table de facteurs
    /// aujourd'hui). Défaut `acv-ademe`.
    pub methodology: Option<Methodology>,
    /// Version (défaut : dernière version de la méthode).
    pub version: Option<u32>,
}

/// `GET /v1/price` — `region`, si fourni, doit être national (toute autre
/// valeur renvoie 400, le TRV est national) : documenté ici comme côté SDK
/// TS, pas imposé par un type dédié (le serveur reste la source de vérité).
#[derive(Debug, Clone, Default)]
pub struct PriceOptions {
    pub region: Option<Region>,
}

/// `GET /v1/price/date` — `from`/`to` (requis) sont des arguments directs de
/// [`crate::CarbonFr::price_history`] ; `region`, si fourni, doit être
/// national (même remarque que [`PriceOptions`]).
#[derive(Debug, Clone, Default)]
pub struct PriceHistoryOptions {
    pub region: Option<Region>,
}

/// `GET /v1/cost-reference` — filtres en `String`/`u32` bruts : les valeurs
/// possibles (`source`, `technology`, `perimeter`) ne sont pas dans le
/// périmètre des enums dupliqués par cette mission (`sdk-spec.json`
/// `enums_to_duplicate`).
#[derive(Debug, Clone, Default)]
pub struct CostReferenceOptions {
    /// Source (`ademe`, `cour-des-comptes`, `rte`, `irena`, `cre`…).
    pub source: Option<String>,
    /// Technologie (`nucleaire-existant`, `solaire-pv`, `eolien-terrestre`…).
    pub technology: Option<String>,
    /// Périmètre (`plateau`).
    pub perimeter: Option<String>,
    /// Millésime (année du rapport source).
    pub vintage: Option<u32>,
}
