//! Part renouvelable instantanée d'un mix de production.

use carbonfr_core::domain::GenerationMix;

/// Part renouvelable d'un mix dans `[0, 1]`, ou `None` si la production primaire
/// est nulle.
///
/// **Numérateur** = éolien + solaire + hydraulique + bioénergies (le nucléaire
/// est bas-carbone mais **non renouvelable** : exclu, cohérent avec le cadre
/// `rfnbo`).
///
/// **Dénominateur** = production primaire totale, suivant exactement la
/// convention privée `price::mix_shares` (carbon-fr `core`) : productions
/// négatives bornées à 0 ; **pompage et échanges exclus** (pas des productions
/// primaires) ; au régional, le fossile agrégé `thermique` remplace
/// gaz/charbon/fioul. Le test golden ancre la valeur pour détecter une dérive de
/// cette convention.
pub fn renewable_share(mix: &GenerationMix) -> Option<f64> {
    let renewable = mix.eolien.max(0.0)
        + mix.solaire.max(0.0)
        + mix.hydraulique.max(0.0)
        + mix.bioenergies.max(0.0);

    let mut total = mix.nucleaire.max(0.0)
        + mix.hydraulique.max(0.0)
        + mix.eolien.max(0.0)
        + mix.solaire.max(0.0)
        + mix.bioenergies.max(0.0);
    match mix.thermique {
        Some(thermique) => total += thermique.max(0.0),
        None => total += mix.gaz.max(0.0) + mix.charbon.max(0.0) + mix.fioul.max(0.0),
    }

    if total <= 0.0 {
        return None;
    }
    Some(renewable / total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mix national de référence (mêmes valeurs que le fixture `price.rs`).
    fn national() -> GenerationMix {
        GenerationMix {
            nucleaire: 38815.0,
            gaz: 666.0,
            charbon: 0.0,
            fioul: 34.0,
            hydraulique: 8893.0,
            eolien: 2555.0,
            solaire: 1050.0,
            bioenergies: 1006.0,
            pompage: -500.0,
            echanges: -3000.0,
            thermique: None,
        }
    }

    #[test]
    fn national_share_matches_sum_of_renewable_mix_shares() {
        // renouvelable = 8893+2555+1050+1006 = 13504 ; total = 53019 → ≈0,2547.
        let s = renewable_share(&national()).unwrap();
        assert!((s - 0.254701).abs() < 1e-5, "part = {s}");
    }

    #[test]
    fn excludes_pompage_and_echanges() {
        // pompage négatif et échanges (import) ne doivent pas entrer au dénominateur.
        let mut mix = national();
        mix.pompage = -9999.0;
        mix.echanges = 9999.0;
        let s = renewable_share(&mix).unwrap();
        assert!((s - 0.254701).abs() < 1e-5);
    }

    #[test]
    fn clamps_negative_production_to_zero() {
        let mut mix = national();
        mix.fioul = -100.0; // production négative bornée à 0
        let s = renewable_share(&mix).unwrap();
        // dénominateur identique à charbon=fioul=0 → ≈0,254865
        assert!((s - 0.254865).abs() < 1e-5, "part = {s}");
    }

    #[test]
    fn regional_uses_thermique_aggregate() {
        let mix = GenerationMix {
            nucleaire: 0.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 100.0,
            eolien: 100.0,
            solaire: 0.0,
            bioenergies: 0.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: Some(200.0),
        };
        // renouvelable = 200 ; total = 200 + 200 (thermique) = 400 → 0,5
        assert_eq!(renewable_share(&mix), Some(0.5));
    }

    #[test]
    fn none_on_zero_production() {
        let mix = GenerationMix {
            nucleaire: 0.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 0.0,
            eolien: 0.0,
            solaire: 0.0,
            bioenergies: 0.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        };
        assert_eq!(renewable_share(&mix), None);
    }

    #[test]
    fn one_when_only_renewables() {
        let mix = GenerationMix {
            nucleaire: 0.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 50.0,
            eolien: 50.0,
            solaire: 50.0,
            bioenergies: 50.0,
            pompage: 100.0,
            echanges: 100.0,
            thermique: None,
        };
        assert_eq!(renewable_share(&mix), Some(1.0));
    }
}
