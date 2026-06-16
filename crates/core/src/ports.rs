//! Ports : les frontières du domaine avec le monde extérieur.
//!
//! Le domaine *définit* ces traits ; les adapters les *implémentent*. Aucune
//! implémentation concrète (HTTP, SQL, …) ne vit ici (règle d'or, ADR-0002).

use async_trait::async_trait;
use thiserror::Error;
use time::{Date, Duration, OffsetDateTime};

use crate::domain::{
    CrossBorderSnapshot, ForecastPoint, Granularity, IntensityStats, LoadRecord, Measurement,
    Region, RollupBucket, Subscription, TimeRange, VisitStats, WeatherForecast,
};

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

    /// Charges **réalisées** nationales historiques (consommation) sur `range`,
    /// par export de masse — pour calibrer `climatology@2` (ADR-0011 §4).
    async fn export_national_loads(&self, range: TimeRange)
    -> Result<Vec<LoadRecord>, SourceError>;
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

/// Port sortant : store de **charge** (consommation réalisée + prévue, ADR-0011
/// §4). Distinct du repository d'intensité car la charge n'est pas du carbone et
/// la prévision future n'a pas d'intensité.
#[async_trait]
pub trait ConsumptionRepository: Send + Sync {
    /// Insère/met à jour des charges. Un champ `None` ne doit pas écraser une
    /// valeur déjà présente (réalisée et prévue arrivent séparément).
    async fn upsert_loads(&self, loads: &[LoadRecord]) -> Result<usize, RepositoryError>;

    /// Charges d'une région sur `range`, triées par horodatage croissant.
    async fn load_range(
        &self,
        region: Region,
        range: TimeRange,
    ) -> Result<Vec<LoadRecord>, RepositoryError>;
}

/// Port sortant : source amont de **charge** (consommation récente + prévision
/// J-1/J), pour alimenter le store. Jamais appelée par requête utilisateur — le
/// poller l'ingère (ADR-0003).
#[async_trait]
pub trait ConsumptionSource: Send + Sync {
    /// Charges récentes et **à venir** (réalisées + prévues) pour une région.
    async fn recent_loads(&self, region: Region) -> Result<Vec<LoadRecord>, SourceError>;
}

/// Port sortant : store de **prévision météo** nationale (ADR-0012). Daté par
/// `(run_at, valid_at)` pour l'anti-fuite : on retrouve la prévision **telle
/// qu'elle était disponible** à un instant donné.
#[async_trait]
pub trait WeatherRepository: Send + Sync {
    /// Insère/met à jour des prévisions météo (clé `(valid_at, run_at)`).
    async fn upsert_weather(&self, forecasts: &[WeatherForecast])
    -> Result<usize, RepositoryError>;

    /// Prévisions dont le `valid_at` tombe dans `valid`, triées par
    /// `(valid_at, run_at)` croissants.
    async fn weather_range(
        &self,
        valid: TimeRange,
    ) -> Result<Vec<WeatherForecast>, RepositoryError>;
}

/// Port sortant : source amont de **prévision météo** (ADR-0012). Jamais appelée
/// par requête utilisateur — le poller l'ingère, comme ODRÉ.
#[async_trait]
pub trait WeatherForecastSource: Send + Sync {
    /// Prévision météo nationale **courante** (produite à l'instant de l'appel).
    async fn current_forecast(&self) -> Result<Vec<WeatherForecast>, SourceError>;
}

/// Port sortant : source du **contexte d'import** transfrontalier (ADR-0010 §5).
///
/// Fournit, au pas quart d'heure, les flux signés par frontière **et** l'intensité
/// carbone du voisin — l'entrée nécessaire au calcul `acv-ademe@2`
/// *consumption-based*. Source européenne (ENTSO-E) pour la souveraineté ; jamais
/// appelée par requête utilisateur — le poller l'ingère, comme ODRÉ.
#[async_trait]
pub trait CrossBorderSource: Send + Sync {
    /// Contexte d'import **récent** (national), du plus ancien au plus récent.
    async fn recent_flows(&self) -> Result<Vec<CrossBorderSnapshot>, SourceError>;
}

/// Port sortant : store du **contexte d'import** transfrontalier (ADR-0010 §6),
/// aligné au pas quart d'heure du mix pour le calcul `acv-ademe@2` à la lecture.
#[async_trait]
pub trait CrossBorderRepository: Send + Sync {
    /// Insère/met à jour des snapshots d'import (clé `at`).
    async fn upsert_flows(
        &self,
        snapshots: &[CrossBorderSnapshot],
    ) -> Result<usize, RepositoryError>;

    /// Snapshot d'import au plus proche de `at` (≤ `at`), s'il existe.
    async fn flows_at(
        &self,
        at: OffsetDateTime,
    ) -> Result<Option<CrossBorderSnapshot>, RepositoryError>;

    /// Snapshots d'import dont l'horodatage tombe dans `range`, triés par `at`
    /// croissant — pour dériver la série `acv-ademe@2` sur un intervalle.
    async fn flows_range(
        &self,
        range: TimeRange,
    ) -> Result<Vec<CrossBorderSnapshot>, RepositoryError>;
}

