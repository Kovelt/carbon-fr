//! # carbonfr-adapter-forecast
//!
//! Adapter **sortant** : implémentation de [`ForecastModel`] par **climatologie**
//! (`climatology@1`, ADR-0009).
//!
//! Cet adapter ne porte **aucune logique métier** : il se contente de l'IO de
//! lecture — récupérer les `N` dernières semaines de la série `(region,
//! methodology_id)` via [`IntensityRepository`] — puis de **déléguer au calcul
//! pur** du domaine ([`climatology_forecast`]). La formule, elle, vit dans
//! `core` (testable sans IO).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use carbonfr_core::domain::{
    ClimatologyParams, EmissionFactors, ForecastPoint, HorizonBands, Region, TD_LOSS_FACTOR_V1,
    TimeRange, acv_ademe_forecast, climatology_forecast,
};
use carbonfr_core::ports::{
    Clock, CrossBorderRepository, ForecastError, ForecastModel, IntensityRepository,
};
use time::{Duration, OffsetDateTime};

/// Profondeur d'historique par défaut alimentant la climatologie.
/// **10 semaines glissantes** — valeur calée par backtest (addendum ADR-0009).
const DEFAULT_WEEKS: i64 = 10;

/// Pas d'alignement du seau de cache de [`CachedForecaster`] : la cadence
/// quart d'heure canonique des mesures éCO2mix. Les requêtes d'un même seau
/// partagent la série calculée (la donnée sous-jacente ne change qu'au cycle
/// du poller).
const CACHE_BUCKET_SECS: i64 = 900;

/// Plafond d'entrées du cache : la cardinalité réelle est minuscule
/// (13 régions × ~2 méthodologies × quelques horizons) — borne dure contre
/// toute dérive mémoire.
const CACHE_MAX_ENTRIES: usize = 256;

/// Horloge système : implémentation par défaut du port [`Clock`].
struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

/// Clé de cache d'une prévision : cible `(region, methodology)`, seau de
/// départ (`from` aligné sur le pas quart d'heure) et horizon.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    region: Region,
    methodology: String,
    from_bucket: i64,
    horizon_secs: i64,
}

/// Série mise en cache, datée pour l'expiration TTL.
struct CacheEntry {
    stored_at: std::time::Instant,
    points: Arc<Vec<ForecastPoint>>,
}

/// Décorateur **cache TTL** d'un [`ForecastModel`] (audit perf 2026-08).
///
/// Sans lui, chaque requête des cinq endpoints de prévision (`/forecast`,
/// `/greenest-window`, `/schedule`, `/schedule/slots`, `/below`) relisait
/// ~10 semaines d'historique en base et rebâtissait la climatologie, pour une
/// donnée qui ne change qu'au cycle du poller. La série calculée est donc
/// mémorisée par clé `(region, methodology, from aligné sur le pas, horizon)`
/// avec un TTL = intervalle de poll (`CARBONFR_POLL_SECS`).
///
/// Prudence de sémantique : **seul le trafic `from ≈ maintenant`** (le seau
/// quart d'heure courant — l'écrasante majorité, `from` vaut `now` par défaut)
/// est mis en cache. Un `from` arbitraire (passé ou futur) contourne le cache
/// et garde exactement le comportement d'avant — sans lui, la clé serait de
/// cardinalité non bornée. Les erreurs ne sont jamais mises en cache.
///
/// ADR-0009 intact : la prévision reste « calculée à la lecture, jamais
/// persistée » — le cache est un mémo **en mémoire**, sans millésime ni base.
#[derive(Clone)]
pub struct CachedForecaster<F> {
    inner: F,
    ttl: std::time::Duration,
    clock: Arc<dyn Clock>,
    cache: Arc<Mutex<HashMap<CacheKey, CacheEntry>>>,
}

