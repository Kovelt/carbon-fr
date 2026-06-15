//! # carbonfr-adapter-odre
//!
//! Adapter **sortant** : implémentation de [`Eco2mixSource`] au-dessus de l'API
//! Explore d'[ODRÉ](https://odre.opendatasoft.com/) (jeux éCO2mix de RTE).
//!
//! ## Périmètre
//!
//! L'éCO2mix **temps réel** ne publie le `taux_co2` (intensité carbone) qu'au
//! niveau **national** → mesures `rte-direct` nationales. Pour les **12 régions**,
//! l'adapter lit le mix de production régional (`eco2mix-regional-tr`) et **dérive**
//! l'intensité par la méthode `acv-ademe` (cycle de vie, ADR-0008). Le national
//! porte donc `rte-direct` (+ `acv-ademe` dérivée par l'ingestion) ; les régions
//! portent `acv-ademe`.
//!
//! Conformément au quota (ADR-0003), un **poller unique** appelle cet adapter ;
//! l'API sert ensuite depuis la base. Le backfill historique passe par l'export
//! de masse d'ODRÉ, pas par [`range`](OdreClient::range) (qui est plafonné par
//! l'API paginée).

mod dto;

use async_trait::async_trait;
use carbonfr_core::domain::{LoadRecord, Measurement, Region, TimeRange};
use carbonfr_core::ports::{ConsumptionSource, Eco2mixArchive, Eco2mixSource, SourceError};
use serde::de::DeserializeOwned;
use time::format_description::well_known::Rfc3339;

use dto::{ConsumptionRecord, NationalRecord, RecordsResponse, RegionalRecord};

/// URL de base de l'API Explore d'ODRÉ.
const DEFAULT_BASE_URL: &str = "https://odre.opendatasoft.com";
/// Dataset éCO2mix national temps réel.
const NATIONAL_DATASET: &str = "eco2mix-national-tr";
/// Dataset éCO2mix régional temps réel (intensité dérivée, ADR-0008).
const REGIONAL_DATASET: &str = "eco2mix-regional-tr";
/// Dataset éCO2mix national consolidé + définitif (historique, ADR-0003).
const NATIONAL_ARCHIVE_DATASET: &str = "eco2mix-national-cons-def";
/// Plafond de pagination de l'API ODS v2.1 (`offset + limit ≤ 10 000`).
const API_WINDOW: u64 = 10_000;
/// Taille de page (maximum autorisé par l'API `records`).
const PAGE_SIZE: u64 = 100;

/// Client de la source éCO2mix d'ODRÉ.
///
/// Sans état métier : un seul client peut être partagé (`reqwest::Client` gère
/// son propre pool de connexions et est `Clone`).
#[derive(Clone)]
pub struct OdreClient {
    http: reqwest::Client,
    base_url: String,
}

impl OdreClient {
    /// Construit un client visant l'API publique d'ODRÉ.
    pub fn new() -> Result<Self, SourceError> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("carbon-fr/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| SourceError::Unavailable(format!("construction du client HTTP : {e}")))?;
        Ok(Self::with_http(http, DEFAULT_BASE_URL))
    }

