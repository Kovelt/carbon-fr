//! Facteurs d'émission cycle de vie et calcul de l'intensité `acv-ademe`
//! (ADR-0008). Table **versionnée** : c'est une constante de domaine, pas une
//! dépendance IO.

use crate::domain::{CarbonIntensity, CrossBorderFlows, GenerationMix, Measurement, Methodology};

/// Facteur de pertes en transport & distribution (ADR-0010 §3), **versionné**.
///
/// Périmètre consommation : livrer 1 kWh au compteur impose d'en produire
/// davantage (pertes réseau). On applique un *uplift* `× (1 + facteur)` à
/// l'intensité réseau. **v1 = 0,072** (~7,2 %, ordre de grandeur des pertes du
/// système électrique français — transport RTE ~2 % + distribution Enedis
/// ~6 %). ⚠️ **valeur à sourcer précisément** (Bilan électrique RTE / Base
/// Carbone ADEME) avant publication ; tout changement = bump `acv-ademe@N`.
pub const TD_LOSS_FACTOR_V1: f64 = 0.072;

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
    /// Facteur du thermique fossile **agrégé** (mix régional). En v1 = facteur
    /// du gaz, le charbon/fioul étant quasi nuls en France (ADR-0008).
    pub thermique: f64,
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
            thermique: 406.0,
        }
    }
}

/// Émissions cycle de vie (gCO₂eq·MW/kWh) **et** production totale (MW) du mix
/// domestique. Pompage et échanges exclus ; productions négatives bornées à 0.
/// Brique partagée par les méthodes `acv-ademe` production et consommation.
fn production_emissions_output(mix: &GenerationMix, factors: &EmissionFactors) -> (f64, f64) {
    // Fossile : soit le thermique agrégé (régional), soit le détail par filière
    // (national). Les deux ne coexistent pas.
    let fossil = match mix.thermique {
        Some(thermique) => [(thermique, factors.thermique)].to_vec(),
        None => vec![
            (mix.gaz, factors.gaz),
            (mix.charbon, factors.charbon),
            (mix.fioul, factors.fioul),
        ],
    };

    let mut terms = vec![
        (mix.nucleaire, factors.nucleaire),
        (mix.hydraulique, factors.hydraulique),
        (mix.eolien, factors.eolien),
        (mix.solaire, factors.solaire),
        (mix.bioenergies, factors.bioenergies),
    ];
    terms.extend(fossil);

    let mut emissions = 0.0;
    let mut production = 0.0;
    for (output, factor) in terms {
        let output = output.max(0.0);
        emissions += output * factor;
        production += output;
    }
    (emissions, production)
}

/// Intensité carbone du mix de production, par la méthode `acv-ademe` (ADR-0008)
/// : moyenne des facteurs pondérée par la production. Pompage et échanges
/// exclus ; productions négatives bornées à 0. `None` si la production totale
/// est nulle (ou hors domaine).
pub fn acv_ademe_intensity(
    mix: &GenerationMix,
    factors: &EmissionFactors,
) -> Option<CarbonIntensity> {
    let (emissions, production) = production_emissions_output(mix, factors);
    if production <= 0.0 {
        return None;
    }
    CarbonIntensity::new(emissions / production)
}

/// Intensité carbone *consumption-based* (`acv-ademe@2`, ADR-0010) : empreinte
/// de l'électricité **réellement consommée** en France.
///
/// > (émissions de production − exports valorisés à l'intensité de production
/// > + imports valorisés à l'intensité du voisin) / consommation,
/// > puis *uplift* des pertes T&D.
///
/// avec `consommation = production − exports + imports`. Les exports sont
/// produits en France mais consommés ailleurs : retirés du numérateur (à
/// l'intensité de production domestique) **et** du dénominateur. `None` si la
/// production ou la consommation résultante est nulle.
pub fn acv_ademe_consumption_intensity(
    mix: &GenerationMix,
    flows: &CrossBorderFlows,
    factors: &EmissionFactors,
    td_loss: f64,
) -> Option<CarbonIntensity> {
    let (prod_emissions, prod_mwh) = production_emissions_output(mix, factors);
    if prod_mwh <= 0.0 {
        return None;
    }
    let prod_intensity = prod_emissions / prod_mwh;

    let imports_mwh = flows.imports_mw();
    let exports_mwh = flows.exports_mw();
    let imported_emissions = flows.imported_emissions();

    let consumption = prod_mwh - exports_mwh + imports_mwh;
    if consumption <= 0.0 {
        return None;
    }

    let consumed_emissions = prod_emissions - exports_mwh * prod_intensity + imported_emissions;
    let grid_intensity = (consumed_emissions / consumption).max(0.0);

    CarbonIntensity::new(grid_intensity * (1.0 + td_loss))
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
            thermique: None,
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
            thermique: None,
        };
        assert!(acv_ademe_intensity(&empty, &EmissionFactors::acv_ademe_v1()).is_none());
    }

    #[test]
    fn regional_thermique_aggregate_is_used() {
        // Mix régional (Bretagne) : thermique agrégé, pas de gaz/charbon/fioul.
        let regional = GenerationMix {
            nucleaire: 0.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 110.0,
            eolien: 373.0,
            solaire: 0.0,
            bioenergies: 45.0,
            pompage: 0.0,
            echanges: 1492.0, // import massif, exclu en v1 (basée production)
            thermique: Some(0.0),
        };
        let intensity = acv_ademe_intensity(&regional, &EmissionFactors::acv_ademe_v1()).unwrap();
        // Production locale : éolien + hydraulique + bioénergies (528 MW).
        // (373×7,3 + 110×4 + 45×24) / 528 ≈ 8,04 gCO₂eq/kWh.
        assert!(
            (intensity.value() - 8.04).abs() < 0.1,
            "intensité = {}",
            intensity.value()
        );
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
