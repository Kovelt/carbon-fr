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
//!
//! IEC 62325 définit deux types de courbe (`curveType`) : `A01` (bloc fixe,
//! toutes les positions présentes) et `A03` (blocs de taille variable : une
//! valeur reste valable jusqu'à la position suivante, les **répétitions sont
//! omises** du document — cas des flux A11 et des prix A44). Le développement
//! des périodes comble donc les positions omises en reconduisant la dernière
//! valeur jusqu'au point suivant ou à la fin de période (cf. `expand_curve`).

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
    /// Fin de période : borne du comblement des positions omises (courbe A03).
    pub end: String,
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
    /// Développe la période en couples `(horodatage, MW)`, en reconduisant
    /// chaque point jusqu'au suivant ou à la fin de période (courbe A03).
    fn expand(&self) -> Result<Vec<(OffsetDateTime, f64)>, EntsoeError> {
        let start = parse_instant(&self.time_interval.start)?;
        let end = parse_instant(&self.time_interval.end)?;
        let step = parse_resolution_minutes(&self.resolution)?;
        expand_curve(
            start,
            end,
            step,
            self.points.iter().map(|p| (p.position, p.quantity)),
        )
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
    /// Type de courbe IEC 62325 (`A01`/`A03`). Lu pour documenter le contrat :
    /// le comblement d'`expand_curve` est inconditionnel et couvre les deux cas.
    #[serde(default, rename = "curveType")]
    #[allow(dead_code)]
    pub curve_type: Option<String>,
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
    /// Type de courbe IEC 62325 (`A01`/`A03`). Lu pour documenter le contrat :
    /// le comblement d'`expand_curve` est inconditionnel et couvre les deux cas.
    #[serde(default, rename = "curveType")]
    #[allow(dead_code)]
    pub curve_type: Option<String>,
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
    /// Développe la période en couples `(horodatage, €/MWh)`, en reconduisant
    /// chaque point jusqu'au suivant ou à la fin de période (courbe A03).
    fn expand(&self) -> Result<Vec<(OffsetDateTime, f64)>, EntsoeError> {
        let start = parse_instant(&self.time_interval.start)?;
        let end = parse_instant(&self.time_interval.end)?;
        let step = parse_resolution_minutes(&self.resolution)?;
        expand_curve(
            start,
            end,
            step,
            self.points.iter().map(|p| (p.position, p.amount)),
        )
    }
}

/// Nombre maximal de pas comblés par période : garde (esprit audit F14) contre
/// un XML malformé/hostile dont l'intervalle démesuré ferait exploser le
/// comblement en mémoire/CPU (repère : un an au pas 15 min = 35 136 pas).
const MAX_EXPAND_STEPS: i64 = 100_000;

/// Développe des points `(position, valeur)` en série `(horodatage, valeur)`,
/// en **reconduisant** chaque valeur jusqu'à la position du point suivant ou à
/// la fin de période (`timeInterval.end`) : c'est le contrat de la courbe A03
/// (blocs de taille variable, répétitions omises — cf. doc de module). Le
/// comblement est inconditionnel : sans effet sur une courbe A01 complète.
fn expand_curve(
    start: OffsetDateTime,
    end: OffsetDateTime,
    step_minutes: i64,
    points: impl Iterator<Item = (u32, f64)>,
) -> Result<Vec<(OffsetDateTime, f64)>, EntsoeError> {
    // Dernière position couverte par la période (division entière : une fin
    // non alignée sur le pas n'étend pas le comblement au-delà ; une fin
    // antérieure au début désactive simplement le comblement).
    let span_steps = (end - start).whole_minutes() / step_minutes;
    if span_steps > MAX_EXPAND_STEPS {
        return Err(EntsoeError::Parse(format!(
            "période démesurée : {span_steps} pas de {step_minutes} min"
        )));
    }
    let last_position = span_steps.max(0) as u32;
    // Les positions doivent croître pour borner le comblement (l'ordre du XML
    // n'est pas garanti par le parseur) : tri, sans déduplication.
    let mut sorted: Vec<(u32, f64)> = points.collect();
    sorted.sort_by_key(|&(position, _)| position);
    let mut out = Vec::new();
    for (i, &(position, value)) in sorted.iter().enumerate() {
        // Borne d'exclusion du comblement : position du point suivant, sinon
        // fin de période — jamais au-delà de la période (position suivante
        // malformée comprise).
        let until = sorted
            .get(i + 1)
            .map_or(u32::MAX, |&(next, _)| next)
            .min(last_position.saturating_add(1));
        // Le point explicite est toujours émis (une position hors bornes
        // propage une erreur, F14) ; le comblement suit jusqu'à `until`.
        let mut p = position;
        loop {
            out.push((expand_position(start, step_minutes, p)?, value));
            p = match p.checked_add(1) {
                Some(next) if next < until => next,
                _ => break,
            };
        }
    }
    Ok(out)
}

