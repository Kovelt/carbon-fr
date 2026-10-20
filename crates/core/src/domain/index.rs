//! Index qualitatif d'intensité carbone (ADR-0033) : la lecture à cinq bandes
//! d'une [`CarbonIntensity`], façon *very low → very high* de
//! carbonintensity.org.uk, sur un barème **absolu** à seuils **sourcés**.
//!
//! Le barème est une **constante de domaine versionnée** (`index@1`), comme les
//! facteurs ACV (ADR-0008) : immuable une fois publié, toute révision passe par
//! un `index@2` et un nouvel ADR (ADR-0005). Il ne dépend ni de la méthodologie
//! ni de la région — un seul barème pour `rte-direct` comme `acv-ademe`, national
//! comme régional — et il n'est **pas stocké** : il se dérive à la projection
//! (adapter HTTP), sur tout l'historique, sans migration.

use crate::domain::CarbonIntensity;

/// Index qualitatif d'intensité (ADR-0033), barème `index@1`.
///
/// Cinq bandes à bornes **inférieures incluses, supérieures exclues** (`[a, b[`),
/// en gCO₂eq/kWh :
///
/// | Bande | Intervalle | Ancrage du seuil haut |
/// |---|---|---|
/// | `VeryLow` | `< 20` | mix nucléaire / hydraulique / éolien (facteurs ACV ADEME 6 / 4 / 7,3 g) |
/// | `Low` | `[20, 64,8[` | 18 gCO₂eq/MJ × 3,6 : seuil RFNBO de la zone de dépôt (règl. délégué (UE) 2023/1184, art. 4 §2) |
/// | `Moderate` | `[64,8, 100[` | 100 gCO₂e/kWh : seuil taxonomie européenne (règl. délégué (UE) 2021/2139, annexe I) |
/// | `High` | `[100, 200[` | 200 g ≈ la moitié du facteur gaz CCGT (406 g) |
/// | `VeryHigh` | `≥ 200` | mix majoritairement fossile |
///
/// `#[non_exhaustive]` : une bande ajoutée par un futur `index@2` ne cassera pas
/// les `match` des consommateurs (ADR-0030 §3).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntensityIndex {
    VeryLow,
    Low,
    Moderate,
    High,
    VeryHigh,
}

impl IntensityIndex {
    /// Version du barème (`index@1`).
    pub const VERSION: u32 = 1;

    /// Bornes supérieures **exclues** de `VeryLow`, `Low`, `Moderate` et `High`
    /// (gCO₂eq/kWh), dans l'ordre croissant : `20 · 64,8 · 100 · 200`. Tout
    /// changement = `index@2` + nouvel ADR.
    pub const THRESHOLDS_V1: [f64; 4] = [20.0, 64.8, 100.0, 200.0];

    /// Les cinq bandes, de la plus basse à la plus haute.
    pub const ALL: [IntensityIndex; 5] = [
        IntensityIndex::VeryLow,
        IntensityIndex::Low,
        IntensityIndex::Moderate,
        IntensityIndex::High,
        IntensityIndex::VeryHigh,
    ];

    /// Bande d'une intensité : la première borne strictement supérieure à la
    /// valeur la désigne (`[a, b[` — une valeur égale à une borne tombe dans la
    /// bande **supérieure**).
    pub fn of(intensity: CarbonIntensity) -> Self {
        let value = intensity.value();
        let rank = Self::THRESHOLDS_V1
            .iter()
            .take_while(|&&threshold| value >= threshold)
            .count();
        Self::ALL[rank]
    }

