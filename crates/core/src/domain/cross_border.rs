//! Flux transfrontaliers signés par voisin (ADR-0010).
//!
//! Value object porté **à côté** du mix pour le chemin `acv-ademe`
//! *consumption-based* : la méthode consommation valorise les imports à
//! l'intensité du pays d'origine, ce que le solde net `GenerationMix::echanges`
//! (un seul scalaire) ne permet pas. Aucune IO : la donnée est remplie par un
//! adapter (ENTSO-E), le domaine ne fait que la consommer.

use time::OffsetDateTime;

use crate::domain::CarbonIntensity;

/// Voisin électrique de la France métropolitaine (zone d'ajustement ENTSO-E
/// adjacente à RTE). Les interconnexions Manche (IFA/IFA2/ElecLink) sont
/// agrégées sous `GreatBritain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Neighbor {
    Belgium,
    Germany,
    Spain,
    Italy,
    Switzerland,
    GreatBritain,
}

impl Neighbor {
    /// Les six frontières électriques de la France métropolitaine.
    pub const ALL: [Neighbor; 6] = [
        Neighbor::Belgium,
        Neighbor::Germany,
        Neighbor::Spain,
        Neighbor::Italy,
        Neighbor::Switzerland,
        Neighbor::GreatBritain,
    ];

    /// Identifiant stable (clé d'API / de stockage). Suit les codes pays
    /// ISO-3166-1 alpha-2 (`de-lu` pour la zone Allemagne–Luxembourg ENTSO-E).
    pub fn slug(self) -> &'static str {
        match self {
            Neighbor::Belgium => "be",
            Neighbor::Germany => "de-lu",
            Neighbor::Spain => "es",
            Neighbor::Italy => "it-north",
            Neighbor::Switzerland => "ch",
            Neighbor::GreatBritain => "gb",
        }
    }
}

/// Flux net sur une frontière, **signé** (positif = import vers la France),
/// accompagné de l'intensité carbone (cycle de vie) du voisin au même instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossBorderFlow {
    pub neighbor: Neighbor,
    /// Puissance échangée (MW), positive = import vers la France.
    pub flow_mw: f64,
    /// Intensité carbone du voisin (gCO₂eq/kWh) — valorise l'import.
    pub neighbor_intensity: CarbonIntensity,
}

/// Contexte d'import au pas de la mesure : l'ensemble des flux frontaliers.
///
/// Aligné au pas quart d'heure du mix (ADR-0010 §6). Une frontière absente vaut
/// flux nul (pas d'échange mesuré sur ce pas).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CrossBorderFlows {
    pub flows: Vec<CrossBorderFlow>,
}

impl CrossBorderFlows {
    pub fn new(flows: Vec<CrossBorderFlow>) -> Self {
        Self { flows }
    }

    /// Total des imports (MW), bornes positives des flux.
    pub fn imports_mw(&self) -> f64 {
        self.flows.iter().map(|f| f.flow_mw.max(0.0)).sum()
    }

    /// Total des exports (MW), bornes positives de l'opposé des flux.
    pub fn exports_mw(&self) -> f64 {
        self.flows.iter().map(|f| (-f.flow_mw).max(0.0)).sum()
    }

    /// Émissions importées (gCO₂eq·MW/kWh) : Σ import_n × intensité_n.
    pub fn imported_emissions(&self) -> f64 {
        self.flows
            .iter()
            .filter(|f| f.flow_mw > 0.0)
            .map(|f| f.flow_mw * f.neighbor_intensity.value())
            .sum()
    }
}

/// Contexte d'import **horodaté** : les flux frontaliers à un pas quart d'heure.
///
/// Unité d'échange du port [`CrossBorderSource`](crate::ports::CrossBorderSource)
/// et de son store : aligné au pas du mix (ADR-0010 §6) pour permettre le calcul
/// `acv-ademe@2` à la lecture.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossBorderSnapshot {
    pub at: OffsetDateTime,
    pub flows: CrossBorderFlows,
}