impl<F> CachedForecaster<F> {
    /// Enrobe `inner` d'un cache d'une durée de vie `ttl` (l'intervalle de
    /// poll : au-delà, une nouvelle mesure a pu déplacer l'ancre du modèle).
    pub fn new(inner: F, ttl: std::time::Duration) -> Self {
        Self {
            inner,
            ttl,
            clock: Arc::new(SystemClock),
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Injecte une horloge (port [`Clock`]) — testabilité : l'instant qui
    /// décide de la mise en cache (`from ≈ maintenant`) peut être figé.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Verrou du cache, avec récupération d'un éventuel empoisonnement (le
    /// contenu reste cohérent : au pire une entrée expirée, filtrée au TTL).
    fn lock(&self) -> MutexGuard<'_, HashMap<CacheKey, CacheEntry>> {
        self.cache.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Série en cache pour `key`, si présente et non expirée.
    fn lookup(&self, key: &CacheKey) -> Option<Vec<ForecastPoint>> {
        let cache = self.lock();
        let entry = cache.get(key)?;
        (entry.stored_at.elapsed() < self.ttl).then(|| entry.points.as_ref().clone())
    }

    /// Mémorise la série : purge les entrées expirées, borne la taille (évince
    /// la plus ancienne au plafond), puis insère.
    fn store(&self, key: CacheKey, points: &[ForecastPoint]) {
        let mut cache = self.lock();
        cache.retain(|_, e| e.stored_at.elapsed() < self.ttl);
        if cache.len() >= CACHE_MAX_ENTRIES
            && let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, e)| e.stored_at)
                .map(|(k, _)| k.clone())
        {
            cache.remove(&oldest);
        }
        cache.insert(
            key,
            CacheEntry {
                stored_at: std::time::Instant::now(),
                points: Arc::new(points.to_vec()),
            },
        );
    }
}

/// Seau quart d'heure d'un horodatage (division euclidienne : stable aussi
/// avant l'époque Unix).
fn bucket(t: OffsetDateTime) -> i64 {
    t.unix_timestamp().div_euclid(CACHE_BUCKET_SECS)
}

#[async_trait]
impl<F: ForecastModel> ForecastModel for CachedForecaster<F> {
    async fn forecast(
        &self,
        region: Region,
        methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError> {
        // Seul le seau courant est mis en cache (cf. doc du type) : un `from`
        // explicite hors du quart d'heure en cours passe tout droit.
        if bucket(from) != bucket(self.clock.now()) {
            return self
                .inner
                .forecast(region, methodology_id, from, horizon)
                .await;
        }
        let key = CacheKey {
            region,
            methodology: methodology_id.to_string(),
            from_bucket: bucket(from),
            horizon_secs: horizon.whole_seconds(),
        };
        if let Some(points) = self.lookup(&key) {
            return Ok(points);
        }
        let points = self
            .inner
            .forecast(region, methodology_id, from, horizon)
            .await?;
        self.store(key, &points);
        Ok(points)
    }
}

/// Modèle de prévision `climatology@1` (ADR-0009) branché sur un repository.
///
/// Générique sur `R: IntensityRepository` → dispatch statique, zéro coût (comme
/// les cas d'usage du `core`). La *composition root* y câble le repository
/// Postgres concret. `Clone` quand `R` l'est (le pool Postgres l'est, à coût
/// négligeable) — requis pour le partage dans l'état de l'API.
#[derive(Clone)]
pub struct ClimatologyForecaster<R> {
    repo: R,
    weeks: i64,
    params: ClimatologyParams,
    /// Bandes d'incertitude par horizon (ADR-0011 §5), calibrées par backtest.
    /// `None` → intervalle de repli par dispersion de créneau.
    bands: Option<HorizonBands>,
}

impl<R> ClimatologyForecaster<R> {
    /// Construit avec les défauts calés (addendum ADR-0009) : 10 semaines
    /// d'historique ; pas 15 min ; τ = 2 semaines. Intervalles non calibrés
    /// (repli par créneau) tant que [`with_bands`](Self::with_bands) n'est pas
    /// appelé.
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            weeks: DEFAULT_WEEKS,
            params: ClimatologyParams::default(),
            bands: None,
        }
    }

    /// Surcharge la profondeur d'historique (en semaines, au moins 1) et les
    /// paramètres du modèle.
    pub fn with_config(repo: R, weeks: u32, params: ClimatologyParams) -> Self {
        Self {
            repo,
            weeks: weeks.max(1) as i64,
            params,
            bands: None,
        }
    }

    /// Injecte les bandes d'incertitude par horizon (calibrées par backtest,
    /// ADR-0011) : les intervalles s'élargiront alors avec l'horizon.
    pub fn with_bands(mut self, bands: HorizonBands) -> Self {
        self.bands = Some(bands);
        self
    }
}