    /// Identifiant stable, en kebab-case comme tous les slugs de l'API
    /// (`very-low`, `low`, `moderate`, `high`, `very-high`).
    pub fn slug(self) -> &'static str {
        match self {
            IntensityIndex::VeryLow => "very-low",
            IntensityIndex::Low => "low",
            IntensityIndex::Moderate => "moderate",
            IntensityIndex::High => "high",
            IntensityIndex::VeryHigh => "very-high",
        }
    }

    /// Bornes de la bande, `(min incluse, max exclue)` en gCO₂eq/kWh ; `None`
    /// aux extrémités (pas de minimum pour `VeryLow`, pas de maximum pour
    /// `VeryHigh`).
    pub fn bounds(self) -> (Option<f64>, Option<f64>) {
        let rank = Self::ALL
            .iter()
            .position(|&band| band == self)
            .unwrap_or(Self::ALL.len() - 1);
        let min = rank.checked_sub(1).map(|i| Self::THRESHOLDS_V1[i]);
        let max = Self::THRESHOLDS_V1.get(rank).copied();
        (min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_of(g: f64) -> IntensityIndex {
        IntensityIndex::of(CarbonIntensity::new(g).unwrap())
    }

    #[test]
    fn bands_are_closed_below_and_open_above() {
        // Une valeur égale à une borne tombe dans la bande supérieure.
        assert_eq!(index_of(0.0), IntensityIndex::VeryLow);
        assert_eq!(index_of(19.999), IntensityIndex::VeryLow);
        assert_eq!(index_of(20.0), IntensityIndex::Low);
        assert_eq!(index_of(64.79), IntensityIndex::Low);
        assert_eq!(index_of(64.8), IntensityIndex::Moderate);
        assert_eq!(index_of(99.99), IntensityIndex::Moderate);
        assert_eq!(index_of(100.0), IntensityIndex::High);
        assert_eq!(index_of(199.9), IntensityIndex::High);
        assert_eq!(index_of(200.0), IntensityIndex::VeryHigh);
        assert_eq!(index_of(1_038.0), IntensityIndex::VeryHigh);
        assert_eq!(index_of(f64::MAX), IntensityIndex::VeryHigh);
    }

    #[test]
    fn thresholds_are_the_adr_0033_ones_and_increasing() {
        assert_eq!(IntensityIndex::THRESHOLDS_V1, [20.0, 64.8, 100.0, 200.0]);
        assert!(
            IntensityIndex::THRESHOLDS_V1
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        assert_eq!(IntensityIndex::VERSION, 1);
        // 18 gCO₂eq/MJ (RFNBO) × 3,6 MJ/kWh = 64,8 g/kWh.
        assert!((18.0 * 3.6 - IntensityIndex::THRESHOLDS_V1[1]).abs() < 1e-9);
    }

    #[test]
    fn slugs_are_kebab_case_and_unique() {
        let slugs: Vec<&str> = IntensityIndex::ALL.iter().map(|b| b.slug()).collect();
        assert_eq!(slugs, ["very-low", "low", "moderate", "high", "very-high"]);
        for slug in slugs {
            assert!(
                slug.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{slug}"
            );
        }
    }

    #[test]
    fn bounds_follow_the_thresholds() {
        assert_eq!(IntensityIndex::VeryLow.bounds(), (None, Some(20.0)));
        assert_eq!(IntensityIndex::Low.bounds(), (Some(20.0), Some(64.8)));
        assert_eq!(IntensityIndex::Moderate.bounds(), (Some(64.8), Some(100.0)));
        assert_eq!(IntensityIndex::High.bounds(), (Some(100.0), Some(200.0)));
        assert_eq!(IntensityIndex::VeryHigh.bounds(), (Some(200.0), None));
        // Les bandes se suivent sans trou ni recouvrement.
        for pair in IntensityIndex::ALL.windows(2) {
            assert_eq!(pair[0].bounds().1, pair[1].bounds().0);
        }
    }

    #[test]
    fn bands_are_ordered() {
        assert!(IntensityIndex::VeryLow < IntensityIndex::Low);
        assert!(IntensityIndex::High < IntensityIndex::VeryHigh);
        assert!(IntensityIndex::ALL.windows(2).all(|p| p[0] < p[1]));
    }
}
