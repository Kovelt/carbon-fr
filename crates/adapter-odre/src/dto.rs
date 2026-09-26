//! DTO de désérialisation des réponses ODRÉ et mapping vers le domaine.
//!
//! La frontière sérialisation vit ici (et non dans `core`, qui reste pur) :
//! `serde` décode le JSON d'ODRÉ, puis [`NationalRecord::into_measurement`]
//! traduit l'enregistrement brut en [`Measurement`] du domaine.

use carbonfr_core::domain::{
    CarbonIntensity, EmissionFactors, GenerationMix, LoadRecord, Measurement, Methodology, Region,
    Vintage, acv_ademe_intensity,
};
use carbonfr_core::ports::SourceError;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Réponse de l'endpoint `records` de l'API Explore v2.1 d'Opendatasoft.
#[derive(Debug, Deserialize)]
pub(crate) struct RecordsResponse<T> {
    pub total_count: u64,
    pub results: Vec<T>,
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
    #[serde(default)]
    pub consommation: Option<f64>,
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
                thermique: None,
            }),
        })
    }
}

impl NationalRecord {
    /// Charge **réalisée** portée par l'enregistrement (consommation), ou `None`.
    /// Sert au backfill de charge (proxy de calibration `climatology@2`).
    pub(crate) fn into_realized_load(self) -> Result<Option<LoadRecord>, SourceError> {
        let Some(mw) = self.consommation else {
            return Ok(None);
        };
        let at = OffsetDateTime::parse(&self.date_heure, &Rfc3339).map_err(|e| {
            SourceError::Invalid(format!(
                "horodatage illisible « {} » : {e}",
                self.date_heure
            ))
        })?;
        Ok(Some(LoadRecord::realized(at, Region::National, mw)))
    }
}

/// Enregistrement de **charge** du dataset `eco2mix-national-tr` : consommation
/// réalisée et prévisions RTE (ADR-0011 §4). La prévision intraday (`prevision_j`)
/// prime sur la veille (`prevision_j1`) quand elle existe.
#[derive(Debug, Deserialize)]
pub(crate) struct ConsumptionRecord {
    pub date_heure: String,
    pub consommation: Option<f64>,
    pub prevision_j1: Option<f64>,
    pub prevision_j: Option<f64>,
}