    /// Construit un client à partir d'un [`reqwest::Client`] et d'une URL de
    /// base explicites — utile pour pointer vers un serveur factice en test.
    pub fn with_http(http: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            http,
            base_url: base_url.into(),
        }
    }

    fn records_url(&self, dataset: &str) -> String {
        format!(
            "{}/api/explore/v2.1/catalog/datasets/{dataset}/records",
            self.base_url.trim_end_matches('/')
        )
    }

    async fn fetch<T: DeserializeOwned>(
        &self,
        dataset: &str,
        query: &[(&str, String)],
    ) -> Result<RecordsResponse<T>, SourceError> {
        let resp = self
            .http
            .get(self.records_url(dataset))
            .query(query)
            .send()
            .await
            .map_err(|e| SourceError::Unavailable(format!("requête ODRÉ : {e}")))?;

        if !resp.status().is_success() {
            return Err(SourceError::Unavailable(format!(
                "ODRÉ a répondu {}",
                resp.status()
            )));
        }

        resp.json::<RecordsResponse<T>>()
            .await
            .map_err(|e| SourceError::Invalid(format!("réponse ODRÉ illisible : {e}")))
    }

    /// Filtre ODSQL `date_heure ∈ [start, end)` restreint aux mesures portant un
    /// `taux_co2`.
    fn time_filter(range: TimeRange) -> Result<String, SourceError> {
        let start = range
            .start()
            .format(&Rfc3339)
            .map_err(|e| SourceError::Invalid(format!("borne de début : {e}")))?;
        let end = range
            .end()
            .format(&Rfc3339)
            .map_err(|e| SourceError::Invalid(format!("borne de fin : {e}")))?;
        Ok(format!(
            "date_heure >= '{start}' and date_heure < '{end}' and taux_co2 is not null"
        ))
    }

    /// Export de masse (un téléchargement) : l'endpoint `exports/json` renvoie un
    /// tableau JSON de tous les enregistrements filtrés, sans plafond paginé.
    async fn fetch_export(
        &self,
        dataset: &str,
        filter: &str,
    ) -> Result<Vec<NationalRecord>, SourceError> {
        let url = format!(
            "{}/api/explore/v2.1/catalog/datasets/{dataset}/exports/json",
            self.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .get(url)
            .query(&[("where", filter)])
            .send()
            .await
            .map_err(|e| SourceError::Unavailable(format!("export ODRÉ : {e}")))?;

        if !resp.status().is_success() {
            return Err(SourceError::Unavailable(format!(
                "ODRÉ (export) a répondu {}",
                resp.status()
            )));
        }

        resp.json::<Vec<NationalRecord>>()
            .await
            .map_err(|e| SourceError::Invalid(format!("export ODRÉ illisible : {e}")))
    }

    /// Filtre de facette ODRÉ ciblant une région par code INSEE.
    fn region_refine(region: Region) -> Result<String, SourceError> {
        let code = region.insee_code().ok_or(SourceError::NoData(region))?;
        Ok(format!("code_insee_region:\"{code}\""))
    }

    /// Dernière mesure régionale : mix de production le plus récent, intensité
    /// dérivée `acv-ademe` (ADR-0008).
    async fn latest_regional(&self, region: Region) -> Result<Measurement, SourceError> {
        let query = [
            ("refine", Self::region_refine(region)?),
            ("where", "consommation is not null".to_string()),
            ("order_by", "date_heure desc".to_string()),
            ("limit", "1".to_string()),
        ];
        let page = self
            .fetch::<RegionalRecord>(REGIONAL_DATASET, &query)
            .await?;
        page.results
            .into_iter()
            .next()
            .ok_or(SourceError::NoData(region))?
            .into_measurement(region)
    }

    /// Série régionale sur un intervalle (API paginée). Les créneaux sans
    /// production locale (intensité indéfinie) sont ignorés.
    async fn range_regional(
        &self,
        region: Region,
        range: TimeRange,
    ) -> Result<Vec<Measurement>, SourceError> {
        let refine = Self::region_refine(region)?;
        let start = range
            .start()
            .format(&Rfc3339)
            .map_err(|e| SourceError::Invalid(format!("borne de début : {e}")))?;
        let end = range
            .end()
            .format(&Rfc3339)
            .map_err(|e| SourceError::Invalid(format!("borne de fin : {e}")))?;
        let where_clause = format!(
            "date_heure >= '{start}' and date_heure < '{end}' and consommation is not null"
        );

        let mut measurements = Vec::new();
        let mut offset = 0u64;
        loop {
            let query = [
                ("refine", refine.clone()),
                ("where", where_clause.clone()),
                ("order_by", "date_heure asc".to_string()),
                ("limit", PAGE_SIZE.to_string()),
                ("offset", offset.to_string()),
            ];
            let page = self
                .fetch::<RegionalRecord>(REGIONAL_DATASET, &query)
                .await?;
            let total = page.total_count;

            if offset == 0 && total > API_WINDOW {
                return Err(SourceError::Unavailable(format!(
                    "plage de {total} points : au-delà du plafond de l'API paginée ({API_WINDOW})"
                )));
            }

            let count = page.results.len() as u64;
            for record in page.results {
                match record.into_measurement(region) {
                    Ok(measurement) => measurements.push(measurement),
                    // Production locale nulle sur ce créneau → intensité indéfinie.
                    Err(SourceError::NoData(_)) => {}
                    Err(other) => return Err(other),
                }
            }
            offset += count;

            if count < PAGE_SIZE || offset >= total || offset >= API_WINDOW {
                break;
            }
        }
        Ok(measurements)
    }
}

#[async_trait]
impl Eco2mixSource for OdreClient {
    async fn latest(&self, region: Region) -> Result<Measurement, SourceError> {
        // Régional : pas de taux_co2 publié → intensité dérivée `acv-ademe`.
        if region != Region::National {
            return self.latest_regional(region).await;
        }

        let query = [
            ("where", "taux_co2 is not null".to_string()),
            ("order_by", "date_heure desc".to_string()),
            ("limit", "1".to_string()),
        ];
        let page = self
            .fetch::<NationalRecord>(NATIONAL_DATASET, &query)
            .await?;

        page.results
            .into_iter()
            .next()
            .ok_or(SourceError::NoData(region))?
            .into_measurement()
    }