#[async_trait]
impl<R: IntensityRepository> ForecastModel for ClimatologyForecaster<R> {
    async fn forecast(
        &self,
        region: Region,
        methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError> {
        // Fenêtre d'historique : [from − N semaines, from). Semi-ouverte → exclut
        // `from` et le futur : on ne nourrit la climatologie que d'observations
        // passées (la plus récente sert d'ancre de persistance).
        let history_start = from - Duration::days(self.weeks * 7);
        let window = TimeRange::new(history_start, from)
            .ok_or_else(|| ForecastError::Unavailable("fenêtre d'historique invalide".into()))?;

        let history = self
            .repo
            .range(region, methodology_id, window)
            .await
            .map_err(|e| ForecastError::Unavailable(e.to_string()))?;

        // None (historique vide / paramètres invalides) ou série vide → on ne
        // peut pas prévoir.
        climatology_forecast(&history, from, horizon, self.params, self.bands.as_ref())
            .filter(|points| !points.is_empty())
            .ok_or(ForecastError::NotEnoughData)
    }
}

/// Modèle de prévision **`acv-ademe@2`** (consumption-based, ADR-0013) :
/// climatologie des **entrées** (mix + contexte d'import) puis application du
/// calculateur pur `AcvAdeme`.
///
/// Cet adapter ne porte aucune logique métier : il lit l'historique du mix
/// (`acv-ademe@1`, via [`IntensityRepository`]) et du contexte d'import (via
/// [`CrossBorderRepository`]), puis **délègue au calcul pur** du domaine
/// ([`acv_ademe_forecast`]). **National** uniquement (ADR-0013 §8).
#[derive(Clone)]
pub struct AcvAdemeForecaster<R, C> {
    repo: R,
    cross_border: C,
    weeks: i64,
    params: ClimatologyParams,
    /// Bandes d'incertitude par horizon (ADR-0011 §5), calibrées par backtest
    /// `acv-ademe` ; `None` → repli sur la dispersion par créneau.
    bands: Option<HorizonBands>,
}

impl<R, C> AcvAdemeForecaster<R, C> {
    /// Construit avec les défauts calés (10 semaines d'historique, ADR-0009).
    pub fn new(repo: R, cross_border: C) -> Self {
        Self {
            repo,
            cross_border,
            weeks: DEFAULT_WEEKS,
            params: ClimatologyParams::default(),
            bands: None,
        }
    }

