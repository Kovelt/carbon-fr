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
//! **Ouvert** : [`EligibilitySignalProvenance`] (`sdk-spec.json`
//! `enums_to_duplicate.EligibilitySignalProvenance.open = true`) — même
//! doctrine que [`crate::Region`] : `#[non_exhaustive]` + variante
//! [`EligibilitySignalProvenance::Other`], pour ne jamais faire échouer la
//! désérialisation d'un signal si le serveur ajoute une provenance avant la
//! prochaine version du SDK.

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
}
