//! Adapter sortant **ENTSO-E** : contexte d'import transfrontalier pour la
//! méthode `acv-ademe@2` *consumption-based* (ADR-0010).
//!
//! Implémente le port [`CrossBorderSource`](carbonfr_core::ports::CrossBorderSource)
//! : pour chaque frontière de la France métropolitaine, le **flux net signé**
//! (flux physique import − export, `documentType=A11`) et l'**intensité carbone
//! du voisin** dérivée de sa génération par type (`documentType=A75`,
//! `processType=A16`) via les **mêmes facteurs ADEME** que le domaine (méthode
//! cohérente, souveraine, vérifiable).
//!
//! La donnée vit sur la Transparency Platform ENTSO-E (organisme européen). Le
//! token (`CARBONFR_ENTSOE_TOKEN`) est requis ; **jamais appelée par requête
//! utilisateur** — le poller l'ingère.
//!
//! Chemins XML, codes EIC et URL de base (flux A11 / génération A75) **validés
//! contre l'API live** le 2026-06-16 (test `tests/live.rs`, `--ignored`) : 5
//! frontières actives (BE/DE/ES/IT/CH), flux et intensités voisines plausibles.
//! La frontière GB est indisponible côté ENTSO-E depuis le Brexit — dégradation
//! propre (frontière simplement absente des snapshots, pas d'erreur).
//!
//! Implémente aussi [`SpotPriceSource`](carbonfr_core::ports::SpotPriceSource) :
//! le **prix spot day-ahead** de la zone FR (`documentType=A44`, ADR-0023),
//! composante énergie de la décomposition du prix. Chemin A44 **validé contre
//! l'API live le 2026-06-20** (`recent_prices_live`, `--ignored`) : zone FR,
//! résolution **PT15M** (pas quart d'heure, MTU 15 min de la cible européenne),
//! prix plausibles. Le parseur développe correctement les positions au pas 15 min.

mod codes;
mod dto;

use std::collections::BTreeMap;

use async_trait::async_trait;
use carbonfr_core::domain::{
    CarbonIntensity, CrossBorderFlow, CrossBorderFlows, CrossBorderSnapshot, EmissionFactors,
    GenerationMix, Neighbor, SpotPrice, acv_ademe_intensity,
};
use carbonfr_core::ports::{CrossBorderSource, SourceError, SpotPriceSource};
use thiserror::Error;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{Duration, OffsetDateTime};

use codes::{FR_EIC, neighbor_eic};
use dto::{DayAheadPriceDocument, FiliereMw, FlowDocument, GenerationDocument};

const DEFAULT_BASE_URL: &str = "https://web-api.tp.entsoe.eu/api";
/// Fenêtre récente interrogée à chaque cycle (heures).
const DEFAULT_WINDOW_HOURS: i64 = 6;
/// `documentType` flux physique transfrontalier.
const DOC_PHYSICAL_FLOW: &str = "A11";
/// `documentType` génération par type de production.
const DOC_GENERATION: &str = "A75";
/// `documentType` prix day-ahead du marché de gros (ADR-0023).
const DOC_DAY_AHEAD_PRICE: &str = "A44";
/// `processType` génération réalisée.
const PROCESS_REALISED: &str = "A16";
/// Fenêtre **avant** maintenant couverte par l'ingestion de prix : le day-ahead
/// publie les heures à venir (utile à la primitive « cheapest window », ADR-0023).
const PRICE_LOOKAHEAD_HOURS: i64 = 24;

/// Format `periodStart`/`periodEnd` ENTSO-E : `yyyyMMddHHmm` (UTC).
const PERIOD_FMT: &[FormatItem<'static>] = format_description!("[year][month][day][hour][minute]");

/// Erreur de l'adapter ENTSO-E.
#[derive(Debug, Error)]
pub enum EntsoeError {
    #[error("requête ENTSO-E échouée : {0}")]
    Http(String),
    #[error("réponse ENTSO-E invalide : {0}")]
    Parse(String),
    #[error("configuration ENTSO-E absente : {0}")]
    Config(String),
}

impl From<EntsoeError> for SourceError {
    fn from(e: EntsoeError) -> Self {
        match e {
            EntsoeError::Http(m) | EntsoeError::Config(m) => SourceError::Unavailable(m),
            EntsoeError::Parse(m) => SourceError::Invalid(m),
        }
    }
}

/// Client HTTP **borné en temps** : sans timeouts, une réponse qui pend
/// bloquerait l'ingestion ENTSO-E du poller indéfiniment. Repli sur le défaut si
/// la construction échoue (improbable).
fn build_http() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

/// Client ENTSO-E (Transparency Platform RESTful API).
#[derive(Clone)]
pub struct EntsoeClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
    window_hours: i64,
}