    async fn range(
        &self,
        region: Region,
        range: TimeRange,
    ) -> Result<Vec<Measurement>, SourceError> {
        if region != Region::National {
            return self.range_regional(region, range).await;
        }

        let filter = Self::time_filter(range)?;

        let mut measurements = Vec::new();
        let mut offset = 0u64;
        loop {
            let query = [
                ("where", filter.clone()),
                ("order_by", "date_heure asc".to_string()),
                ("limit", PAGE_SIZE.to_string()),
                ("offset", offset.to_string()),
            ];
            let page = self
                .fetch::<NationalRecord>(NATIONAL_DATASET, &query)
                .await?;
            let total = page.total_count;

            // Refus explicite plutôt que troncature silencieuse : au-delà du
            // plafond de l'API paginée, c'est l'export de masse qu'il faut.
            if offset == 0 && total > API_WINDOW {
                return Err(SourceError::Unavailable(format!(
                    "plage de {total} points : au-delà du plafond de l'API paginée \
                     ({API_WINDOW}) — utiliser l'export de masse ODRÉ"
                )));
            }

            let count = page.results.len() as u64;
            for record in page.results {
                measurements.push(record.into_measurement()?);
            }
            offset += count;

            if count < PAGE_SIZE || offset >= total || offset >= API_WINDOW {
                break;
            }
        }

        Ok(measurements)
    }
}

#[async_trait]
impl ConsumptionSource for OdreClient {
    async fn recent_loads(&self, region: Region) -> Result<Vec<LoadRecord>, SourceError> {
        // Charge nationale uniquement (le modèle ajusté est national `rte-direct`,
        // ADR-0011 §4). Régional : pas de charge ingérée → vide (pas d'ajustement).
        if region != Region::National {
            return Ok(Vec::new());
        }
        // Les ~100 horodatages les plus récents : tri décroissant → les créneaux
        // **futurs** (prévision sans réalisée) en tête, puis le passé proche.
        let query = [
            (
                "where",
                "consommation is not null or prevision_j1 is not null".to_string(),
            ),
            ("order_by", "date_heure desc".to_string()),
            ("limit", PAGE_SIZE.to_string()),
        ];
        let page = self
            .fetch::<ConsumptionRecord>(NATIONAL_DATASET, &query)
            .await?;
        let mut loads = Vec::with_capacity(page.results.len());
        for record in page.results {
            if let Some(load) = record.into_load(region)? {
                loads.push(load);
            }
        }
        Ok(loads)
    }
}

#[async_trait]
impl Eco2mixArchive for OdreClient {
    async fn export_national(&self, range: TimeRange) -> Result<Vec<Measurement>, SourceError> {
        let filter = Self::time_filter(range)?;
        let records = self.fetch_export(NATIONAL_ARCHIVE_DATASET, &filter).await?;
        // L'export n'est pas trié ; le tri est garanti à la lecture (repository).
        records
            .into_iter()
            .map(NationalRecord::into_measurement)
            .collect()
    }

    async fn export_national_loads(
        &self,
        range: TimeRange,
    ) -> Result<Vec<LoadRecord>, SourceError> {
        let start = range
            .start()
            .format(&Rfc3339)
            .map_err(|e| SourceError::Invalid(format!("borne de début : {e}")))?;
        let end = range
            .end()
            .format(&Rfc3339)
            .map_err(|e| SourceError::Invalid(format!("borne de fin : {e}")))?;
        let filter = format!(
            "date_heure >= '{start}' and date_heure < '{end}' and consommation is not null"
        );
        let records = self.fetch_export(NATIONAL_ARCHIVE_DATASET, &filter).await?;
        let mut loads = Vec::with_capacity(records.len());
        for record in records {
            if let Some(load) = record.into_realized_load()? {
                loads.push(load);
            }
        }
        Ok(loads)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_url_is_well_formed() {
        let client = OdreClient::new().expect("client");
        assert_eq!(
            client.records_url(NATIONAL_DATASET),
            "https://odre.opendatasoft.com/api/explore/v2.1/catalog/datasets/eco2mix-national-tr/records"
        );
    }

    #[test]
    fn region_refine_uses_insee_code() {
        assert_eq!(
            OdreClient::region_refine(Region::Bretagne).unwrap(),
            "code_insee_region:\"53\""
        );
        // National n'a pas de code régional.
        assert!(matches!(
            OdreClient::region_refine(Region::National),
            Err(SourceError::NoData(Region::National))
        ));
    }
}
