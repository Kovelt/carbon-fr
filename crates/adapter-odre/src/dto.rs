//! DTO de désérialisation des réponses ODRÉ et mapping vers le domaine.
//!
//! La frontière sérialisation vit ici (et non dans `core`, qui reste pur) :
//! `serde` décode le JSON d'ODRÉ, puis [`NationalRecord::into_measurement`]
//! traduit l'enregistrement brut en [`Measurement`] du domaine.

use carbonfr_core::domain::{
    CarbonIntensity, GenerationMix, Measurement, Methodology, Region, Vintage,
};
use carbonfr_core::ports::SourceError;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Réponse de l'endpoint `records` de l'API Explore v2.1 d'Opendatasoft.
#[derive(Debug, Deserialize)]
pub(crate) struct RecordsResponse {
    pub total_count: u64,
    pub results: Vec<NationalRecord>,
}

/// Un enregistrement du dataset `eco2mix-national-tr`.
///
/// Seuls les champs exploités par la méthodologie `rte-direct` sont décodés ;
/// l'API en expose davantage (détail thermique, échanges commerciaux par pays,
/// stockage batterie…), ignorés ici.
#[derive(Debug, Deserialize)]
pub(crate) struct NationalRecord {
    pub date_heure: String,
    pub nature: String,
    pub taux_co2: Option<f64>,
    pub nucleaire: Option<f64>,
    pub gaz: Option<f64>,
    pub charbon: Option<f64>,
    pub fioul: Option<f64>,
    pub hydraulique: Option<f64>,
    pub eolien: Option<f64>,
    pub solaire: Option<f64>,
    pub bioenergies: Option<f64>,
    pub pompage: Option<f64>,
    pub ech_physiques: Option<f64>,
}

impl NationalRecord {
    /// Convertit l'enregistrement en [`Measurement`] national (méthodologie
    /// `rte-direct`, ADR-0005). Échoue si l'horodatage ou le `taux_co2` est
    /// illisible ou hors domaine.
    pub(crate) fn into_measurement(self) -> Result<Measurement, SourceError> {
        let at = OffsetDateTime::parse(&self.date_heure, &Rfc3339).map_err(|e| {
            SourceError::Invalid(format!(
                "horodatage illisible « {} » : {e}",
                self.date_heure
            ))
        })?;

        let taux = self
            .taux_co2
            .ok_or_else(|| SourceError::Invalid("taux_co2 absent de l'enregistrement".into()))?;
        let intensity = CarbonIntensity::new(taux)
            .ok_or_else(|| SourceError::Invalid(format!("taux_co2 hors domaine : {taux}")))?;

        Ok(Measurement {
            at,
            region: Region::National,
            intensity,
            methodology: Methodology::rte_direct(),
            vintage: parse_vintage(&self.nature),
            mix: Some(GenerationMix {
                nucleaire: self.nucleaire.unwrap_or(0.0),
                gaz: self.gaz.unwrap_or(0.0),
                charbon: self.charbon.unwrap_or(0.0),
                fioul: self.fioul.unwrap_or(0.0),
                hydraulique: self.hydraulique.unwrap_or(0.0),
                eolien: self.eolien.unwrap_or(0.0),
                solaire: self.solaire.unwrap_or(0.0),
                bioenergies: self.bioenergies.unwrap_or(0.0),
                pompage: self.pompage.unwrap_or(0.0),
                echanges: self.ech_physiques.unwrap_or(0.0),
            }),
        })
    }
}

/// Mappe le champ `nature` d'ODRÉ vers le millésime (ADR-0006).
///
/// Une valeur inconnue est rabattue sur [`Vintage::Tr`] (qualité la plus
/// basse) : l'upsert conditionnel ne risque alors jamais d'écraser une donnée
/// déjà consolidée par une valeur mal étiquetée.
fn parse_vintage(nature: &str) -> Vintage {
    match nature.trim() {
        "Données consolidées" => Vintage::Consolidated,
        "Données définitives" => Vintage::Definitive,
        _ => Vintage::Tr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Enregistrement réel (capturé sur eco2mix-national-tr), tronqué aux champs
    // décodés. Les champs absents ici (détail thermique…) sont ignorés.
    const SAMPLE: &str = r#"{
        "total_count": 1,
        "results": [{
            "perimetre": "France",
            "nature": "Données temps réel",
            "date_heure": "2026-06-14T19:00:00+00:00",
            "consommation": 41528,
            "fioul": 34, "charbon": 0, "gaz": 666, "nucleaire": 38815,
            "eolien": 2555, "solaire": 1050, "hydraulique": 8893,
            "pompage": -76, "bioenergies": 1006, "ech_physiques": -11574,
            "taux_co2": 15
        }]
    }"#;

    fn first(json: &str) -> NationalRecord {
        serde_json::from_str::<RecordsResponse>(json)
            .expect("désérialisation")
            .results
            .into_iter()
            .next()
            .expect("au moins un résultat")
    }

    #[test]
    fn maps_national_record() {
        let m = first(SAMPLE).into_measurement().expect("mapping");

        assert_eq!(m.region, Region::National);
        assert_eq!(m.intensity.value(), 15.0);
        assert_eq!(m.vintage, Vintage::Tr);
        assert_eq!(m.methodology, Methodology::rte_direct());

        assert_eq!(m.at.year(), 2026);
        assert_eq!(m.at.hour(), 19);
        assert_eq!(m.at.offset(), time::UtcOffset::UTC);

        let mix = m.mix.expect("mix présent");
        assert_eq!(mix.nucleaire, 38815.0);
        assert_eq!(mix.echanges, -11574.0);
        assert_eq!(mix.pompage, -76.0);
    }

    #[test]
    fn vintage_mapping_covers_three_natures() {
        assert_eq!(parse_vintage("Données temps réel"), Vintage::Tr);
        assert_eq!(parse_vintage("Données consolidées"), Vintage::Consolidated);
        assert_eq!(parse_vintage("Données définitives"), Vintage::Definitive);
        // Inconnu → repli prudent sur la plus basse qualité.
        assert_eq!(parse_vintage("Autre chose"), Vintage::Tr);
    }

    #[test]
    fn missing_taux_co2_is_invalid() {
        let json = r#"{"total_count":1,"results":[{
            "nature":"Données temps réel",
            "date_heure":"2026-06-14T19:00:00+00:00",
            "taux_co2":null
        }]}"#;
        let err = first(json).into_measurement().unwrap_err();
        assert!(matches!(err, SourceError::Invalid(_)));
    }

    #[test]
    fn unparseable_timestamp_is_invalid() {
        let json = r#"{"total_count":1,"results":[{
            "nature":"Données temps réel",
            "date_heure":"hier soir",
            "taux_co2":15
        }]}"#;
        let err = first(json).into_measurement().unwrap_err();
        assert!(matches!(err, SourceError::Invalid(_)));
    }
}