impl ConsumptionRecord {
    /// Convertit en [`LoadRecord`], ou `None` si aucune charge (ni réalisée ni
    /// prévue) n'est présente. Échoue si l'horodatage est illisible.
    pub(crate) fn into_load(self, region: Region) -> Result<Option<LoadRecord>, SourceError> {
        let realized = self.consommation;
        let forecast = self.prevision_j.or(self.prevision_j1);
        if realized.is_none() && forecast.is_none() {
            return Ok(None);
        }
        let at = OffsetDateTime::parse(&self.date_heure, &Rfc3339).map_err(|e| {
            SourceError::Invalid(format!(
                "horodatage illisible « {} » : {e}",
                self.date_heure
            ))
        })?;
        Ok(Some(LoadRecord {
            at,
            region,
            realized,
            forecast,
        }))
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

/// Un enregistrement du dataset `eco2mix-regional-tr` (temps réel) ou
/// `eco2mix-regional-cons-def` (export de masse, ADR-0003 addendum
/// 2026-09-26).
///
/// Le thermique fossile est **agrégé** (`thermique`) ; il n'y a pas de
/// `taux_co2` régional. L'intensité est donc **dérivée** par la méthode
/// `acv-ademe` (ADR-0008).
///
/// `code_insee_region` n'est renseigné (et exploité) que par l'export de masse
/// régional (`export_regional`, qui couvre toutes les régions à la fois) : les
/// requêtes `latest`/`range` filtrent déjà par région via `refine`, la région
/// leur est donc connue par ailleurs.
///
/// **Typage numérique tolérant** (constat prod 2026-09-26) : selon le jeu et le
/// champ, un même champ peut être publié en nombre JSON *ou* en chaîne
/// numérique (ex. `eolien: "115"` dans le consolidé, `pompage: "-2"` en temps
/// réel) — [`lenient_f64`] décode les deux, une chaîne vide/non numérique ou un
/// `null`/champ absent donnant `None` plutôt qu'une erreur de désérialisation.
#[derive(Debug, Deserialize)]
pub(crate) struct RegionalRecord {
    pub date_heure: String,
    pub nature: String,
    #[serde(default)]
    pub code_insee_region: Option<String>,
    #[serde(default, deserialize_with = "lenient_f64")]
    pub thermique: Option<f64>,
    #[serde(default, deserialize_with = "lenient_f64")]
    pub nucleaire: Option<f64>,
    #[serde(default, deserialize_with = "lenient_f64")]
    pub eolien: Option<f64>,
    #[serde(default, deserialize_with = "lenient_f64")]
    pub solaire: Option<f64>,
    #[serde(default, deserialize_with = "lenient_f64")]
    pub hydraulique: Option<f64>,
    #[serde(default, deserialize_with = "lenient_f64")]
    pub bioenergies: Option<f64>,
    #[serde(default, deserialize_with = "lenient_f64")]
    pub ech_physiques: Option<f64>,
    // NB : `pompage` n'entre pas dans le calcul acv-ademe → non décodé
    // (mix.pompage = 0), qu'il soit publié en nombre (consolidé) ou en chaîne
    // (temps réel).
}

/// Décodeur tolérant pour un champ numérique ODRÉ publié tantôt en nombre
/// JSON, tantôt en chaîne (constat prod 2026-09-26, cf. doc de
/// [`RegionalRecord`]) : nombre → `Some`, chaîne numérique (espaces en trop
/// tolérés) → `Some` parsée, chaîne vide/non numérique → `None` **sans
/// erreur** (comme un champ absent ou `null`, cf. `#[serde(default)]` sur
/// chaque champ appelant).
fn lenient_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Lenient {
        Number(f64),
        Text(String),
    }

    match Option::<Lenient>::deserialize(deserializer)? {
        None => Ok(None),
        Some(Lenient::Number(n)) => Ok(Some(n)),
        Some(Lenient::Text(s)) => Ok(s.trim().parse::<f64>().ok()),
    }
}

impl RegionalRecord {
    /// Convertit en [`Measurement`] régional `acv-ademe` (intensité dérivée du
    /// mix de production). Échoue si l'horodatage est illisible ; renvoie
    /// `NoData` si la production locale est nulle (intensité indéfinie).
    pub(crate) fn into_measurement(self, region: Region) -> Result<Measurement, SourceError> {
        let at = OffsetDateTime::parse(&self.date_heure, &Rfc3339).map_err(|e| {
            SourceError::Invalid(format!(
                "horodatage illisible « {} » : {e}",
                self.date_heure
            ))
        })?;

        let mix = GenerationMix {
            nucleaire: self.nucleaire.unwrap_or(0.0),
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: self.hydraulique.unwrap_or(0.0),
            eolien: self.eolien.unwrap_or(0.0),
            solaire: self.solaire.unwrap_or(0.0),
            bioenergies: self.bioenergies.unwrap_or(0.0),
            pompage: 0.0,
            echanges: self.ech_physiques.unwrap_or(0.0),
            thermique: Some(self.thermique.unwrap_or(0.0)),
        };

        let intensity = acv_ademe_intensity(&mix, &EmissionFactors::acv_ademe_v1())
            .ok_or(SourceError::NoData(region))?;

        Ok(Measurement {
            at,
            region,
            intensity,
            methodology: Methodology::acv_ademe(),
            vintage: parse_vintage(&self.nature),
            mix: Some(mix),
        })
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
        serde_json::from_str::<RecordsResponse<NationalRecord>>(json)
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

    fn first_regional(json: &str) -> RegionalRecord {
        serde_json::from_str::<RecordsResponse<RegionalRecord>>(json)
            .expect("désérialisation régionale")
            .results
            .into_iter()
            .next()
            .expect("au moins un résultat")
    }

    #[test]
    fn lenient_f64_decodes_plain_number() {
        let json = r#"{"total_count":1,"results":[{
            "nature":"Données temps réel",
            "date_heure":"2026-06-30T21:30:00+00:00",
            "eolien":115
        }]}"#;
        assert_eq!(first_regional(json).eolien, Some(115.0));
    }

    #[test]
    fn lenient_f64_decodes_numeric_string() {
        let json = r#"{"total_count":1,"results":[{
            "nature":"Données consolidées",
            "date_heure":"2026-06-30T21:30:00+00:00",
            "eolien":"115"
        }]}"#;
        assert_eq!(first_regional(json).eolien, Some(115.0));
    }

    #[test]
    fn lenient_f64_treats_null_and_absent_field_as_none() {
        let json = r#"{"total_count":1,"results":[{
            "nature":"Données temps réel",
            "date_heure":"2026-06-30T21:30:00+00:00",
            "eolien":null
        }]}"#;
        let record = first_regional(json);
        assert_eq!(record.eolien, None);
        // `hydraulique` totalement absent du JSON.
        assert_eq!(record.hydraulique, None);
    }

    #[test]
    fn lenient_f64_treats_non_numeric_string_as_none_without_error() {
        let json = r#"{"total_count":1,"results":[{
            "nature":"Données temps réel",
            "date_heure":"2026-06-30T21:30:00+00:00",
            "eolien":"ND"
        }]}"#;
        // Ne doit jamais faire échouer la désérialisation du lot.
        assert_eq!(first_regional(json).eolien, None);
    }

    /// Enregistrement réel du consolidé régional (région 84, capturé le
    /// 2026-09-26 — cf. cahier des charges PROD-1) : `eolien` en chaîne,
    /// `pompage` en nombre (non décodé, cf. commentaire du champ), et des
    /// champs `tco_*`/`tch_*`/`stockage_batterie`/`eolien_terrestre` inconnus
    /// du DTO — ignorés silencieusement par serde (pas de
    /// `deny_unknown_fields`).
    const REAL_CONSOLIDATED_REGIONAL_SAMPLE: &str = r#"{
        "total_count": 1,
        "results": [{
            "code_insee_region": "84",
            "libelle_region": "Auvergne-Rhône-Alpes",
            "nature": "Données consolidées",
            "date_heure": "2026-06-30T21:30:00+00:00",
            "consommation": 7122,
            "thermique": 376,
            "nucleaire": 4555,
            "eolien": "115",
            "solaire": 0,
            "hydraulique": 5098,
            "pompage": -4,
            "bioenergies": 111,
            "ech_physiques": -3129,
            "stockage_batterie": 0,
            "eolien_terrestre": "115",
            "eolien_offshore": "0",
            "tco_thermique": "5.3",
            "tch_thermique": "5.3"
        }]
    }"#;

    #[test]
    fn maps_real_consolidated_regional_record() {
        let record = first_regional(REAL_CONSOLIDATED_REGIONAL_SAMPLE);
        assert_eq!(record.code_insee_region.as_deref(), Some("84"));
        assert_eq!(record.eolien, Some(115.0));

        let region = Region::from_insee_code(record.code_insee_region.as_deref().unwrap())
            .expect("code INSEE 84 connu");
        assert_eq!(region, Region::AuvergneRhoneAlpes);

        let m = record.into_measurement(region).expect("mapping régional");
        assert_eq!(m.region, Region::AuvergneRhoneAlpes);
        assert_eq!(m.methodology, Methodology::acv_ademe());
        assert_eq!(m.vintage, Vintage::Consolidated);

        let mix = m.mix.expect("mix présent");
        assert_eq!(mix.eolien, 115.0);
        assert_eq!(mix.nucleaire, 4555.0);
        assert_eq!(mix.hydraulique, 5098.0);
        assert_eq!(mix.thermique, Some(376.0));
        // `pompage` n'est jamais décodé pour le régional (cf. `RegionalRecord`).
        assert_eq!(mix.pompage, 0.0);
    }
}