/// Niveau d'accès d'un appelant authentifié (ADR-0015). L'**anonyme** n'est pas
/// un tier : c'est l'**absence** de clé (géré au bord, jamais ici). Le payant
/// sera un tier additionnel, sans refonte (ADR-0015 §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiTier {
    Free,
}

/// Enregistrement d'une clé API résolue (ADR-0015). Type **de bord** : il vit
/// avec son port, pas dans le domaine carbone — le `core` n'apprend jamais qui
/// appelle (les cas d'usage ne prennent aucun principal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyRecord {
    pub tier: ApiTier,
    /// Libellé non-sensible (ex. nom du projet). **Jamais** la clé en clair.
    pub label: String,
}

/// Port sortant : **registre des clés API** (tier hébergé, ADR-0015).
///
/// Ne manipule que l'**empreinte** d'une clé (hachée par l'adapter entrant),
/// jamais la clé en clair. Le `core` ne dépend pas de l'auth : ce port n'est
/// consommé que par le **middleware de bord** (`adapter-http`), pas par les cas
/// d'usage — l'identité reste une préoccupation d'infrastructure.
#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    /// Résout l'empreinte d'une clé présentée. `None` si inconnue (→ 401 au bord).
    async fn resolve(&self, key_hash: &str) -> Result<Option<ApiKeyRecord>, RepositoryError>;

    /// Enregistre une nouvelle clé (empreinte + tier + libellé). Idempotent sur
    /// l'empreinte.
    async fn insert_key(
        &self,
        key_hash: &str,
        tier: ApiTier,
        label: &str,
    ) -> Result<(), RepositoryError>;
}

/// Port sortant : **registre des abonnements webhook** (ADR-0016). Possédés par
/// une clé (empreinte). Consommé par les endpoints de gestion **et** par le
/// watcher de fond (`active`).
#[async_trait]
pub trait SubscriptionRepository: Send + Sync {
    /// Crée un abonnement.
    async fn create(&self, subscription: &Subscription) -> Result<(), RepositoryError>;

    /// Liste les abonnements d'un propriétaire (par empreinte de clé).
    async fn list_for_owner(
        &self,
        owner_key_hash: &str,
    ) -> Result<Vec<Subscription>, RepositoryError>;

    /// Supprime un abonnement **possédé par** `owner_key_hash`. `true` si une
    /// ligne a été supprimée (sinon : inexistant ou non possédé → pas de fuite).
    async fn delete(&self, id: &str, owner_key_hash: &str) -> Result<bool, RepositoryError>;

    /// Tous les abonnements actifs (pour l'évaluation par le watcher).
    async fn active(&self) -> Result<Vec<Subscription>, RepositoryError>;
}

/// Une livraison de webhook prête à émettre : corps JSON + signature HMAC.
#[derive(Debug, Clone)]
pub struct WebhookDelivery {
    pub url: String,
    pub body: String,
    /// Signature `sha256=<hex>` pour l'en-tête `X-Carbonfr-Signature`.
    pub signature: String,
}

/// Port sortant : **émission** d'une livraison de webhook (ADR-0016). La seule
/// frontière par laquelle `carbon-fr` fait une requête **sortante** ; l'adapter
/// re-valide l'IP à la résolution (anti-SSRF TOCTOU) avant d'émettre.
#[async_trait]
pub trait Notifier: Send + Sync {
    /// Émet la livraison. `Err` si l'endpoint échoue (l'appelant gère retries /
    /// désactivation).
    async fn deliver(&self, delivery: &WebhookDelivery) -> Result<(), SourceError>;
}

/// Port sortant : compteur de consultations (visiteurs).
///
/// Ne reçoit qu'une **clé visiteur déjà anonymisée** (hachée par l'adapter
/// entrant) : aucune donnée personnelle ne franchit cette frontière.
#[async_trait]
pub trait VisitCounter: Send + Sync {
    /// Enregistre une visite pour `day`, dédupliquée par `(visitor, day)`.
    /// Retourne les statistiques à jour.
    async fn record_visit(&self, visitor: &str, day: Date) -> Result<VisitStats, RepositoryError>;

    /// Statistiques de consultation courantes.
    async fn visit_stats(&self) -> Result<VisitStats, RepositoryError>;
}

/// Port sortant : modèle de prévision d'intensité carbone (phase 3, ADR-0009).
#[async_trait]
pub trait ForecastModel: Send + Sync {
    /// Prévision à pas régulier sur `horizon`, à partir de `from`, pour la série
    /// `(region, methodology_id)`. La méthodologie est explicite : on prévoit une
    /// méthode précise (`rte-direct`, `acv-ademe`…), pas une intensité générique.
    ///
    /// Renvoie des [`ForecastPoint`] (estimation + intervalle + modèle), **pas**
    /// des `Measurement` : une prévision n'est ni une observation ni un millésime
    /// (ADR-0011).
    async fn forecast(
        &self,
        region: Region,
        methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError>;
}

/// Port sortant : source de temps (testabilité — l'instant peut être figé).
pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}
