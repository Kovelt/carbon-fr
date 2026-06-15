//! Codes ENTSO-E : zones EIC et types de production (`PsrType`).
//!
//! Tables de correspondance vers le vocabulaire du domaine. Sources : codes EIC
//! des zones d'ajustement et liste `PsrType` du guide RESTful API ENTSO-E
//! (IEC 62325). ⚠️ **à valider contre l'API live** (test `--ignored`).

use carbonfr_core::domain::Neighbor;

/// Zone EIC de la France métropolitaine (RTE).
pub const FR_EIC: &str = "10YFR-RTE------C";

/// Zone EIC d'un voisin (zone d'ajustement adjacente à RTE).
///
/// `Italy` = zone Italie-Nord (frontière physique avec la France) ; `Germany` =
/// zone de marché commune Allemagne–Luxembourg (DE-LU).
pub fn neighbor_eic(neighbor: Neighbor) -> &'static str {
    match neighbor {
        Neighbor::Belgium => "10YBE----------2",
        Neighbor::Germany => "10Y1001A1001A82H", // DE-LU
        Neighbor::Spain => "10YES-REE------0",
        Neighbor::Italy => "10Y1001A1001A73I", // IT-North
        Neighbor::Switzerland => "10YCH-SWISSGRIDZ",
        Neighbor::GreatBritain => "10YGB----------A",
    }
}

/// Filière du domaine ciblée par un type de production ENTSO-E (`PsrType`).
///
/// Le mapping agrège vers les filières de `GenerationMix`. Stockage hydraulique
/// (pompage, B10) et stockage batterie (B25) sont **ignorés** (pas une
/// production primaire — cohérent avec l'exclusion du pompage côté domaine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filiere {
    Nucleaire,
    Gaz,
    Charbon,
    Fioul,
    Hydraulique,
    Eolien,
    Solaire,
    Bioenergies,
    /// Comptabilisé hors facteur (déchets, géothermie, autre) : exclu du calcul.
    Ignore,
}

/// Mappe un code `PsrType` ENTSO-E (B01..B25) vers une filière du domaine.
pub fn psr_type_to_filiere(code: &str) -> Filiere {
    match code {
        "B01" => Filiere::Bioenergies, // Biomass
        "B02" => Filiere::Charbon,     // Fossil Brown coal/Lignite
        "B03" => Filiere::Gaz,         // Fossil Coal-derived gas
        "B04" => Filiere::Gaz,         // Fossil Gas
        "B05" => Filiere::Charbon,     // Fossil Hard coal
        "B06" => Filiere::Fioul,       // Fossil Oil
        "B07" => Filiere::Fioul,       // Fossil Oil shale
        "B08" => Filiere::Charbon,     // Fossil Peat (assimilé charbon)
        "B09" => Filiere::Ignore,      // Geothermal
        "B10" => Filiere::Ignore,      // Hydro Pumped Storage
        "B11" => Filiere::Hydraulique, // Hydro Run-of-river
        "B12" => Filiere::Hydraulique, // Hydro Water Reservoir
        "B13" => Filiere::Ignore,      // Marine
        "B14" => Filiere::Nucleaire,   // Nuclear
        "B15" => Filiere::Bioenergies, // Other renewable (prudence)
        "B16" => Filiere::Solaire,     // Solar
        "B17" => Filiere::Bioenergies, // Waste
        "B18" => Filiere::Eolien,      // Wind Offshore
        "B19" => Filiere::Eolien,      // Wind Onshore
        _ => Filiere::Ignore,          // B20 Other, B25 Storage, inconnu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_psr_types_map_to_expected_filiere() {
        assert_eq!(psr_type_to_filiere("B14"), Filiere::Nucleaire);
        assert_eq!(psr_type_to_filiere("B04"), Filiere::Gaz);
        assert_eq!(psr_type_to_filiere("B05"), Filiere::Charbon);
        assert_eq!(psr_type_to_filiere("B16"), Filiere::Solaire);
        assert_eq!(psr_type_to_filiere("B19"), Filiere::Eolien);
        assert_eq!(psr_type_to_filiere("B11"), Filiere::Hydraulique);
        // Pompage & inconnu → ignorés.
        assert_eq!(psr_type_to_filiere("B10"), Filiere::Ignore);
        assert_eq!(psr_type_to_filiere("B99"), Filiere::Ignore);
    }

    #[test]
    fn every_neighbor_has_an_eic() {
        for n in Neighbor::ALL {
            assert!(neighbor_eic(n).starts_with("10Y"));
        }
    }
}
