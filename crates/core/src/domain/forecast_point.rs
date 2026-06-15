//! Contrat de prévision (ADR-0011) : [`ForecastPoint`] et [`ModelVersion`].
//!
//! Un point de **prévision** n'est **pas** une [`Measurement`](super::Measurement) :
//! - il n'a **pas de millésime** (`vintage`) — c'est une prédiction, pas une
//!   observation révisée ;
//! - il porte une **incertitude** de premier ordre (intervalle `lower`/`upper`),
//!   propriété qu'un `Measurement` ne peut pas exprimer ;
//! - il dit **quel modèle** l'a produit ([`ModelVersion`]), pour la
//!   reproductibilité et l'honnêteté.
//!
//! Réutiliser `Measurement` détournait `vintage` (mis à `Tr` faute de mieux) et
//! interdisait les intervalles : ce type dédié corrige le contrat (ADR-0011).

use time::OffsetDateTime;

use crate::domain::{CarbonIntensity, Methodology, Region};

/// Identité **versionnée** d'un modèle de prévision (ex. `climatology@1`), sur
/// le modèle de [`Methodology`] : une prévision est produite *par* un modèle
/// précis. Tout changement de modèle = bump de version, exposé (ADR-0011).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelVersion {
    pub id: String,
    pub version: u32,
}

impl ModelVersion {
    pub fn new(id: impl Into<String>, version: u32) -> Self {
        Self {
            id: id.into(),
            version,
        }
    }
}

impl std::fmt::Display for ModelVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.id, self.version)
    }
}

/// Un point de prévision : estimation centrale **encadrée** par un intervalle
/// d'incertitude, pour une cible `(region, methodology)` et un `model` donnés.
///
/// Invariant garanti à la construction : `lower ≤ expected ≤ upper`.
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastPoint {
    /// Début du pas (quart d'heure).
    pub at: OffsetDateTime,
    pub region: Region,
    /// Estimation centrale.
    pub expected: CarbonIntensity,
    /// Borne basse de l'intervalle.
    pub lower: CarbonIntensity,
    /// Borne haute de l'intervalle.
    pub upper: CarbonIntensity,
    /// Méthodologie carbone prévue (une prévision est faite *pour* une méthode).
    pub methodology: Methodology,
    /// Modèle qui l'a produite.
    pub model: ModelVersion,
}

impl ForecastPoint {
    /// Construit un point en **garantissant l'invariant** `lower ≤ expected ≤
    /// upper` : une borne incohérente est resserrée sur `expected` (plutôt que
    /// d'échouer — une prévision dérivée ne doit pas casser sur un arrondi).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        at: OffsetDateTime,
        region: Region,
        expected: CarbonIntensity,
        lower: CarbonIntensity,
        upper: CarbonIntensity,
        methodology: Methodology,
        model: ModelVersion,
    ) -> Self {
        let lower = if lower.value() <= expected.value() {
            lower
        } else {
            expected
        };
        let upper = if upper.value() >= expected.value() {
            upper
        } else {
            expected
        };
        Self {
            at,
            region,
            expected,
            lower,
            upper,
            methodology,
            model,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ci(g: f64) -> CarbonIntensity {
        CarbonIntensity::new(g).unwrap()
    }

    #[test]
    fn model_version_displays_id_at_version() {
        assert_eq!(
            ModelVersion::new("climatology", 1).to_string(),
            "climatology@1"
        );
    }

    #[test]
    fn construction_keeps_consistent_band() {
        let p = ForecastPoint::new(
            OffsetDateTime::UNIX_EPOCH,
            Region::National,
            ci(50.0),
            ci(40.0),
            ci(70.0),
            Methodology::rte_direct(),
            ModelVersion::new("climatology", 1),
        );
        assert_eq!(p.lower.value(), 40.0);
        assert_eq!(p.expected.value(), 50.0);
        assert_eq!(p.upper.value(), 70.0);
    }

    #[test]
    fn inconsistent_bounds_are_snapped_to_expected() {
        // lower > expected et upper < expected → resserrés sur expected.
        let p = ForecastPoint::new(
            OffsetDateTime::UNIX_EPOCH,
            Region::National,
            ci(50.0),
            ci(60.0), // lower au-dessus de expected
            ci(45.0), // upper en dessous de expected
            Methodology::rte_direct(),
            ModelVersion::new("climatology", 1),
        );
        assert_eq!(p.lower.value(), 50.0);
        assert_eq!(p.upper.value(), 50.0);
        assert!(p.lower.value() <= p.expected.value());
        assert!(p.expected.value() <= p.upper.value());
    }
}
