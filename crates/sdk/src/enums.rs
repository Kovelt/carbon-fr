//! Petits enums de domaine **dupliqués** depuis le contrat HTTP `/v1`, comme
//! [`crate::Region`] (ADR-0031 décision 5) — pas de dépendance à
//! `carbonfr-core`. Valeurs sérialisées identiques aux slugs HTTP (query
//! string et JSON), sans renommage : `#[serde(rename_all = "kebab-case")]`
//! transforme un nom de variante `PascalCase` en son slug (`RteDirect` →
//! `rte-direct`), vérifié par les tests `serde_matches_http_slug` en bas de
//! fichier plutôt que supposé.
//!
//! **Exhaustifs** (ADR-0030 §3 : catalogue **fini par construction du
//! domaine** — l'étendre serait un changement de méthodologie/modèle
//! nécessitant un nouvel ADR, rien à protéger côté SemVer) : [`Methodology`],
//! [`Estimator`], [`EligibilityFramework`], [`Interval`], [`FlowDirection`],
//! [`ThresholdDirection`], [`WebhookStatus`], [`EligibilitySignalVerdict`].
//!
//! **Ouverts** : [`EligibilitySignalProvenance`] (`sdk-spec.json`
//! `enums_to_duplicate.EligibilitySignalProvenance.open = true`) et
//! [`IntensityIndex`] (index qualitatif, ADR-0033) — même doctrine que
//! [`crate::Region`] : `#[non_exhaustive]` + variante `Other`
//! ([`EligibilitySignalProvenance::Other`], [`IntensityIndex::Other`]), pour
//! ne jamais faire échouer la désérialisation d'une réponse si le serveur
//! ajoute une provenance ou une bande avant la prochaine version du SDK.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Méthodologie de calcul de l'intensité carbone (ADR-0005) : `rte-direct`
/// (national, taux CO₂ publié directement) ou `acv-ademe` (cycle de vie,
/// national + 12 régions). Catalogue **fini** : une nouvelle méthodologie est
/// un changement de méthodologie, donc un nouvel ADR (`CONTRIBUTING.md`) —
/// exhaustif, pas `#[non_exhaustive]` (ADR-0030 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Methodology {
    RteDirect,
    AcvAdeme,
}

impl Methodology {
    pub fn as_str(&self) -> &'static str {
        match self {
            Methodology::RteDirect => "rte-direct",
            Methodology::AcvAdeme => "acv-ademe",
        }
    }
}

impl fmt::Display for Methodology {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Estimateur d'intensité prévue (ADR-0009/0011) : `central` (estimation) ou
/// `prudent` (borne haute de l'intervalle de confiance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Estimator {
    Central,
    Prudent,
}

impl Estimator {
    pub fn as_str(&self) -> &'static str {
        match self {
            Estimator::Central => "central",
            Estimator::Prudent => "prudent",
        }
    }
}

impl fmt::Display for Estimator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Cadre d'éligibilité électrolyseur (ADR-0025/0026) : `rfnbo` (Règl. délégués
/// UE 2023/1184-1185) ou `low-carbon` (Acte délégué UE 2025/2359). Deux
/// piliers du droit européen tel qu'adopté ; une révision réglementaire
/// (`rfnbo:2026-revision`, `planned`) reste une **version** du même cadre
/// (`RulesetInfo::version`), pas un 3ᵉ cadre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EligibilityFramework {
    Rfnbo,
    LowCarbon,
}

impl EligibilityFramework {
    pub fn as_str(&self) -> &'static str {
        match self {
            EligibilityFramework::Rfnbo => "rfnbo",
            EligibilityFramework::LowCarbon => "low-carbon",
        }
    }
}

impl fmt::Display for EligibilityFramework {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Pas d'agrégation d'une série (`GET /v1/intensity/stats?interval=`) :
/// `hour` ou `day`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Interval {
    Hour,
    Day,
}