    /// Injecte les bandes d'incertitude par horizon (ADR-0013 §6) : les
    /// intervalles `@2` s'élargiront alors avec l'horizon.
    pub fn with_bands(mut self, bands: HorizonBands) -> Self {
        self.bands = Some(bands);
        self
    }
}

#[async_trait]
impl<R, C> ForecastModel for AcvAdemeForecaster<R, C>
where
    R: IntensityRepository,
    C: CrossBorderRepository,
{
    async fn forecast(
        &self,
        region: Region,
        _methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError> {
        if region != Region::National {
            return Err(ForecastError::Unavailable(
                "acv-ademe@2 (consommation) n'est prévu qu'au national".into(),
            ));
        }
        let history_start = from - Duration::days(self.weeks * 7);
        let window = TimeRange::new(history_start, from)
            .ok_or_else(|| ForecastError::Unavailable("fenêtre d'historique invalide".into()))?;

        // Mix FR : porté par les mesures `acv-ademe@1` ; contexte d'import : store
        // ENTSO-E. Les deux **tels que disponibles** (anti-fuite, ADR-0013 §7).
        let mix_history = self
            .repo
            .range(region, "acv-ademe", window)
            .await
            .map_err(|e| ForecastError::Unavailable(e.to_string()))?;
        let flow_history = self
            .cross_border
            .flows_range(window)
            .await
            .map_err(|e| ForecastError::Unavailable(e.to_string()))?;

        acv_ademe_forecast(
            &mix_history,
            &flow_history,
            from,
            horizon,
            self.params,
            &EmissionFactors::acv_ademe_v1(),
            TD_LOSS_FACTOR_V1,
            self.bands.as_ref(),
        )
        .filter(|points| !points.is_empty())
        .ok_or(ForecastError::NotEnoughData)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbonfr_core::domain::{
        CarbonIntensity, Granularity, HorizonBands, IntensityStats, Measurement, Methodology,
        RollupBucket, Vintage,
    };
    use carbonfr_core::ports::RepositoryError;

    /// Repository en mémoire : seul `range` est significatif (filtre par région,
    /// méthodologie et fenêtre), le reste satisfait le trait sans IO.
    struct FakeRepo {
        points: Vec<Measurement>,
    }

    #[async_trait]
    impl IntensityRepository for FakeRepo {
        async fn upsert_many(&self, _m: &[Measurement]) -> Result<usize, RepositoryError> {
            Ok(0)
        }

        async fn latest(
            &self,
            _region: Region,
            _methodology_id: &str,
        ) -> Result<Option<Measurement>, RepositoryError> {
            Ok(None)
        }

        async fn range(
            &self,
            region: Region,
            methodology_id: &str,
            range: TimeRange,
        ) -> Result<Vec<Measurement>, RepositoryError> {
            let mut got: Vec<Measurement> = self
                .points
                .iter()
                .filter(|m| {
                    m.region == region && m.methodology.id == methodology_id && range.contains(m.at)
                })
                .cloned()
                .collect();
            got.sort_by_key(|m| m.at);
            Ok(got)
        }

        async fn stats(
            &self,
            _region: Region,
            _methodology_id: &str,
            _range: TimeRange,
        ) -> Result<Option<IntensityStats>, RepositoryError> {
            Ok(None)
        }

        async fn rollup(
            &self,
            _region: Region,
            _methodology_id: &str,
            _range: TimeRange,
            _granularity: Granularity,
        ) -> Result<Vec<RollupBucket>, RepositoryError> {
            Ok(vec![])
        }

        async fn refresh_rollups(&self) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    fn point(at: OffsetDateTime, region: Region, methodology: &str, g: f64) -> Measurement {
        Measurement {
            at,
            region,
            intensity: CarbonIntensity::new(g).unwrap(),
            methodology: Methodology::new(methodology, 1),
            vintage: Vintage::Tr,
            mix: None,
        }
    }

    /// Motif horaire (creux la nuit, pointe l'après-midi) — pour vérifier que la
    /// climatologie traverse bien l'adapter.
    fn hourly_pattern(t: OffsetDateTime) -> f64 {
        match t.hour() {
            0..=5 => 20.0,
            12..=17 => 80.0,
            _ => 50.0,
        }
    }

    fn seed_pattern(end: OffsetDateTime, step: Duration, count: usize) -> Vec<Measurement> {
        (0..count)
            .map(|i| {
                let at = end - step * ((count - i) as i32);
                point(at, Region::National, "rte-direct", hourly_pattern(at))
            })
            .collect()
    }

    #[tokio::test]
    async fn forecasts_from_repository_history() {
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let step = Duration::hours(1);
        let repo = FakeRepo {
            points: seed_pattern(from, step, 14 * 24),
        };
        let forecaster = ClimatologyForecaster::with_config(
            repo,
            8,
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        );

        let out = forecaster
            .forecast(Region::National, "rte-direct", from, Duration::hours(24))
            .await
            .unwrap();

        assert_eq!(out.len(), 24);
        let night = out
            .iter()
            .find(|m| m.at.hour() == 3)
            .unwrap()
            .expected
            .value();
        let day = out
            .iter()
            .find(|m| m.at.hour() == 14)
            .unwrap()
            .expected
            .value();
        assert!(night < day, "nuit {night} doit être < jour {day}");
    }

    #[tokio::test]
    async fn empty_history_is_not_enough_data() {
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let repo = FakeRepo { points: vec![] };
        let forecaster = ClimatologyForecaster::new(repo);
        let err = forecaster
            .forecast(Region::National, "rte-direct", from, Duration::hours(24))
            .await
            .unwrap_err();
        assert!(matches!(err, ForecastError::NotEnoughData));
    }

    #[tokio::test]
    async fn filters_by_region_and_methodology() {
        // L'historique n'existe que pour (National, rte-direct) : prévoir une
        // autre méthodologie ne trouve rien → NotEnoughData.
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let step = Duration::hours(1);
        let repo = FakeRepo {
            points: seed_pattern(from, step, 14 * 24),
        };
        let forecaster = ClimatologyForecaster::new(repo);
        let err = forecaster
            .forecast(Region::National, "acv-ademe", from, Duration::hours(24))
            .await
            .unwrap_err();
        assert!(matches!(err, ForecastError::NotEnoughData));
    }

    #[tokio::test]
    async fn history_window_excludes_data_older_than_n_weeks() {
        // Fenêtre = 1 semaine. Une observation extrême vieille de 10 jours est
        // hors fenêtre : elle ne doit pas polluer la prévision.
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let step = Duration::hours(1);
        let mut points: Vec<Measurement> = (0..7 * 24)
            .map(|i| point(from - step * (i + 1), Region::National, "rte-direct", 50.0))
            .collect();
        points.push(point(
            from - Duration::days(10),
            Region::National,
            "rte-direct",
            9999.0,
        ));

        let forecaster = ClimatologyForecaster::with_config(
            FakeRepo { points },
            1,
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        );
        let out = forecaster
            .forecast(Region::National, "rte-direct", from, Duration::hours(6))
            .await
            .unwrap();
        assert!(
            out.iter().all(|m| m.expected.value() < 100.0),
            "la valeur hors fenêtre (9999) ne doit pas influencer la prévision"
        );
    }

    // ---- CachedForecaster (audit perf 2026-08) ------------------------------

    use std::sync::atomic::{AtomicUsize, Ordering};

    use carbonfr_core::domain::ModelVersion;

    /// Modèle factice : compte les calculs et renvoie un point déterministe.
    struct CountingModel {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ForecastModel for CountingModel {
        async fn forecast(
            &self,
            region: Region,
            methodology_id: &str,
            from: OffsetDateTime,
            _horizon: Duration,
        ) -> Result<Vec<ForecastPoint>, ForecastError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let g = CarbonIntensity::new(42.0).unwrap();
            Ok(vec![ForecastPoint::new(
                from,
                region,
                g,
                g,
                g,
                Methodology::new(methodology_id, 1),
                ModelVersion::new("climatology", 1),
            )])
        }
    }

    /// Horloge figée : rend le seau « from ≈ maintenant » déterministe.
    struct FixedClock(OffsetDateTime);

    impl Clock for FixedClock {
        fn now(&self) -> OffsetDateTime {
            self.0
        }
    }

    fn cached(
        ttl: std::time::Duration,
        now: OffsetDateTime,
    ) -> (CachedForecaster<CountingModel>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let model = CountingModel {
            calls: calls.clone(),
        };
        (
            CachedForecaster::new(model, ttl).with_clock(Arc::new(FixedClock(now))),
            calls,
        )
    }

    #[tokio::test]
    async fn cache_serves_second_call_without_recompute() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let (forecaster, calls) = cached(std::time::Duration::from_secs(900), now);

        let first = forecaster
            .forecast(Region::National, "rte-direct", now, Duration::hours(24))
            .await
            .unwrap();
        let second = forecaster
            .forecast(Region::National, "rte-direct", now, Duration::hours(24))
            .await
            .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1, "un seul calcul attendu");
        assert_eq!(first, second, "la série servie du cache est identique");
    }

    #[tokio::test]
    async fn cache_expires_after_ttl() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        // TTL nul : l'entrée expire immédiatement → chaque appel recalcule.
        let (forecaster, calls) = cached(std::time::Duration::ZERO, now);

        for _ in 0..2 {
            forecaster
                .forecast(Region::National, "rte-direct", now, Duration::hours(24))
                .await
                .unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2, "TTL écoulé → recalcul");
    }

    #[tokio::test]
    async fn distant_from_bypasses_cache() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let (forecaster, calls) = cached(std::time::Duration::from_secs(900), now);

        // `from` rétrospectif (hors du seau courant) : jamais mis en cache —
        // comportement d'avant préservé, cardinalité de clé bornée.
        let from = now - Duration::days(2);
        for _ in 0..2 {
            forecaster
                .forecast(Region::National, "rte-direct", from, Duration::hours(24))
                .await
                .unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2, "from passé → pas de cache");
    }