/// Description d'une erreur réseau reqwest **sans l'URL** (audit F06).
///
/// `reqwest::Error::to_string()` embarque l'URL complète de la requête, qui porte
/// le `securityToken` ENTSO-E en query-string → propager `e.to_string()` fait
/// **fuiter le token en clair dans les logs** (surtout `CARBONFR_LOG_FORMAT=json`
/// agrégé). On ne conserve donc que la **nature** de l'erreur, jamais l'URL. Même
/// blindage que celui déjà appliqué au DSN Postgres.
fn describe_http_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "délai de requête dépassé".to_string()
    } else if e.is_connect() {
        "échec de connexion".to_string()
    } else if e.is_body() || e.is_decode() {
        "réponse illisible".to_string()
    } else if let Some(status) = e.status() {
        format!("statut {status}")
    } else {
        "requête réseau échouée".to_string()
    }
}

impl EntsoeClient {
    /// Construit le client depuis l'environnement (`CARBONFR_ENTSOE_TOKEN`,
    /// `CARBONFR_ENTSOE_BASE_URL`, `CARBONFR_ENTSOE_WINDOW_HOURS`).
    pub fn from_env() -> Result<Self, EntsoeError> {
        let token = std::env::var("CARBONFR_ENTSOE_TOKEN")
            .map_err(|_| EntsoeError::Config("CARBONFR_ENTSOE_TOKEN non défini".to_string()))?;
        let base_url = std::env::var("CARBONFR_ENTSOE_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let window_hours = std::env::var("CARBONFR_ENTSOE_WINDOW_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_WINDOW_HOURS);
        Ok(Self {
            http: build_http(),
            base_url,
            token,
            window_hours,
        })
    }