impl Interval {
    pub fn as_str(&self) -> &'static str {
        match self {
            Interval::Hour => "hour",
            Interval::Day => "day",
        }
    }
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Sens d'un flux transfrontalier (`ExchangeEntry`/`ExchangesResponse`,
/// ADR-0017) — **champ de réponse uniquement**, jamais un paramètre de
/// requête : `import` (`> 0`), `export` (`< 0`) ou `balanced`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowDirection {
    Import,
    Export,
    Balanced,
}

impl FlowDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            FlowDirection::Import => "import",
            FlowDirection::Export => "export",
            FlowDirection::Balanced => "balanced",
        }
    }
}

impl fmt::Display for FlowDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Sens de franchissement d'un seuil (webhooks, ADR-0016) — paramètre de
/// requête **et** champ de réponse (`CreateWebhookRequest`/`WebhookSummary`) :
/// `below` ou `above`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThresholdDirection {
    Below,
    Above,
}

impl ThresholdDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThresholdDirection::Below => "below",
            ThresholdDirection::Above => "above",
        }
    }
}

impl fmt::Display for ThresholdDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// État d'un abonnement webhook (`WebhookSummary::status`, ADR-0016) :
/// `active`, ou `disabled` (désactivé automatiquement après une série de
/// livraisons échouées — le réactiver suppose de le supprimer puis le
/// recréer, nouveau secret).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebhookStatus {
    Active,
    Disabled,
}

impl WebhookStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WebhookStatus::Active => "active",
            WebhookStatus::Disabled => "disabled",
        }
    }
}

impl fmt::Display for WebhookStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Verdict d'un signal d'éligibilité (`EligibilitySignalBody::verdict`,
/// ADR-0025/0026) : `pass`, `fail` ou `indeterminate` (donnée manquante,
/// jamais extrapolée).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EligibilitySignalVerdict {
    Pass,
    Fail,
    Indeterminate,
}

impl EligibilitySignalVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            EligibilitySignalVerdict::Pass => "pass",
            EligibilitySignalVerdict::Fail => "fail",
            EligibilitySignalVerdict::Indeterminate => "indeterminate",
        }
    }
}

impl fmt::Display for EligibilitySignalVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Provenance de la valeur d'un signal d'éligibilité
/// (`EligibilitySignalBody::provenance`, absente sur un signal
/// `indeterminate`) : `observed` (donnée mesurée/publiée) ou `forecast`
/// (prévision, `share-clim@1`). **Ouverte** (`#[non_exhaustive]` +
/// [`EligibilitySignalProvenance::Other`], comme [`crate::Region`]) :
/// `sdk-spec.json` la marque `open: true`, contrairement aux autres enums de
/// ce fichier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EligibilitySignalProvenance {
    Observed,
    Forecast,
    /// Valeur non reconnue par cette version du SDK — transmise telle quelle.
    Other(String),
}

impl EligibilitySignalProvenance {
    pub fn as_str(&self) -> &str {
        match self {
            EligibilitySignalProvenance::Observed => "observed",
            EligibilitySignalProvenance::Forecast => "forecast",
            EligibilitySignalProvenance::Other(s) => s.as_str(),
        }
    }
}

impl fmt::Display for EligibilitySignalProvenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for EligibilitySignalProvenance {
    fn from(value: &str) -> Self {
        match value {
            "observed" => EligibilitySignalProvenance::Observed,
            "forecast" => EligibilitySignalProvenance::Forecast,
            other => EligibilitySignalProvenance::Other(other.to_string()),
        }
    }
}

impl Serialize for EligibilitySignalProvenance {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EligibilitySignalProvenance {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer).map_err(D::Error::custom)?;
        Ok(EligibilitySignalProvenance::from(value.as_str()))
    }
}