/// Décale `start` de `(position − 1) × step` minutes **sans paniquer** en cas de
/// dépassement : `position` vient du XML ENTSO-E, non borné (audit F14). Un point
/// malformé/hostile (`position` proche de `u32::MAX`) produirait un décalage de
/// centaines de milliers d'années → `start + Duration` panique et tue le poller.
/// On propage une `EntsoeError::Parse` à la place (échec par source, non
/// bloquant, comme partout ailleurs).
fn expand_position(
    start: OffsetDateTime,
    step_minutes: i64,
    position: u32,
) -> Result<OffsetDateTime, EntsoeError> {
    let offset_minutes = i64::from(position)
        .checked_sub(1)
        .and_then(|n| n.checked_mul(step_minutes))
        .ok_or_else(|| EntsoeError::Parse(format!("position hors bornes : {position}")))?;
    start
        .checked_add(time::Duration::minutes(offset_minutes))
        .ok_or_else(|| EntsoeError::Parse(format!("horodatage hors bornes (position {position})")))
}

/// `TimeSeries` d'un document de prix day-ahead (`Publication_MarketDocument`).
#[derive(Debug, Deserialize)]
pub(crate) struct PriceTimeSeries {
    /// Type de courbe IEC 62325 (`A01`/`A03`). Lu pour documenter le contrat :
    /// le comblement d'`expand_curve` est inconditionnel et couvre les deux cas.
    #[serde(default, rename = "curveType")]
    #[allow(dead_code)]
    pub curve_type: Option<String>,
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
    // `.get(..16)` plutôt que l'indexation `raw[..16]` : défense en profondeur
    // contre un slice sur une frontière non-UTF-8 (audit F14) — `None` propagé
    // en erreur au lieu de paniquer.
    let normalised = match raw.get(..16) {
        Some(head) if raw.len() == 17 && raw.ends_with('Z') => format!("{head}:00Z"),
        _ => raw.to_string(),
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
        // La fixture officielle est en courbe A03 : un unique Point (position 1)
        // couvre 24 h — la valeur doit être reconduite sur TOUS les pas, pas
        // seulement le premier (complétude, audit 2026-08).
        assert_eq!(doc.series[0].curve_type.as_deref(), Some("A03"));
        let first = doc.series[0].periods[0].expand().unwrap();
        assert_eq!(first.len(), 24, "24 pas horaires attendus (A03 comblée)");
        assert!(first.iter().all(|&(_, mw)| mw == 100.0));
        assert_eq!(first[0].0, datetime!(2013-12-18 23:00 UTC));
        assert_eq!(first[23].0, datetime!(2013-12-19 22:00 UTC));
        let second = doc.series[1].periods[0].expand().unwrap();
        assert_eq!(second.len(), 24);
        assert!(second.iter().all(|&(_, mw)| mw == 10.0));
        assert_eq!(second[0].0, datetime!(2013-12-18 22:00 UTC));
        // Agrégat du document : les deux directions de l'exemple se recouvrent
        // de 23:00 à 21:00 et se somment (en requête réelle, une seule
        // direction par appel — pas de recouvrement).
        let series = doc.flow_series().unwrap();
        assert_eq!(series.len(), 25);
        assert_eq!(series.get(&datetime!(2013-12-18 22:00 UTC)), Some(&10.0));
        assert_eq!(series.get(&datetime!(2013-12-18 23:00 UTC)), Some(&110.0));
        assert_eq!(series.get(&datetime!(2013-12-19 22:00 UTC)), Some(&100.0));
    }