    /// Client explicite (tests / composition root alternative).
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            http: build_http(),
            base_url: base_url.into(),
            token: token.into(),
            window_hours: DEFAULT_WINDOW_HOURS,
        }
    }

    /// Récupère et désérialise un document XML pour des paramètres donnés.
    async fn fetch(&self, params: &[(&str, &str)]) -> Result<String, EntsoeError> {
        let mut query: Vec<(&str, &str)> = vec![("securityToken", self.token.as_str())];
        query.extend_from_slice(params);
        let resp = self
            .http
            .get(&self.base_url)
            .query(&query)
            .send()
            .await
            // NE PAS propager `e.to_string()` : il contient l'URL avec le token.
            .map_err(|e| EntsoeError::Http(describe_http_error(&e)))?;
        if !resp.status().is_success() {
            return Err(EntsoeError::Http(format!("statut {}", resp.status())));
        }
        resp.text()
            .await
            .map_err(|e| EntsoeError::Http(describe_http_error(&e)))
    }

    /// Génération par type d'une zone → intensité ACV par horodatage.
    async fn neighbor_intensity_series(
        &self,
        eic: &str,
        start: &str,
        end: &str,
    ) -> Result<BTreeMap<OffsetDateTime, f64>, EntsoeError> {
        let xml = self
            .fetch(&[
                ("documentType", DOC_GENERATION),
                ("processType", PROCESS_REALISED),
                ("in_Domain", eic),
                ("periodStart", start),
                ("periodEnd", end),
            ])
            .await?;
        let doc: GenerationDocument = quick_xml::de::from_str(&xml)
            .map_err(|e| EntsoeError::Parse(format!("génération : {e}")))?;
        let factors = EmissionFactors::acv_ademe_v1();
        let mut out = BTreeMap::new();
        for (at, mw) in doc.mix_by_instant()? {
            if let Some(intensity) = acv_ademe_intensity(&mix_from(&mw), &factors) {
                out.insert(at, intensity.value());
            }
        }
        Ok(out)
    }

    /// Prix spot day-ahead de la zone de marché FR → série €/MWh par horodatage.
    /// `in_Domain` = `out_Domain` = FR (le prix de la zone française, ADR-0023).
    async fn day_ahead_price_series(
        &self,
        start: &str,
        end: &str,
    ) -> Result<BTreeMap<OffsetDateTime, f64>, EntsoeError> {
        let xml = self
            .fetch(&[
                ("documentType", DOC_DAY_AHEAD_PRICE),
                ("in_Domain", FR_EIC),
                ("out_Domain", FR_EIC),
                ("periodStart", start),
                ("periodEnd", end),
            ])
            .await?;
        let doc: DayAheadPriceDocument = quick_xml::de::from_str(&xml)
            .map_err(|e| EntsoeError::Parse(format!("prix day-ahead : {e}")))?;
        doc.price_series()
    }

    /// Assemble les prix spot récents (et à venir, day-ahead) de la zone FR.
    async fn collect_recent_prices(&self) -> Result<Vec<SpotPrice>, EntsoeError> {
        let now = OffsetDateTime::now_utc();
        let from = now - Duration::hours(self.window_hours);
        let to = now + Duration::hours(PRICE_LOOKAHEAD_HOURS);
        let start = from
            .format(PERIOD_FMT)
            .map_err(|e| EntsoeError::Parse(e.to_string()))?;
        let end = to
            .format(PERIOD_FMT)
            .map_err(|e| EntsoeError::Parse(e.to_string()))?;

        let series = self.day_ahead_price_series(&start, &end).await?;
        // `SpotPrice::new` écarte les valeurs non finies ; les prix négatifs sont
        // conservés (phénomène de marché réel). BTreeMap → tri croissant garanti.
        Ok(series
            .into_iter()
            .filter_map(|(at, eur)| SpotPrice::new(at, eur))
            .collect())
    }

    /// Flux physique d'une direction (out → in) → série MW par horodatage.
    async fn flow_series(
        &self,
        out_domain: &str,
        in_domain: &str,
        start: &str,
        end: &str,
    ) -> Result<BTreeMap<OffsetDateTime, f64>, EntsoeError> {
        let xml = self
            .fetch(&[
                ("documentType", DOC_PHYSICAL_FLOW),
                ("out_Domain", out_domain),
                ("in_Domain", in_domain),
                ("periodStart", start),
                ("periodEnd", end),
            ])
            .await?;
        let doc: FlowDocument =
            quick_xml::de::from_str(&xml).map_err(|e| EntsoeError::Parse(format!("flux : {e}")))?;
        doc.flow_series()
    }

    /// Assemble les snapshots d'import récents pour toutes les frontières.
    async fn collect_recent(&self) -> Result<Vec<CrossBorderSnapshot>, EntsoeError> {
        let now = OffsetDateTime::now_utc();
        let from = now - Duration::hours(self.window_hours);
        let start = from
            .format(PERIOD_FMT)
            .map_err(|e| EntsoeError::Parse(e.to_string()))?;
        let end = now
            .format(PERIOD_FMT)
            .map_err(|e| EntsoeError::Parse(e.to_string()))?;

        // Par voisin : flux net signé (import − export) indexé par horodatage,
        // accompagné de l'intensité du voisin au plus proche (≤).
        let mut per_neighbor: Vec<(Neighbor, BTreeMap<OffsetDateTime, CrossBorderFlow>)> =
            Vec::new();
        for neighbor in Neighbor::ALL {
            let eic = neighbor_eic(neighbor);
            let imports = self.flow_series(eic, FR_EIC, &start, &end).await?;
            let exports = self.flow_series(FR_EIC, eic, &start, &end).await?;
            let intensity = self.neighbor_intensity_series(eic, &start, &end).await?;

            let mut instants: Vec<OffsetDateTime> = imports.keys().copied().collect();
            instants.extend(exports.keys().copied());
            instants.sort_unstable();
            instants.dedup();

            let mut by_instant = BTreeMap::new();
            for at in instants {
                let net = imports.get(&at).copied().unwrap_or(0.0)
                    - exports.get(&at).copied().unwrap_or(0.0);
                let Some(i_value) = at_or_before(&intensity, at) else {
                    continue; // pas d'intensité voisine connue → on saute ce flux
                };
                let Some(neighbor_intensity) = CarbonIntensity::new(i_value) else {
                    continue;
                };
                by_instant.insert(
                    at,
                    CrossBorderFlow {
                        neighbor,
                        flow_mw: net,
                        neighbor_intensity,
                    },
                );
            }
            per_neighbor.push((neighbor, by_instant));
        }

        // Instants de référence = union des horodatages de flux de tous les voisins.
        let mut instants: Vec<OffsetDateTime> = per_neighbor
            .iter()
            .flat_map(|(_, m)| m.keys().copied())
            .collect();
        instants.sort_unstable();
        instants.dedup();

        let mut snapshots = Vec::with_capacity(instants.len());
        for at in instants {
            let flows: Vec<CrossBorderFlow> = per_neighbor
                .iter()
                .filter_map(|(_, m)| at_or_before_value(m, at))
                .collect();
            if flows.is_empty() {
                continue;
            }
            snapshots.push(CrossBorderSnapshot {
                at,
                flows: CrossBorderFlows::new(flows),
            });
        }
        Ok(snapshots)
    }
}