/// Index qualitatif d'une intensité carbone (ADR-0033, barème `index@1`) :
/// cinq bandes **absolues** en gCO₂eq/kWh, séparées par les bornes
/// 20 · 64,8 · 100 · 200 — **basses incluses, hautes exclues** (`[a, b[` :
/// 20 g est `low`, 19,9 g est `very-low`). Un seul barème pour toutes les
/// méthodologies et toutes les régions ; le catalogue des bornes et de leurs
/// sources est servi par `GET /v1/intensity/index`
/// ([`CarbonFr::intensity_index`](crate::CarbonFr::intensity_index)).
///
/// Porté par le champ `index` de [`IntensityResponse`](crate::IntensityResponse),
/// de [`HistoryPoint`](crate::HistoryPoint), de
/// [`ForecastPointBody`](crate::ForecastPointBody) (calculé sur `expected`) et
/// de chaque événement du flux SSE (`IntensityEvent`, feature `stream`) :
/// `Some` face à un serveur ≥ 0.11, `None` face à un serveur plus ancien.
///
/// **Ouvert** : `#[non_exhaustive]` **et** variante [`IntensityIndex::Other`],
/// comme [`crate::Region`] — une bande ajoutée un jour par le serveur
/// (nouveau barème `index@2`, donc nouvel ADR) arrive en `Other(libellé)` au
/// lieu de faire échouer la désérialisation de la réponse entière (doctrine
/// d'ADR-0031 décision 5). Pas d'`Ord` : la variante `Other` n'a pas de rang
/// connu ; pour ordonner des bandes, lire `min`/`max` du catalogue
/// ([`IndexBand`](crate::IndexBand)).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IntensityIndex {
    /// `< 20` gCO₂eq/kWh.
    VeryLow,
    /// `20 ≤ x < 64,8` gCO₂eq/kWh — sous le seuil RFNBO de 18 gCO₂eq/MJ.
    Low,
    /// `64,8 ≤ x < 100` gCO₂eq/kWh — sous le seuil de la taxonomie européenne.
    Moderate,
    /// `100 ≤ x < 200` gCO₂eq/kWh.
    High,
    /// `≥ 200` gCO₂eq/kWh.
    VeryHigh,
    /// Libellé de bande non reconnu par cette version du SDK — transmis tel
    /// quel.
    Other(String),
}

impl IntensityIndex {
    /// Libellé HTTP, identique à la valeur servie dans le champ `index` :
    /// `very-low`, `low`, `moderate`, `high` ou `very-high` ; pour
    /// [`IntensityIndex::Other`], le libellé tel que reçu.
    pub fn as_str(&self) -> &str {
        match self {
            IntensityIndex::VeryLow => "very-low",
            IntensityIndex::Low => "low",
            IntensityIndex::Moderate => "moderate",
            IntensityIndex::High => "high",
            IntensityIndex::VeryHigh => "very-high",
            IntensityIndex::Other(label) => label.as_str(),
        }
    }
}

impl fmt::Display for IntensityIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for IntensityIndex {
    fn from(label: &str) -> Self {
        match label {
            "very-low" => IntensityIndex::VeryLow,
            "low" => IntensityIndex::Low,
            "moderate" => IntensityIndex::Moderate,
            "high" => IntensityIndex::High,
            "very-high" => IntensityIndex::VeryHigh,
            other => IntensityIndex::Other(other.to_string()),
        }
    }
}

impl From<String> for IntensityIndex {
    fn from(label: String) -> Self {
        IntensityIndex::from(label.as_str())
    }
}