    #[test]
    fn price_a03_carries_omitted_positions_forward() {
        // Courbe A03 au MTU 15 min : les positions 2-4 (prix inchangé) sont
        // omises du document ; chaque quart d'heure doit néanmoins porter un
        // prix — un trou ici rendrait le pilier prix rfnbo indéterminé à tort.
        let xml = r#"<?xml version="1.0"?>
<Publication_MarketDocument xmlns="urn:iec62325.351:tc57wg16:451-3:publicationdocument:7:0">
  <TimeSeries>
    <in_Domain.mRID codingScheme="A01">10YFR-RTE------C</in_Domain.mRID>
    <out_Domain.mRID codingScheme="A01">10YFR-RTE------C</out_Domain.mRID>
    <curveType>A03</curveType>
    <Period>
      <timeInterval><start>2026-01-01T00:00Z</start><end>2026-01-01T01:30Z</end></timeInterval>
      <resolution>PT15M</resolution>
      <Point><position>1</position><price.amount>60</price.amount></Point>
      <Point><position>5</position><price.amount>20</price.amount></Point>
    </Period>
  </TimeSeries>
</Publication_MarketDocument>"#;
        let doc: DayAheadPriceDocument = quick_xml::de::from_str(xml).unwrap();
        let series = doc.price_series().unwrap();
        assert_eq!(series.len(), 6, "6 quarts d'heure attendus (A03 comblée)");
        assert_eq!(series.get(&datetime!(2026-01-01 00:00 UTC)), Some(&60.0));
        assert_eq!(series.get(&datetime!(2026-01-01 00:15 UTC)), Some(&60.0));
        assert_eq!(series.get(&datetime!(2026-01-01 00:45 UTC)), Some(&60.0));
        assert_eq!(series.get(&datetime!(2026-01-01 01:00 UTC)), Some(&20.0));
        assert_eq!(series.get(&datetime!(2026-01-01 01:15 UTC)), Some(&20.0));
    }

    #[test]
    fn expand_position_rejects_overflow_without_panic() {
        // F14 : `position` non bornée venant du XML.
        let start = datetime!(2024-01-01 00:00 UTC);
        assert!(matches!(
            expand_position(start, 60, u32::MAX),
            Err(EntsoeError::Parse(_))
        ));
        // Non-régression du cas nominal.
        assert_eq!(expand_position(start, 60, 1).unwrap(), start);
        assert_eq!(
            expand_position(start, 15, 5).unwrap(),
            datetime!(2024-01-01 01:00 UTC)
        );
    }

    #[test]
    fn huge_position_in_generation_errors_instead_of_panicking() {
        // F14 : un XML malformé/hostile ne doit jamais paniquer le poller.
        let xml = r#"<GL_MarketDocument>
          <TimeSeries>
            <inBiddingZone_Domain.mRID>10YFR-RTE------C</inBiddingZone_Domain.mRID>
            <MktPSRType><psrType>B14</psrType></MktPSRType>
            <Period>
              <timeInterval><start>2024-01-01T00:00Z</start><end>2024-01-01T01:00Z</end></timeInterval>
              <resolution>PT60M</resolution>
              <Point><position>4294967295</position><quantity>5000</quantity></Point>
            </Period>
          </TimeSeries>
        </GL_MarketDocument>"#;
        let doc: GenerationDocument = quick_xml::de::from_str(xml).unwrap();
        assert!(matches!(doc.mix_by_instant(), Err(EntsoeError::Parse(_))));
    }

    #[test]
    fn huge_position_in_price_errors_instead_of_panicking() {
        // F14 : même motif dupliqué côté prix day-ahead.
        let xml = r#"<?xml version="1.0"?>
<Publication_MarketDocument xmlns="urn:iec62325.351:tc57wg16:451-3:publicationdocument:7:0">
  <TimeSeries>
    <in_Domain.mRID codingScheme="A01">10YFR-RTE------C</in_Domain.mRID>
    <out_Domain.mRID codingScheme="A01">10YFR-RTE------C</out_Domain.mRID>
    <Period>
      <timeInterval><start>2024-01-01T00:00Z</start><end>2024-01-01T02:00Z</end></timeInterval>
      <resolution>PT60M</resolution>
      <Point><position>4294967295</position><price.amount>42.5</price.amount></Point>
    </Period>
  </TimeSeries>
</Publication_MarketDocument>"#;
        let doc: DayAheadPriceDocument = quick_xml::de::from_str(xml).unwrap();
        assert!(matches!(doc.price_series(), Err(EntsoeError::Parse(_))));
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