#[async_trait]
impl CrossBorderSource for EntsoeClient {
    async fn recent_flows(&self) -> Result<Vec<CrossBorderSnapshot>, SourceError> {
        Ok(self.collect_recent().await?)
    }
}

#[async_trait]
impl SpotPriceSource for EntsoeClient {
    async fn recent_prices(&self) -> Result<Vec<SpotPrice>, SourceError> {
        Ok(self.collect_recent_prices().await?)
    }
}

/// Construit un `GenerationMix` national-style (thermique détaillé) à partir des
/// MW par filière d'un voisin. `echanges`/`pompage` à 0 : seule la production
/// compte pour l'intensité du voisin.
fn mix_from(f: &FiliereMw) -> GenerationMix {
    GenerationMix {
        nucleaire: f.nucleaire,
        gaz: f.gaz,
        charbon: f.charbon,
        fioul: f.fioul,
        hydraulique: f.hydraulique,
        eolien: f.eolien,
        solaire: f.solaire,
        bioenergies: f.bioenergies,
        pompage: 0.0,
        echanges: 0.0,
        thermique: None,
    }
}

/// Valeur d'une série au plus proche horodatage `≤ at`.
fn at_or_before(series: &BTreeMap<OffsetDateTime, f64>, at: OffsetDateTime) -> Option<f64> {
    series.range(..=at).next_back().map(|(_, v)| *v)
}

/// `CrossBorderFlow` au plus proche horodatage `≤ at` (clone).
fn at_or_before_value(
    series: &BTreeMap<OffsetDateTime, CrossBorderFlow>,
    at: OffsetDateTime,
) -> Option<CrossBorderFlow> {
    series.range(..=at).next_back().map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn at_or_before_picks_latest_not_after() {
        let mut s = BTreeMap::new();
        s.insert(datetime!(2024-01-01 00:00 UTC), 10.0);
        s.insert(datetime!(2024-01-01 01:00 UTC), 20.0);
        assert_eq!(
            at_or_before(&s, datetime!(2024-01-01 00:30 UTC)),
            Some(10.0)
        );
        assert_eq!(
            at_or_before(&s, datetime!(2024-01-01 01:00 UTC)),
            Some(20.0)
        );
        assert_eq!(at_or_before(&s, datetime!(2023-12-31 23:00 UTC)), None);
    }

    #[test]
    fn mix_from_excludes_exchanges_and_pumping() {
        let f = FiliereMw {
            nucleaire: 1000.0,
            ..Default::default()
        };
        let mix = mix_from(&f);
        assert_eq!(mix.nucleaire, 1000.0);
        assert_eq!(mix.echanges, 0.0);
        assert_eq!(mix.pompage, 0.0);
    }
}