    #[tokio::test]
    async fn distinct_keys_do_not_collide() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let (forecaster, calls) = cached(std::time::Duration::from_secs(900), now);

        forecaster
            .forecast(Region::National, "rte-direct", now, Duration::hours(24))
            .await
            .unwrap();
        // Autre horizon, autre région, autre méthodologie : trois clés neuves.
        forecaster
            .forecast(Region::National, "rte-direct", now, Duration::hours(48))
            .await
            .unwrap();
        forecaster
            .forecast(Region::Bretagne, "rte-direct", now, Duration::hours(24))
            .await
            .unwrap();
        forecaster
            .forecast(Region::National, "acv-ademe", now, Duration::hours(24))
            .await
            .unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            4,
            "chaque clé calcule une fois"
        );
    }

    #[tokio::test]
    async fn injected_bands_drive_the_interval() {
        // Historique plat à 40 → sans bandes, l'intervalle est dégénéré (40,40).
        // Avec des bandes calibrées (résidus −10..+20), il s'ouvre autour de 40.
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let step = Duration::minutes(15);
        let history: Vec<Measurement> = (1..=8 * 7 * 96)
            .map(|i: i32| point(from - step * i, Region::National, "rte-direct", 40.0))
            .collect();

        let residuals: Vec<f64> = (-10..=20).map(|x| x as f64).collect();
        let bands = HorizonBands::from_residuals(
            step,
            &[
                residuals.clone(),
                residuals.clone(),
                residuals.clone(),
                residuals,
            ],
            0.1,
        );

        let forecaster = ClimatologyForecaster::with_config(
            FakeRepo { points: history },
            8,
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        )
        .with_bands(bands);

        let out = forecaster
            .forecast(Region::National, "rte-direct", from, Duration::hours(1))
            .await
            .unwrap();

        let p = &out[0];
        assert!((p.expected.value() - 40.0).abs() < 1.0);
        // L'intervalle vient des bandes (résidus signés), pas dégénéré.
        assert!(
            p.lower.value() < p.expected.value(),
            "lower = {}",
            p.lower.value()
        );
        assert!(
            p.upper.value() > p.expected.value(),
            "upper = {}",
            p.upper.value()
        );
    }
}
