//! Ports : les frontières du domaine avec le monde extérieur.
//!
//! Le domaine *définit* ces traits ; les adapters les *implémentent*. Aucune
//! implémentation concrète (HTTP, SQL, …) ne vit ici (règle d'or, ADR-0002).

use async_trait::async_trait;
use thiserror::Error;
use time::{Duration, OffsetDateTime};

use crate::domain::{Granularity, IntensityStats, Measurement, Region, RollupBucket, TimeRange};

/// Erreur de récupération depuis une source amont (ODRÉ, ou source de secours).
#[derive(Debug, Error)]
pub enum SourceError {
    #[error("aucune donnée disponible pour la région {0}")]
    NoData(Region),
    #[error("source indisponible : {0}")]
    Unavailable(String),
    #[error("réponse de la source invalide : {0}")]
    Invalid(String),
}

/// Erreur de persistance.
#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("erreur de stockage : {0}")]
    Backend(String),
}

/// Erreur du modèle de prévision.
#[derive(Debug, Error)]
pub enum ForecastError {
    #[error("prévision indisponible : {0}")]
    Unavailable(String),
    #[error("données insuffisantes pour prévoir")]
    NotEnoughData,
}

/// Port sortant : récupération de la donnée carbone amont (RTE/ODRÉ ou
/// secours). Voir ADR-0003.
#[async_trait]
pub trait Eco2mixSource: Send + Sync {
    /// Dernière mesure disponible pour une région.
    async fn latest(&self, region: Region) -> Result<Measurement, SourceError>;

    /// Mesures sur un intervalle restreint (API paginée — pour le rattrapage de
    /// courts trous, pas le backfill historique massif : voir [`Eco2mixArchive`]).
    async fn range(
        &self,
        region: Region,
        range: TimeRange,
    ) -> Result<Vec<Measurement>, SourceError>;
}

/// Port sortant : export de masse de l'historique (ADR-0003).
///
/// Distinct d'[`Eco2mixSource`] : l'historique se rapatrie par **un
/// téléchargement** (export du jeu consolidé/définitif), jamais en parcourant
/// l'API paginée — qui brûlerait le quota. Le découpage temporel de la requête
/// est porté par l'appelant (le cas d'usage de backfill), pour borner la taille
/// de chaque export.
#[async_trait]
pub trait Eco2mixArchive: Send + Sync {
    /// Mesures nationales historiques sur `range`, obtenues par export de masse.
    async fn export_national(&self, range: TimeRange) -> Result<Vec<Measurement>, SourceError>;
}

/// Port sortant : persistance des mesures (read-model + historique).
///
/// L'écriture est un **upsert conditionnel au millésime** (ADR-0006) :
/// l'implémentation ne remplace une mesure de clé `(region, at, methodology)`
/// que si le `vintage` entrant est de qualité supérieure ou égale.
#[async_trait]
pub trait IntensityRepository: Send + Sync {
    /// Insère ou met à jour des mesures selon la règle de millésime.
    /// Retourne le nombre de lignes effectivement écrites ou mises à jour.
    async fn upsert_many(&self, measurements: &[Measurement]) -> Result<usize, RepositoryError>;

    /// Dernière mesure connue (meilleur millésime) pour une région et une
    /// méthodologie.
    async fn latest(
        &self,
        region: Region,
        methodology_id: &str,
    ) -> Result<Option<Measurement>, RepositoryError>;

    /// Mesures sur un intervalle, triées par horodatage croissant.
    async fn range(
        &self,
        region: Region,
        methodology_id: &str,
        range: TimeRange,
    ) -> Result<Vec<Measurement>, RepositoryError>;

    /// Statistiques (moyenne/min/max/effectif) sur un intervalle, calculées sur
    /// les mesures brutes. `None` si l'intervalle ne contient aucune donnée.
    async fn stats(
        &self,
        region: Region,
        methodology_id: &str,
        range: TimeRange,
    ) -> Result<Option<IntensityStats>, RepositoryError>;

    /// Série agrégée par pas (`granularity`) sur un intervalle, servie depuis les
    /// rollups (vues matérialisées). Triée par seau croissant.
    async fn rollup(
        &self,
        region: Region,
        methodology_id: &str,
        range: TimeRange,
        granularity: Granularity,
    ) -> Result<Vec<RollupBucket>, RepositoryError>;

    /// Rafraîchit les rollups après une ingestion ou un backfill. Sans effet
    /// pour les implémentations qui agrègent à la volée.
    async fn refresh_rollups(&self) -> Result<(), RepositoryError>;
}

/// Port sortant : modèle de prévision d'intensité carbone (phase 3).
#[async_trait]
pub trait ForecastModel: Send + Sync {
    /// Prévision à pas régulier sur `horizon`, à partir de `from`.
    async fn forecast(
        &self,
        region: Region,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<Measurement>, ForecastError>;
}

/// Port sortant : source de temps (testabilité — l'instant peut être figé).
pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}
