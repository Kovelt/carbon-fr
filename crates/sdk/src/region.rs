//! Slug de région, **dupliqué** depuis le contrat HTTP `/v1` — pas de
//! dépendance à `carbonfr-core` (ADR-0031 décision 5). Valeurs identiques à
//! celles attendues en paramètre de requête (query string) ; aucun
//! renommage serde.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Région éCO2mix : national ou l'une des 12 régions métropolitaines, ou tout
/// autre slug accepté/renvoyé par le serveur mais pas encore listé ici.
///
/// `#[non_exhaustive]` **et** variante ouverte [`Region::Other`] : le SDK TS
/// documente `Region` comme une union fermée de 13 valeurs tout en tolérant
/// `string & {}`, pour ne jamais perdre un futur slug serveur avant la
/// prochaine version du SDK — [`Region::Other`] joue le même rôle côté Rust
/// (contrairement au `Region` **exhaustif** d'`carbonfr-core`/`eligibility`,
/// qui documente le domaine interne, pas le contrat HTTP public, ADR-0030 §3).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
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
    ProvenceAlpesCoteDAzur,
    /// Slug non reconnu par cette version du SDK — transmis tel quel.
    Other(String),
}

impl Region {
    /// Slug HTTP (identique à la valeur de query string attendue par le
    /// serveur, `crates/adapter-http/src/handlers.rs::resolve_region`).
    pub fn as_str(&self) -> &str {
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
            Region::ProvenceAlpesCoteDAzur => "provence-alpes-cote-d-azur",
            Region::Other(slug) => slug.as_str(),
        }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for Region {
    fn from(slug: &str) -> Self {
        match slug {
            "national" => Region::National,
            "auvergne-rhone-alpes" => Region::AuvergneRhoneAlpes,
            "bourgogne-franche-comte" => Region::BourgogneFrancheComte,
            "bretagne" => Region::Bretagne,
            "centre-val-de-loire" => Region::CentreValDeLoire,
            "grand-est" => Region::GrandEst,
            "hauts-de-france" => Region::HautsDeFrance,
            "ile-de-france" => Region::IleDeFrance,
            "normandie" => Region::Normandie,
            "nouvelle-aquitaine" => Region::NouvelleAquitaine,
            "occitanie" => Region::Occitanie,
            "pays-de-la-loire" => Region::PaysDeLaLoire,
            "provence-alpes-cote-d-azur" => Region::ProvenceAlpesCoteDAzur,
            other => Region::Other(other.to_string()),
        }
    }
}

impl From<String> for Region {
    fn from(slug: String) -> Self {
        Region::from(slug.as_str())
    }
}

/// Sérialisé comme le slug HTTP (`as_str`) — utilisé tel quel dans un corps
/// de requête JSON (ex. `CreateWebhookRequest::region`).
impl Serialize for Region {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Désérialisé depuis n'importe quel slug (`From<String>` : un slug inconnu
/// devient [`Region::Other`], jamais une erreur — cf. le commentaire de
/// module) — utilisé pour les champs de réponse (`IntensityResponse::region`…).
impl<'de> Deserialize<'de> for Region {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let slug = String::deserialize(deserializer).map_err(D::Error::custom)?;
        Ok(Region::from(slug))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_known_slugs() {
        for slug in [
            "national",
            "auvergne-rhone-alpes",
            "bourgogne-franche-comte",
            "bretagne",
            "centre-val-de-loire",
            "grand-est",
            "hauts-de-france",
            "ile-de-france",
            "normandie",
            "nouvelle-aquitaine",
            "occitanie",
            "pays-de-la-loire",
            "provence-alpes-cote-d-azur",
        ] {
            let region = Region::from(slug);
            assert_eq!(region.as_str(), slug);
            assert!(!matches!(region, Region::Other(_)));
        }
    }

    #[test]
    fn unknown_slug_stays_open() {
        let region = Region::from("nouvelle-region-2027");
        assert_eq!(region.as_str(), "nouvelle-region-2027");
        assert!(matches!(region, Region::Other(_)));
    }

    #[test]
    fn serde_round_trips_known_and_unknown_slugs() {
        for slug in ["bretagne", "nouvelle-region-2027"] {
            let region = Region::from(slug);
            let json = serde_json::to_string(&region).expect("sérialisation");
            assert_eq!(json, format!("\"{slug}\""));
            let back: Region = serde_json::from_str(&json).expect("désérialisation");
            assert_eq!(back, region);
        }
    }
}
