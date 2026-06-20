//! Désérialisation des documents XML ENTSO-E (IEC 62325) et projection vers le
//! domaine. La (dé)sérialisation vit ici, jamais dans `core`.
//!
//! Deux documents :
//! - **génération par type** (`documentType=A75`) → racine `GL_MarketDocument`,
//!   `TimeSeries/MktPSRType/psrType` + `Period/Point` → mix par filière ;
//! - **flux physique transfrontalier** (`documentType=A11`) → racine
//!   `Publication_MarketDocument`, `Period/Point` → série de puissance.
//!
//! Chemins XML **validés contre l'API live** le 2026-06-16 (test `--ignored`).

use std::collections::BTreeMap;

use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::EntsoeError;
use crate::codes::{Filiere, psr_type_to_filiere};

/// Point d'une série temporelle : `position` (1-based) + `quantity` (MW).
#[derive(Debug, Deserialize)]
pub(crate) struct Point {
    pub position: u32,
    pub quantity: f64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TimeInterval {
    pub start: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Period {
    #[serde(rename = "timeInterval")]
    pub time_interval: TimeInterval,
    pub resolution: String,
    #[serde(default, rename = "Point")]
    pub points: Vec<Point>,
}

impl Period {
    /// Développe la période en couples `(horodatage, MW)`.
    fn expand(&self) -> Result<Vec<(OffsetDateTime, f64)>, EntsoeError> {
        let start = parse_instant(&self.time_interval.start)?;
        let step = parse_resolution_minutes(&self.resolution)?;
        Ok(self
            .points
            .iter()
            .map(|p| {
                let at = start + time::Duration::minutes((p.position as i64 - 1) * step);
                (at, p.quantity)
            })
            .collect())
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct MktPsrType {
    #[serde(rename = "psrType")]
    pub psr_type: String,
}

/// `TimeSeries` d'un document de génération (`GL_MarketDocument`).
///
/// Une série porte **soit** `inBiddingZone_Domain.mRID` (production), **soit**
/// `outBiddingZone_Domain.mRID` (consommation associée, p. ex. pompage). On ne
/// retient que la **production** : sommer les deux double-compterait (vérifié sur
/// l'exemple officiel A75 qui contient une série de consommation).
#[derive(Debug, Deserialize)]
pub(crate) struct GenerationTimeSeries {
    #[serde(default, rename = "inBiddingZone_Domain.mRID")]
    pub in_domain: Option<String>,
    #[serde(rename = "MktPSRType")]
    pub psr: MktPsrType,
    #[serde(default, rename = "Period")]
    pub periods: Vec<Period>,
}

/// Document de génération par type de production.
#[derive(Debug, Deserialize)]
pub(crate) struct GenerationDocument {
    #[serde(default, rename = "TimeSeries")]
    pub series: Vec<GenerationTimeSeries>,
}

/// `TimeSeries` d'un document de flux physique (`Publication_MarketDocument`).
#[derive(Debug, Deserialize)]
pub(crate) struct FlowTimeSeries {
    #[serde(default, rename = "Period")]
    pub periods: Vec<Period>,
}

/// Document de flux physique transfrontalier (une direction).
#[derive(Debug, Deserialize)]
pub(crate) struct FlowDocument {
    #[serde(default, rename = "TimeSeries")]
    pub series: Vec<FlowTimeSeries>,
}

impl FlowDocument {
    /// Série de puissance `(horodatage, MW)` de la direction interrogée, agrégée
    /// sur les `TimeSeries`/`Period` du document.
    pub(crate) fn flow_series(&self) -> Result<BTreeMap<OffsetDateTime, f64>, EntsoeError> {
        let mut out = BTreeMap::new();
        for ts in &self.series {
            for period in &ts.periods {
                for (at, mw) in period.expand()? {
                    *out.entry(at).or_insert(0.0) += mw;
                }
            }
        }
        Ok(out)
    }
}

/// Point de prix d'une série day-ahead (A44) : `position` (1-based) +
/// `price.amount` (€/MWh). Élément distinct du `Point` MW (autre nom de valeur).
#[derive(Debug, Deserialize)]
pub(crate) struct PricePoint {
    pub position: u32,
    #[serde(rename = "price.amount")]
    pub amount: f64,
}

/// `Period` d'un document de prix : mêmes `timeInterval`/`resolution`, points de
/// prix.
#[derive(Debug, Deserialize)]
pub(crate) struct PricePeriod {
    #[serde(rename = "timeInterval")]
    pub time_interval: TimeInterval,
    pub resolution: String,
    #[serde(default, rename = "Point")]
    pub points: Vec<PricePoint>,
}

impl PricePeriod {
    /// Développe la période en couples `(horodatage, €/MWh)`.
    fn expand(&self) -> Result<Vec<(OffsetDateTime, f64)>, EntsoeError> {
        let start = parse_instant(&self.time_interval.start)?;
        let step = parse_resolution_minutes(&self.resolution)?;
        Ok(self
            .points
            .iter()
            .map(|p| {
                let at = start + time::Duration::minutes((p.position as i64 - 1) * step);
                (at, p.amount)
            })
            .collect())
    }
}

/// `TimeSeries` d'un document de prix day-ahead (`Publication_MarketDocument`).
#[derive(Debug, Deserialize)]
pub(crate) struct PriceTimeSeries {
    #[serde(default, rename = "Period")]
    pub periods: Vec<PricePeriod>,
}

/// Document de prix day-ahead du marché de gros (`documentType=A44`).
#[derive(Debug, Deserialize)]
pub(crate) struct DayAheadPriceDocument {
    #[serde(default, rename = "TimeSeries")]
    pub series: Vec<PriceTimeSeries>,
}

impl DayAheadPriceDocument {
    /// Série de prix `(horodatage, €/MWh)`, agrégée sur `TimeSeries`/`Period`.
    /// Le day-ahead a **une** valeur par pas : on écrase (pas de sommation,
    /// contrairement aux flux physiques).
    pub(crate) fn price_series(&self) -> Result<BTreeMap<OffsetDateTime, f64>, EntsoeError> {
        let mut out = BTreeMap::new();
        for ts in &self.series {
            for period in &ts.periods {
                for (at, eur) in period.expand()? {
                    out.insert(at, eur);
                }
            }
        }
        Ok(out)
    }
}

/// Mix de production par filière à un horodatage donné (MW agrégés par filière).
pub(crate) type MixByInstant = BTreeMap<OffsetDateTime, FiliereMw>;

/// MW par filière (agrégation des `PsrType`) à un instant.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FiliereMw {
    pub nucleaire: f64,
    pub gaz: f64,
    pub charbon: f64,
    pub fioul: f64,
    pub hydraulique: f64,
    pub eolien: f64,
    pub solaire: f64,
    pub bioenergies: f64,
}

impl FiliereMw {
    fn add(&mut self, filiere: Filiere, mw: f64) {
        let mw = mw.max(0.0);
        match filiere {
            Filiere::Nucleaire => self.nucleaire += mw,
            Filiere::Gaz => self.gaz += mw,
            Filiere::Charbon => self.charbon += mw,
            Filiere::Fioul => self.fioul += mw,
            Filiere::Hydraulique => self.hydraulique += mw,
            Filiere::Eolien => self.eolien += mw,
            Filiere::Solaire => self.solaire += mw,
            Filiere::Bioenergies => self.bioenergies += mw,
            Filiere::Ignore => {}
        }
    }
}

impl GenerationDocument {
    /// Agrège la génération par filière et par horodatage.
    pub(crate) fn mix_by_instant(&self) -> Result<MixByInstant, EntsoeError> {
        let mut out: MixByInstant = BTreeMap::new();
        for ts in &self.series {
            // Production seulement : on saute les séries de consommation
            // (`outBiddingZone_Domain` ⇒ pas d'`inBiddingZone_Domain`).
            if ts.in_domain.is_none() {
                continue;
            }
            let filiere = psr_type_to_filiere(&ts.psr.psr_type);
            for period in &ts.periods {
                for (at, mw) in period.expand()? {
                    out.entry(at).or_default().add(filiere, mw);
                }
            }
        }
        Ok(out)
    }
}

/// Parse un horodatage ENTSO-E (`yyyy-MM-ddTHH:mmZ`, parfois avec secondes).
fn parse_instant(raw: &str) -> Result<OffsetDateTime, EntsoeError> {
    // ENTSO-E omet souvent les secondes : on les rétablit pour RFC 3339.
    let normalised = if raw.len() == 17 && raw.ends_with('Z') {
        format!("{}:00Z", &raw[..16])
    } else {
        raw.to_string()
    };
    OffsetDateTime::parse(&normalised, &Rfc3339)
        .map_err(|_| EntsoeError::Parse(format!("horodatage invalide : {raw}")))
}

/// Convertit une résolution ISO-8601 (`PT15M`, `PT60M`, `PT1H`) en minutes.
fn parse_resolution_minutes(raw: &str) -> Result<i64, EntsoeError> {
    match raw {
        "PT15M" => Ok(15),
        "PT30M" => Ok(30),
        "PT60M" | "PT1H" => Ok(60),
        other => Err(EntsoeError::Parse(format!(
            "résolution non gérée : {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const GENERATION_XML: &str = r#"<?xml version="1.0"?>
<GL_MarketDocument xmlns="urn:iec62325.351:tc57wg16:451-6:generationloaddocument:3:0">
  <TimeSeries>
    <inBiddingZone_Domain.mRID codingScheme="A01">10YFR-RTE------C</inBiddingZone_Domain.mRID>
    <MktPSRType><psrType>B14</psrType></MktPSRType>
    <Period>
      <timeInterval><start>2024-01-01T00:00Z</start><end>2024-01-01T01:00Z</end></timeInterval>
      <resolution>PT60M</resolution>
      <Point><position>1</position><quantity>5000</quantity></Point>
    </Period>
  </TimeSeries>
  <TimeSeries>
    <inBiddingZone_Domain.mRID codingScheme="A01">10YFR-RTE------C</inBiddingZone_Domain.mRID>
    <MktPSRType><psrType>B04</psrType></MktPSRType>
    <Period>
      <timeInterval><start>2024-01-01T00:00Z</start><end>2024-01-01T01:00Z</end></timeInterval>
      <resolution>PT60M</resolution>
      <Point><position>1</position><quantity>1000</quantity></Point>
    </Period>
  </TimeSeries>
</GL_MarketDocument>"#;

    const FLOW_XML: &str = r#"<?xml version="1.0"?>
<Publication_MarketDocument xmlns="urn:iec62325.351:tc57wg16:451-3:publicationdocument:7:0">
  <TimeSeries>
    <in_Domain.mRID codingScheme="A01">10YFR-RTE------C</in_Domain.mRID>
    <out_Domain.mRID codingScheme="A01">10Y1001A1001A82H</out_Domain.mRID>
    <Period>
      <timeInterval><start>2024-01-01T00:00Z</start><end>2024-01-01T01:00Z</end></timeInterval>
      <resolution>PT60M</resolution>
      <Point><position>1</position><quantity>1500</quantity></Point>
    </Period>
  </TimeSeries>
</Publication_MarketDocument>"#;

    #[test]
    fn parses_generation_into_mix() {
        let doc: GenerationDocument = quick_xml::de::from_str(GENERATION_XML).unwrap();
        let mix = doc.mix_by_instant().unwrap();
        let slot = mix.get(&datetime!(2024-01-01 00:00 UTC)).unwrap();
        assert_eq!(slot.nucleaire, 5000.0);
        assert_eq!(slot.gaz, 1000.0);
    }

    #[test]
    fn parses_flow_series() {
        let doc: FlowDocument = quick_xml::de::from_str(FLOW_XML).unwrap();
        let series = doc.flow_series().unwrap();
        assert_eq!(series.get(&datetime!(2024-01-01 00:00 UTC)), Some(&1500.0));
    }

    const PRICE_XML: &str = r#"<?xml version="1.0"?>
<Publication_MarketDocument xmlns="urn:iec62325.351:tc57wg16:451-3:publicationdocument:7:0">
  <TimeSeries>
    <in_Domain.mRID codingScheme="A01">10YFR-RTE------C</in_Domain.mRID>
    <out_Domain.mRID codingScheme="A01">10YFR-RTE------C</out_Domain.mRID>
    <Period>
      <timeInterval><start>2024-01-01T00:00Z</start><end>2024-01-01T02:00Z</end></timeInterval>
      <resolution>PT60M</resolution>
      <Point><position>1</position><price.amount>42.5</price.amount></Point>
      <Point><position>2</position><price.amount>-3.1</price.amount></Point>
    </Period>
  </TimeSeries>
</Publication_MarketDocument>"#;

    #[test]
    fn parses_day_ahead_price_series_including_negative() {
        let doc: DayAheadPriceDocument = quick_xml::de::from_str(PRICE_XML).unwrap();
        let series = doc.price_series().unwrap();
        assert_eq!(series.get(&datetime!(2024-01-01 00:00 UTC)), Some(&42.5));
        // Prix négatif conservé tel quel (phénomène de marché réel).
        assert_eq!(series.get(&datetime!(2024-01-01 01:00 UTC)), Some(&-3.1));
    }

    // Exemples XML **officiels** ENTSO-E (gitlab.entsoe.eu/transparency/xml-examples)
    // — la validation qui compte : on parse la donnée telle que la plateforme la
    // produit, pas seulement nos fixtures faites main.
    const REAL_A75: &str = include_str!("../tests/fixtures/generation_a75.xml");
    const REAL_A11: &str = include_str!("../tests/fixtures/physical_flows_a11.xml");

    #[test]
    fn parses_official_a75_and_excludes_consumption_series() {
        let doc: GenerationDocument = quick_xml::de::from_str(REAL_A75).unwrap();
        let mix = doc.mix_by_instant().unwrap();
        let slot = mix.get(&datetime!(2013-12-18 12:00 UTC)).unwrap();
        // 3 TimeSeries : génération B14 (100), CONSOMMATION B14 (100, exclue),
        // génération B19 éolien (100). Le nucléaire doit valoir 100, pas 200.
        assert_eq!(slot.nucleaire, 100.0, "consommation non exclue → 200");
        assert_eq!(slot.eolien, 100.0);
    }

    #[test]
    fn parses_official_a11_flow() {
        let doc: FlowDocument = quick_xml::de::from_str(REAL_A11).unwrap();
        let series = doc.flow_series().unwrap();
        // Deux directions dans l'exemple (en requête réelle, une seule par appel).
        assert_eq!(series.get(&datetime!(2013-12-18 23:00 UTC)), Some(&100.0));
        assert_eq!(series.get(&datetime!(2013-12-18 22:00 UTC)), Some(&10.0));
    }

    #[test]
    fn quarter_hourly_positions_advance_by_15min() {
        let xml = r#"<GL_MarketDocument>
          <TimeSeries>
            <inBiddingZone_Domain.mRID>10YFR-RTE------C</inBiddingZone_Domain.mRID>
            <MktPSRType><psrType>B16</psrType></MktPSRType>
            <Period>
              <timeInterval><start>2024-06-01T10:00Z</start><end>2024-06-01T10:30Z</end></timeInterval>
              <resolution>PT15M</resolution>
              <Point><position>1</position><quantity>100</quantity></Point>
              <Point><position>2</position><quantity>200</quantity></Point>
            </Period>
          </TimeSeries>
        </GL_MarketDocument>"#;
        let doc: GenerationDocument = quick_xml::de::from_str(xml).unwrap();
        let mix = doc.mix_by_instant().unwrap();
        assert_eq!(
            mix.get(&datetime!(2024-06-01 10:00 UTC)).unwrap().solaire,
            100.0
        );
        assert_eq!(
            mix.get(&datetime!(2024-06-01 10:15 UTC)).unwrap().solaire,
            200.0
        );
    }
}