/// Sérialisé comme le libellé HTTP (`as_str`), comme [`crate::Region`].
impl Serialize for IntensityIndex {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Désérialisé depuis n'importe quel libellé (`From<String>` : un libellé
/// inconnu devient [`IntensityIndex::Other`], jamais une erreur — cf. la doc
/// du type), comme [`crate::Region`].
impl<'de> Deserialize<'de> for IntensityIndex {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let label = String::deserialize(deserializer).map_err(D::Error::custom)?;
        Ok(IntensityIndex::from(label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Garde-fou : la conversion `#[serde(rename_all = "kebab-case")]`
    /// produit bien le même slug que `as_str()` (et donc que la query string
    /// HTTP construite avec `as_str()`, cf. `crate::methods`) — les deux
    /// chemins ne doivent jamais diverger.
    macro_rules! assert_serde_matches_as_str {
        ($($value:expr),+ $(,)?) => {
            $(
                let value = $value;
                let json = serde_json::to_string(&value).expect("sérialisation");
                assert_eq!(json, format!("\"{}\"", value.as_str()));
                let back = serde_json::from_str(&json).expect("désérialisation");
                assert_eq!(value, back);
            )+
        };
    }

    #[test]
    fn serde_matches_http_slug() {
        assert_serde_matches_as_str!(Methodology::RteDirect, Methodology::AcvAdeme);
        assert_serde_matches_as_str!(Estimator::Central, Estimator::Prudent);
        assert_serde_matches_as_str!(EligibilityFramework::Rfnbo, EligibilityFramework::LowCarbon);
        assert_serde_matches_as_str!(Interval::Hour, Interval::Day);
        assert_serde_matches_as_str!(
            FlowDirection::Import,
            FlowDirection::Export,
            FlowDirection::Balanced
        );
        assert_serde_matches_as_str!(ThresholdDirection::Below, ThresholdDirection::Above);
        assert_serde_matches_as_str!(WebhookStatus::Active, WebhookStatus::Disabled);
        assert_serde_matches_as_str!(
            EligibilitySignalVerdict::Pass,
            EligibilitySignalVerdict::Fail,
            EligibilitySignalVerdict::Indeterminate
        );
    }

    #[test]
    fn eligibility_signal_provenance_round_trips_known_and_unknown() {
        for slug in ["observed", "forecast", "measured-2027"] {
            let value = EligibilitySignalProvenance::from(slug);
            let json = serde_json::to_string(&value).expect("sérialisation");
            assert_eq!(json, format!("\"{slug}\""));
            let back: EligibilitySignalProvenance =
                serde_json::from_str(&json).expect("désérialisation");
            assert_eq!(back, value);
        }
        assert!(matches!(
            EligibilitySignalProvenance::from("measured-2027"),
            EligibilitySignalProvenance::Other(_)
        ));
    }

    #[test]
    fn intensity_index_round_trips_known_slugs() {
        for (slug, expected) in [
            ("very-low", IntensityIndex::VeryLow),
            ("low", IntensityIndex::Low),
            ("moderate", IntensityIndex::Moderate),
            ("high", IntensityIndex::High),
            ("very-high", IntensityIndex::VeryHigh),
        ] {
            let index = IntensityIndex::from(slug);
            assert_eq!(index, expected);
            assert_eq!(index.as_str(), slug);
            assert_eq!(index.to_string(), slug);
            assert_eq!(IntensityIndex::from(slug.to_string()), expected);
        }
    }

    #[test]
    fn intensity_index_unknown_slug_stays_open() {
        let index = IntensityIndex::from("nouvelle-bande");
        assert_eq!(index.as_str(), "nouvelle-bande");
        assert_eq!(index.to_string(), "nouvelle-bande");
        assert!(matches!(index, IntensityIndex::Other(_)));
        assert!(matches!(
            IntensityIndex::from("nouvelle-bande".to_string()),
            IntensityIndex::Other(_)
        ));
    }

    #[test]
    fn intensity_index_serde_round_trips_known_and_unknown_slugs() {
        for slug in ["very-low", "low", "moderate", "high", "very-high"] {
            let index = IntensityIndex::from(slug);
            assert!(!matches!(index, IntensityIndex::Other(_)), "{slug}");
            let json = serde_json::to_string(&index).expect("sérialisation");
            assert_eq!(json, format!("\"{slug}\""));
            let back: IntensityIndex = serde_json::from_str(&json).expect("désérialisation");
            assert_eq!(back, index);
        }

        // Un libellé inconnu se désérialise en `Other` (jamais d'erreur) et
        // se réserialise à l'identique.
        let unknown: IntensityIndex =
            serde_json::from_str("\"nouvelle-bande\"").expect("libellé inconnu toléré");
        assert_eq!(unknown, IntensityIndex::Other("nouvelle-bande".to_string()));
        let json = serde_json::to_string(&unknown).expect("sérialisation");
        assert_eq!(json, "\"nouvelle-bande\"");
        let back: IntensityIndex = serde_json::from_str(&json).expect("désérialisation");
        assert_eq!(back, unknown);
    }
}
