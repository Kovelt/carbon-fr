//! Régions couvertes : le national agrégé + les 12 régions métropolitaines
//! d'éCO2mix régional (ADR-0003). Pas de DOM-TOM au lancement.

use std::fmt;

/// Périmètre géographique d'une mesure.
///
/// `National` correspond au jeu éCO2mix national ; les 12 variantes régionales
/// correspondent au découpage administratif métropolitain publié par RTE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    National,
    AuvergneRhoneAlpes,
    BourgogneFrancheComte,
    Bretagne,
    CentreValDeLoire,
    GrandEst,
    HautsDeFrance,
    IleDeFrance,
    Normandie,
    NouvelleAquitaine,
    Occitanie,
    PaysDeLaLoire,
    ProvenceAlpesCoteDazur,
}

impl Region {
    /// Les 12 régions métropolitaines (hors `National`).
    pub const METROPOLITAN: [Region; 12] = [
        Region::AuvergneRhoneAlpes,
        Region::BourgogneFrancheComte,
        Region::Bretagne,
        Region::CentreValDeLoire,
        Region::GrandEst,
        Region::HautsDeFrance,
        Region::IleDeFrance,
        Region::Normandie,
        Region::NouvelleAquitaine,
        Region::Occitanie,
        Region::PaysDeLaLoire,
        Region::ProvenceAlpesCoteDazur,
    ];

    /// Identifiant stable, utilisable en clé d'API ou de stockage.
    pub fn slug(self) -> &'static str {
        match self {
            Region::National => "national",
            Region::AuvergneRhoneAlpes => "auvergne-rhone-alpes",
            Region::BourgogneFrancheComte => "bourgogne-franche-comte",
            Region::Bretagne => "bretagne",
            Region::CentreValDeLoire => "centre-val-de-loire",
            Region::GrandEst => "grand-est",
            Region::HautsDeFrance => "hauts-de-france",
            Region::IleDeFrance => "ile-de-france",
            Region::Normandie => "normandie",
            Region::NouvelleAquitaine => "nouvelle-aquitaine",
            Region::Occitanie => "occitanie",
            Region::PaysDeLaLoire => "pays-de-la-loire",
            Region::ProvenceAlpesCoteDazur => "provence-alpes-cote-d-azur",
        }
    }

    /// Libellé humain.
    pub fn label(self) -> &'static str {
        match self {
            Region::National => "National",
            Region::AuvergneRhoneAlpes => "Auvergne-Rhône-Alpes",
            Region::BourgogneFrancheComte => "Bourgogne-Franche-Comté",
            Region::Bretagne => "Bretagne",
            Region::CentreValDeLoire => "Centre-Val de Loire",
            Region::GrandEst => "Grand Est",
            Region::HautsDeFrance => "Hauts-de-France",
            Region::IleDeFrance => "Île-de-France",
            Region::Normandie => "Normandie",
            Region::NouvelleAquitaine => "Nouvelle-Aquitaine",
            Region::Occitanie => "Occitanie",
            Region::PaysDeLaLoire => "Pays de la Loire",
            Region::ProvenceAlpesCoteDazur => "Provence-Alpes-Côte d'Azur",
        }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metropolitan_count_is_twelve() {
        assert_eq!(Region::METROPOLITAN.len(), 12);
    }

    #[test]
    fn slugs_are_unique() {
        let mut slugs: Vec<&str> = std::iter::once(Region::National)
            .chain(Region::METROPOLITAN)
            .map(Region::slug)
            .collect();
        slugs.sort_unstable();
        let before = slugs.len();
        slugs.dedup();
        assert_eq!(before, slugs.len());
    }
}
