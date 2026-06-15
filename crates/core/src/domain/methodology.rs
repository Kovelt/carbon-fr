//! Méthodologie carbone (ADR-0005) et millésime des données (ADR-0006).

use std::fmt;

/// Méthodologie de calcul de l'intensité carbone.
///
/// Attribut **versionné de premier ordre** (ADR-0005) : une valeur n'a de sens
/// que rapportée à la méthode qui l'a produite. Plusieurs méthodes peuvent
/// coexister (`rte-direct`, puis `acv-ademe`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Methodology {
    pub id: String,
    pub version: u32,
}

impl Methodology {
    pub fn new(id: impl Into<String>, version: u32) -> Self {
        Self {
            id: id.into(),
            version,
        }
    }

    /// Méthode par défaut du MVP : reprise de l'estimation RTE (ADR-0005).
    pub fn rte_direct() -> Self {
        Self::new("rte-direct", 1)
    }

    /// Méthode cycle de vie ADEME, basée **production** (ADR-0008) — `acv-ademe@1`.
    pub fn acv_ademe() -> Self {
        Self::new("acv-ademe", 1)
    }

    /// Méthode cycle de vie ADEME, basée **consommation** (imports valorisés à
    /// l'intensité du voisin + pertes T&D, ADR-0010) — `acv-ademe@2`.
    ///
    /// Même identifiant que `@1`, **version distincte** : les deux coexistent
    /// sans collision (la version fait partie de la clé d'unicité, ADR-0006) et
    /// `@1` reste interrogeable (gouvernance ADR-0005 : pas de modification
    /// silencieuse d'une méthode publiée).
    pub fn acv_ademe_consumption() -> Self {
        Self::new("acv-ademe", 2)
    }
}

impl fmt::Display for Methodology {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@v{}", self.id, self.version)
    }
}

/// Millésime d'une mesure (ADR-0006).
///
/// RTE révise ses données : temps réel → consolidé → définitif. L'ordre des
/// variantes encode la **qualité croissante** (`Tr < Consolidated <
/// Definitive`), exploité par l'upsert conditionnel de l'ingestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Vintage {
    Tr,
    Consolidated,
    Definitive,
}

impl Vintage {
    pub fn code(self) -> &'static str {
        match self {
            Vintage::Tr => "tr",
            Vintage::Consolidated => "consolidated",
            Vintage::Definitive => "definitive",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vintage_quality_ordering() {
        assert!(Vintage::Definitive > Vintage::Consolidated);
        assert!(Vintage::Consolidated > Vintage::Tr);
    }
}
