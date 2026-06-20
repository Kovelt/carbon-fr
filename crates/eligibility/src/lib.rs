//! # carbonfr-eligibility
//!
//! Couche A « électrolyseur » de carbon-fr (ADR-0025, méthodologie ADR-0026) :
//! évaluation d'**éligibilité au niveau réseau** par créneau, sous deux cadres
//! explicitement étiquetés et **neutres** — `rfnbo` (renouvelable) et
//! `low-carbon` (bas-carbone inclusif nucléaire/CCS).
//!
//! ## Neutralité (cardinal, ADR-0025)
//!
//! `carbon-fr` expose l'éligibilité **au regard de chaque cadre** ; il ne tranche
//! pas la « couleur » de l'hydrogène ni le débat renouvelable/nucléaire. La donnée
//! est ouverte et vérifiable ; les conclusions appartiennent à l'utilisateur.
//!
//! ## Périmètre
//!
//! Support à la **décision sur signaux de réseau**, **pas** une certification.
//! Hors périmètre (donnée au niveau du site absente) : gCO₂eq/kgH₂, certification,
//! et additionnalité contractuelle (PPA).
//!
//! ## Architecture
//!
//! Crate de **domaine pur** : aucune IO (pas de `serde`/`sqlx`/`axum`/`reqwest`),
//! `time` plutôt que `chrono`, dépend uniquement de `carbonfr-core`. L'évaluation
//! est **totale** (jamais de `Result` : l'indétermination est portée par
//! [`EligibilitySignal::Indeterminate`]).

mod evaluate;
mod ruleset;
mod share;
mod verdict;

pub use evaluate::{best_eligible, evaluate, evaluate_slot, rank_by_score};
pub use ruleset::{
    DEFAULT_ELECTROLYZER_KWH_PER_KG, EligibilityFramework, EligibilityRuleset,
    FOSSIL_COMPARATOR_G_PER_MJ, H2_LHV_MJ_PER_KG, LOW_CARBON_BUDGET_G_PER_KG,
    REQUIRED_GHG_REDUCTION, RulesetStatus, TemporalGranularity, low_carbon_intensity_threshold,
    resolve_ruleset, ruleset_catalog,
};
pub use share::renewable_share;
pub use verdict::{
    EligibilitySignal, EligibilityVerdict, FR_BIDDING_ZONE, Pillar, SlotInput, basis_of,
};
