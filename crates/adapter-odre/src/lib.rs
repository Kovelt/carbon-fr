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
pub mod quota;

use async_trait::async_trait;
use carbonfr_core::domain::{LoadRecord, Measurement, Region, TimeRange};
use carbonfr_core::ports::{ConsumptionSource, Eco2mixArchive, Eco2mixSource, SourceError};
use serde::de::DeserializeOwned;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use dto::{ConsumptionRecord, NationalRecord, RecordsResponse, RegionalRecord};
pub use quota::{DatasetQuota, QuotaTracker};

/// URL de base de l'API Explore d'ODRÉ.
const DEFAULT_BASE_URL: &str = "https://odre.opendatasoft.com";
/// Dataset éCO2mix national temps réel.
const NATIONAL_DATASET: &str = "eco2mix-national-tr";
/// Dataset éCO2mix régional temps réel (intensité dérivée, ADR-0008).
const REGIONAL_DATASET: &str = "eco2mix-regional-tr";
/// Dataset éCO2mix national consolidé + définitif (historique, ADR-0003).
const NATIONAL_ARCHIVE_DATASET: &str = "eco2mix-national-cons-def";
/// Dataset éCO2mix régional consolidé + définitif (historique régional,
/// ADR-0003 addendum 2026-09-26) — pas 30 min (contre 15 min en temps réel).
const REGIONAL_ARCHIVE_DATASET: &str = "eco2mix-regional-cons-def";
/// Plafond de pagination de l'API ODS v2.1 (`offset + limit ≤ 10 000`).
const API_WINDOW: u64 = 10_000;
/// Taille de page (maximum autorisé par l'API `records`).
const PAGE_SIZE: u64 = 100;
/// Colonnes retenues sur l'export de masse régional (ADR-0003 addendum
/// 2026-09-26) : un enregistrement complet pèse ~700 o avec le détail
/// `tco_*`/`tch_*` — un `select` explicite borne la taille du corps téléchargé
/// (jusqu'à ~35 000 lignes pour une tranche de 30 j en temps réel).
const REGIONAL_EXPORT_SELECT: &str = "date_heure,nature,code_insee_region,consommation,\
    thermique,nucleaire,eolien,solaire,hydraulique,bioenergies,ech_physiques";
/// Timeout **par requête** des exports de masse, plus large que le timeout
/// global du client (30 s, [`OdreClient::new`]) : un export régional d'une
/// tranche de 30 j en temps réel (~35 000 lignes) peut le dépasser. N'affecte
/// que les appels `exports/json` — les appels `records` du poller gardent le
/// timeout global.
const EXPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// Installe le provider crypto `ring` de rustls comme provider par défaut du
/// **processus**, si aucun n'est déjà en place.
///
/// reqwest 0.13 (feature `rustls-no-provider`, cf. Cargo.toml racine) ne tire
/// plus `aws-lc-rs` : sans provider installé, `reqwest::Client::builder().build()`
/// **panique**, y compris pour un usage HTTP en clair (la pile TLS est montée
/// dès `.build()`). Un seul provider dans tout le workspace (ADR-0031 décision
/// 3 : pas de double provider) → `ring`, déjà celui de sqlx (`tls-rustls-ring`).
/// Appelé défensivement ici (pas seulement au bootstrap de `bin/server`, cf.
/// `main.rs`) pour que cette crate reste utilisable seule — tests de ce crate,
/// ou toute autre intégration qui ne passerait pas par `carbonfr-server`.
/// `install_default()` renvoie `Err` si un provider est déjà installé (par le
/// bootstrap du binaire, ou par un appel concurrent depuis un autre adapter) :
/// sans conséquence, on l'ignore — c'est forcément le même `ring`, seul
/// provider présent dans le graphe de dépendances du workspace. Le `Once`
/// évite juste de reconstruire un `CryptoProvider` (allocation) à chaque appel.
fn ensure_crypto_provider() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Client de la source éCO2mix d'ODRÉ.
///
/// Sans état métier : un seul client peut être partagé (`reqwest::Client` gère
/// son propre pool de connexions et est `Clone`).
#[derive(Clone)]
pub struct OdreClient {
    http: reqwest::Client,
    base_url: String,
    /// Source d'archive choisie (consolidée par défaut) : détermine à la fois
    /// le jeu **national** et le jeu **régional** exportés par
    /// [`Eco2mixArchive`] (backfill, ADR-0003 addendum 2026-09-26).
    archive_source: ArchiveSource,
    /// Observation opportuniste du quota ODRÉ réel par jeu de données (ADR-0022
    /// addendum 2026-09-26). Partagé entre les clones (`Arc` interne) : un
    /// même traceur peut être branché sur plusieurs clients du même processus
    /// via [`with_quota_tracker`](Self::with_quota_tracker).
    quota: quota::QuotaTracker,
}

