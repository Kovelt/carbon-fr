//! Facteurs d'émission cycle de vie et calcul de l'intensité `acv-ademe`
//! (ADR-0008). Table **versionnée** : c'est une constante de domaine, pas une
//! dépendance IO.

use crate::domain::{CarbonIntensity, GenerationMix, Measurement, Methodology};

/// Facteurs d'émission par filière (gCO₂eq/kWh), en analyse de cycle de vie.
///
/// Voir ADR-0008 pour les valeurs et leurs sources. Le pompage et les échanges
/// n'ont pas de facteur : ils ne sont pas des productions primaires.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmissionFactors {
    pub nucleaire: f64,
    pub gaz: f64,
    pub charbon: f64,
    pub fioul: f64,
    pub hydraulique: f64,
    pub eolien: f64,
    pub solaire: f64,
    pub bioenergies: f64,
}

impl EmissionFactors {
    /// Table `acv-ademe@1` — facteurs ADEME Base Carbone (ADR-0008).
    pub const fn acv_ademe_v1() -> Self {
        Self {
            nucleaire: 6.0,
            gaz: 406.0,
            charbon: 1038.0,
            fioul: 778.0,
            hydraulique: 4.0,
            eolien: 7.3,
            solaire: 55.0,
            bioenergies: 24.0,
        }
    }
}

/// Intensité carbone du mix de production, par la méthode `acv-ademe` (ADR-0008)
/// : moyenne des facteurs pondérée par la production. Pompage et échanges
/// exclus ; productions négatives bornées à 0. `None` si la production totale
/// est nulle (ou hors domaine).
pub fn acv_ademe_intensity(
    mix: &GenerationMix,
    factors: &EmissionFactors,
) -> Option<CarbonIntensity> {
    let terms = [
        (mix.nucleaire, factors.nucleaire),
        (mix.gaz, factors.gaz),
        (mix.charbon, factors.charbon),
        (mix.fioul, factors.fioul),
        (mix.hydraulique, factors.hydraulique),
        (mix.eolien, factors.eolien),
        (mix.solaire, factors.solaire),
        (mix.bioenergies, factors.bioenergies),
    ];

    let mut emissions = 0.0;
    let mut production = 0.0;
    for (output, factor) in terms {
        let output = output.max(0.0);
        emissions += output * factor;
        production += output;
    }

    if production <= 0.0 {
        return None;
    }
    CarbonIntensity::new(emissions / production)
}

/// Dérive la mesure `acv-ademe` à partir d'une mesure portant un mix de
/// production. `None` si la mesure n'a pas de mix ou si l'intensité est
/// indéfinie. Conserve horodatage, région et millésime.
pub fn derive_acv_ademe(measurement: &Measurement) -> Option<Measurement> {
    let mix = measurement.mix.as_ref()?;
    let intensity = acv_ademe_intensity(mix, &EmissionFactors::acv_ademe_v1())?;
    Some(Measurement {
        at: measurement.at,
        region: measurement.region,
        intensity,
        methodology: Methodology::acv_ademe(),
        vintage: measurement.vintage,
        mix: Some(*mix),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Region;
    use time::OffsetDateTime;

    fn national_mix() -> GenerationMix {
        // Mix national bas-carbone (capté sur éCO2mix).
        GenerationMix {
            nucleaire: 38815.0,
            gaz: 666.0,
            charbon: 0.0,
            fioul: 34.0,
            hydraulique: 8893.0,
            eolien: 2555.0,
            solaire: 1050.0,
            bioenergies: 1006.0,
            pompage: -76.0,
            echanges: -11574.0,
        }
    }

    #[test]
    fn acv_intensity_is_plausible_and_excludes_pompage_echanges() {
        let intensity =
            acv_ademe_intensity(&national_mix(), &EmissionFactors::acv_ademe_v1()).unwrap();
        // ≈ 12,56 gCO₂eq/kWh (cf. ADR-0008) : cycle de vie d'un mix très
        // nucléaire/hydraulique, sous le taux_co2 combustion directe du moment.
        assert!(
            (intensity.value() - 12.56).abs() < 0.1,
            "intensité = {}",
            intensity.value()
        );
    }

    #[test]
    fn empty_production_is_none() {
        let empty = GenerationMix {
            nucleaire: 0.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 0.0,
            eolien: 0.0,
            solaire: 0.0,
            bioenergies: 0.0,
            pompage: -10.0,
            echanges: 50.0,
        };
        assert!(acv_ademe_intensity(&empty, &EmissionFactors::acv_ademe_v1()).is_none());
    }

    #[test]
    fn derive_carries_metadata_and_sets_methodology() {
        let source = Measurement {
            at: OffsetDateTime::UNIX_EPOCH,
            region: Region::National,
            intensity: CarbonIntensity::new(15.0).unwrap(),
            methodology: Methodology::rte_direct(),
            vintage: crate::domain::Vintage::Consolidated,
            mix: Some(national_mix()),
        };
        let derived = derive_acv_ademe(&source).unwrap();
        assert_eq!(derived.methodology, Methodology::acv_ademe());
        assert_eq!(derived.at, source.at);
        assert_eq!(derived.region, Region::National);
        assert_eq!(derived.vintage, crate::domain::Vintage::Consolidated);
        assert!(derived.intensity.value() < source.intensity.value());

        // Sans mix → pas de dérivation.
        let no_mix = Measurement {
            mix: None,
            ..source
        };
        assert!(derive_acv_ademe(&no_mix).is_none());
    }
}