/// Jeu de données éCO2mix national exporté par le backfill ([`Eco2mixArchive`]).
///
/// Le jeu **consolidé + définitif** est la référence (ADR-0003), mais RTE le
/// publie avec environ trois mois de retard. Pour combler un trou plus récent
/// (panne de la collecte), le jeu **temps réel** s'exporte de la même façon ;
/// ses mesures portent le millésime `tr` et seront remplacées d'elles-mêmes par
/// les consolidées lors d'un backfill ultérieur (upsert conditionnel au
/// millésime, ADR-0006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArchiveSource {
    /// `eco2mix-national-cons-def` (défaut).
    #[default]
    Consolidated,
    /// `eco2mix-national-tr`.
    Realtime,
}

impl ArchiveSource {
    /// Jeu national exporté par le backfill.
    fn dataset(self) -> &'static str {
        match self {
            Self::Consolidated => NATIONAL_ARCHIVE_DATASET,
            Self::Realtime => NATIONAL_DATASET,
        }
    }

    /// Jeu **régional** exporté par le backfill régional (`export_regional`,
    /// ADR-0003 addendum 2026-09-26) — même logique consolidé/temps réel que
    /// [`dataset`](Self::dataset), sur le jeu régional correspondant.
    fn regional_dataset(self) -> &'static str {
        match self {
            Self::Consolidated => REGIONAL_ARCHIVE_DATASET,
            Self::Realtime => REGIONAL_DATASET,
        }
    }
}

impl OdreClient {
    /// Construit un client visant l'API publique d'ODRÉ.
    pub fn new() -> Result<Self, SourceError> {
        ensure_crypto_provider();
        let http = reqwest::Client::builder()
            .user_agent(concat!("carbon-fr/", env!("CARGO_PKG_VERSION")))
            // Bornes de temps : sans elles, une source amont qui *pend* bloquerait
            // le poller indéfiniment (ingestion gelée, donnée figée sans erreur).
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
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
            archive_source: ArchiveSource::default(),
            quota: quota::QuotaTracker::new(),
        }
    }

    /// Choisit le jeu (national **et** régional) exporté par le backfill
    /// (consolidé par défaut, cf. [`ArchiveSource`]).
    pub fn with_archive_source(mut self, source: ArchiveSource) -> Self {
        self.archive_source = source;
        self
    }

    /// Branche un [`QuotaTracker`] externe (partagé entre
    /// plusieurs clients du même processus, ex. poller + auto-réparation dans
    /// `bin/server`) à la place de celui créé par défaut.
    pub fn with_quota_tracker(mut self, tracker: quota::QuotaTracker) -> Self {
        self.quota = tracker;
        self
    }

    /// Dernières observations connues du quota ODRÉ réel (opportuniste, cf.
    /// module [`quota`]).
    pub fn quota(&self) -> &quota::QuotaTracker {
        &self.quota
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

        // Observation opportuniste du quota réel (ADR-0022 addendum 2026-09-26) :
        // AVANT le contrôle de statut, à dessein — un 429 (quota dépassé) porte les
        // mêmes en-têtes `x-ratelimit-dataset-*` (souvent `remaining: 0`), c'est le
        // cas le plus utile à capter. Lire les en-têtes ne consomme pas le corps —
        // 0 appel HTTP de plus, quelle que soit l'issue de la requête.
        self.quota
            .observe(dataset, resp.headers(), OffsetDateTime::now_utc());

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
    ///
    /// Générique sur le type de sortie `T` (national ou régional) et sur les
    /// paramètres de requête additionnels (`query`, ex. `select` pour borner la
    /// taille du corps régional, ADR-0003 addendum 2026-09-26) : `query` porte
    /// déjà `where`, ajouté par l'appelant. Timeout [`EXPORT_TIMEOUT`], plus
    /// large que le timeout global du client — sans changement pour les appels
    /// `records` du poller.
    async fn fetch_export<T: DeserializeOwned>(
        &self,
        dataset: &str,
        query: &[(&str, &str)],
    ) -> Result<Vec<T>, SourceError> {
        let url = format!(
            "{}/api/explore/v2.1/catalog/datasets/{dataset}/exports/json",
            self.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .get(url)
            .query(query)
            .timeout(EXPORT_TIMEOUT)
            .send()
            .await
            .map_err(|e| SourceError::Unavailable(format!("export ODRÉ : {e}")))?;

        // Cf. `fetch` : observation opportuniste AVANT le contrôle de statut, mêmes
        // en-têtes de quota sur l'export que sur `records` (y compris sur un 429).
        self.quota
            .observe(dataset, resp.headers(), OffsetDateTime::now_utc());

        if !resp.status().is_success() {
            return Err(SourceError::Unavailable(format!(
                "ODRÉ (export) a répondu {}",
                resp.status()
            )));
        }

        resp.json::<Vec<T>>()
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
                    // Comme au national : une ligne invalide est ignorée, pas le lot.
                    Err(err) => tracing::warn!(
                        region = region.slug(),
                        error = %err,
                        "enregistrement ODRÉ régional invalide ignoré"
                    ),
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
                // Une ligne invalide (valeur hors domaine, horodatage illisible)
                // ne doit pas faire perdre toute la fenêtre du poller : on
                // l'ignore et on le journalise.
                match record.into_measurement() {
                    Ok(measurement) => measurements.push(measurement),
                    Err(err) => {
                        tracing::warn!(error = %err, "enregistrement ODRÉ national invalide ignoré")
                    }
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
        let dataset = self.archive_source.dataset();
        let records: Vec<NationalRecord> =
            self.fetch_export(dataset, &[("where", &filter)]).await?;
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
        let dataset = self.archive_source.dataset();
        let records: Vec<NationalRecord> =
            self.fetch_export(dataset, &[("where", &filter)]).await?;
        let mut loads = Vec::with_capacity(records.len());
        for record in records {
            if let Some(load) = record.into_realized_load()? {
                loads.push(load);
            }
        }
        Ok(loads)
    }

    /// Export de masse régional (item PROD-1, ADR-0003 addendum 2026-09-26) :
    /// **un** téléchargement couvre les 12 régions métropolitaines (pas de
    /// `refine` par région, contrairement à `latest_regional`/`range_regional`) ;
    /// chaque enregistrement porte son propre `code_insee_region`, résolu ici en
    /// [`Region`]. `select` (cf. `REGIONAL_EXPORT_SELECT`) borne la taille du
    /// corps téléchargé.
    async fn export_regional(&self, range: TimeRange) -> Result<Vec<Measurement>, SourceError> {
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
        let dataset = self.archive_source.regional_dataset();
        let records: Vec<RegionalRecord> = self
            .fetch_export(
                dataset,
                &[("where", &filter), ("select", REGIONAL_EXPORT_SELECT)],
            )
            .await?;

        let mut measurements = Vec::with_capacity(records.len());
        for record in records {
            let code = record.code_insee_region.clone();
            let region = match code.as_deref().and_then(Region::from_insee_code) {
                Some(region) => region,
                // Code absent, ou hors périmètre métropolitain (Corse, DOM-TOM,
                // cf. ADR-0003) : ignoré, jamais une erreur — l'export couvre
                // toutes les régions publiées, pas seulement les 12 attendues.
                None => {
                    tracing::debug!(
                        code_insee_region = ?code,
                        "enregistrement régional (export) hors périmètre métropolitain ignoré"
                    );
                    continue;
                }
            };
            match record.into_measurement(region) {
                Ok(measurement) => measurements.push(measurement),
                // Production locale nulle sur ce créneau → intensité indéfinie
                // (comme `range_regional`).
                Err(SourceError::NoData(_)) => {}
                Err(err) => tracing::warn!(
                    region = region.slug(),
                    error = %err,
                    "enregistrement ODRÉ régional (export) invalide ignoré"
                ),
            }
        }
        Ok(measurements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_source_selects_dataset() {
        let client = OdreClient::new().expect("client");
        assert_eq!(client.archive_source, ArchiveSource::Consolidated);
        assert_eq!(client.archive_source.dataset(), "eco2mix-national-cons-def");
        assert_eq!(
            client.archive_source.regional_dataset(),
            "eco2mix-regional-cons-def"
        );

        let client = client.with_archive_source(ArchiveSource::Realtime);
        assert_eq!(client.archive_source, ArchiveSource::Realtime);
        assert_eq!(client.archive_source.dataset(), "eco2mix-national-tr");
        assert_eq!(
            client.archive_source.regional_dataset(),
            "eco2mix-regional-tr"
        );

        let client = client.with_archive_source(ArchiveSource::Consolidated);
        assert_eq!(client.archive_source.dataset(), "eco2mix-national-cons-def");
    }

    /// Construction des paramètres de l'export régional : `where` filtre sur
    /// l'intervalle (`consommation is not null`, comme `range_regional`) et
    /// `select` liste exactement les colonnes décodées par [`RegionalRecord`]
    /// (ADR-0003 addendum 2026-09-26).
    #[test]
    fn export_regional_query_is_well_formed() {
        let t0 = OffsetDateTime::from_unix_timestamp(0).unwrap();
        let range = TimeRange::new(t0, t0 + time::Duration::hours(1)).unwrap();
        let start = range.start().format(&Rfc3339).unwrap();
        let end = range.end().format(&Rfc3339).unwrap();
        let filter = format!(
            "date_heure >= '{start}' and date_heure < '{end}' and consommation is not null"
        );

        assert_eq!(
            filter,
            "date_heure >= '1970-01-01T00:00:00Z' and date_heure < '1970-01-01T01:00:00Z' \
             and consommation is not null"
        );
        assert_eq!(
            REGIONAL_EXPORT_SELECT,
            "date_heure,nature,code_insee_region,consommation,\
             thermique,nucleaire,eolien,solaire,hydraulique,bioenergies,ech_physiques"
        );
    }

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

    /// Démarre `app` sur `127.0.0.1:<port éphémère>` et rend l'URL de base
    /// (`http://127.0.0.1:<port>`) ainsi que le `JoinHandle` de la tâche
    /// serveur (abandonnée — donc annulée — à la fin du test avec le runtime
    /// `#[tokio::test]`, pas besoin d'arrêt explicite). Même approche que
    /// `crates/adapter-webhook`.
    async fn spawn_server(app: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind du serveur de test");
        let addr = listener.local_addr().expect("adresse locale");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), handle)
    }

    /// Bout en bout, sans réseau réel : un serveur `axum` local répond comme
    /// ODRÉ (en-têtes de quota + corps `records` minimal), un `OdreClient`
    /// pointé dessus fait un appel public normal (`latest`), et on vérifie que
    /// le quota a bien été observé — 0 appel ODRÉ réel, aucune dépendance
    /// réseau externe.
    #[tokio::test]
    async fn latest_observes_quota_headers_from_response() {
        let app = axum::Router::new().route(
            "/api/explore/v2.1/catalog/datasets/eco2mix-national-tr/records",
            axum::routing::get(|| async {
                let mut headers = axum::http::HeaderMap::new();
                headers.insert("x-ratelimit-dataset-limit", "50000".parse().unwrap());
                headers.insert("x-ratelimit-dataset-remaining", "49977".parse().unwrap());
                headers.insert(
                    "x-ratelimit-dataset-reset",
                    "2026-10-01 00:00:00+00:00".parse().unwrap(),
                );
                headers.insert("x-ratelimit-limit", "10000000".parse().unwrap());
                headers.insert("x-ratelimit-remaining", "9999959".parse().unwrap());
                (
                    headers,
                    axum::Json(serde_json::json!({ "total_count": 0, "results": [] })),
                )
            }),
        );
        let (base_url, _server) = spawn_server(app).await;

        let client = OdreClient::with_http(reqwest::Client::new(), base_url);

        // Résultats vides → `latest` échoue avec `SourceError::NoData` : sans
        // conséquence, seule l'observation du quota nous intéresse ici (les
        // en-têtes sont lus AVANT que le corps ne soit consommé).
        let result = client.latest(Region::National).await;
        assert!(matches!(result, Err(SourceError::NoData(Region::National))));

        let snapshot = client.quota().snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].dataset, NATIONAL_DATASET);
        assert_eq!(snapshot[0].limit, 50_000);
        assert_eq!(snapshot[0].remaining, 49_977);
        assert!(snapshot[0].reset_unix.is_some());
    }

    /// Bout en bout, sans réseau réel (même patron que
    /// `latest_observes_quota_headers_from_response`) : un serveur `axum` local
    /// répond comme l'export de masse régional d'ODRÉ — un tableau JSON brut de
    /// 3 enregistrements (région 84 valide, région 84 sans production locale,
    /// code INSEE inconnu « 94 ») — pointé par `OdreClient::export_regional`
    /// (item PROD-1, ADR-0003 addendum 2026-09-26). Vérifie que seule la ligne
    /// valide produit une mesure, région et méthodologie résolues.
    #[tokio::test]
    async fn export_regional_resolves_region_and_skips_invalid_rows() {
        use carbonfr_core::domain::{Methodology, Vintage};

        let app = axum::Router::new().route(
            "/api/explore/v2.1/catalog/datasets/eco2mix-regional-cons-def/exports/json",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!([
                    {
                        "date_heure": "2026-06-30T21:30:00+00:00",
                        "nature": "Données consolidées",
                        "code_insee_region": "84",
                        "consommation": 7122,
                        "thermique": 376,
                        "nucleaire": 4555,
                        "eolien": "115",
                        "solaire": 0,
                        "hydraulique": 5098,
                        "bioenergies": 111,
                        "ech_physiques": -3129
                    },
                    {
                        "date_heure": "2026-06-30T22:00:00+00:00",
                        "nature": "Données consolidées",
                        "code_insee_region": "84",
                        "consommation": 100,
                        "thermique": 0,
                        "nucleaire": 0,
                        "eolien": 0,
                        "solaire": 0,
                        "hydraulique": 0,
                        "bioenergies": 0,
                        "ech_physiques": 100
                    },
                    {
                        "date_heure": "2026-06-30T21:30:00+00:00",
                        "nature": "Données consolidées",
                        "code_insee_region": "94",
                        "consommation": 500,
                        "thermique": 10,
                        "nucleaire": 0,
                        "eolien": 0,
                        "solaire": 0,
                        "hydraulique": 0,
                        "bioenergies": 0,
                        "ech_physiques": 500
                    }
                ]))
            }),
        );
        let (base_url, _server) = spawn_server(app).await;
        let client = OdreClient::with_http(reqwest::Client::new(), base_url);

        let t0 = OffsetDateTime::from_unix_timestamp(0).unwrap();
        let range = TimeRange::new(t0, t0 + time::Duration::hours(1)).unwrap();
        let measurements = client
            .export_regional(range)
            .await
            .expect("export régional");

        // La 2ᵉ ligne (production locale nulle → NoData) et la 3ᵉ (code INSEE
        // « 94 », hors périmètre métropolitain) sont ignorées silencieusement.
        assert_eq!(measurements.len(), 1);
        let m = &measurements[0];
        assert_eq!(m.region, Region::AuvergneRhoneAlpes);
        assert_eq!(m.methodology, Methodology::acv_ademe());
        assert_eq!(m.vintage, Vintage::Consolidated);
    }
}
