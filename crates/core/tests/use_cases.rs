//! Tests des cas d'usage avec des adapters *fakes* en mémoire.
//!
//! Démonstration concrète du bénéfice hexagonal : toute la logique se teste
//! sans base de données ni réseau.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use time::{Duration, OffsetDateTime};

use carbonfr_core::application::{
    ApplicationError, BackfillHistory, BacktestConsumptionForecast, FindGreenestWindow,
    GetCurrentIntensity, GetIntensityHistory, GetIntensityStats, IngestLatest, IngestRecent,
};
use carbonfr_core::domain::{
    CarbonIntensity, ForecastPoint, GenerationMix, Granularity, IntensityStats, Measurement,
    MeasurementKey, Methodology, ModelVersion, Region, RollupBucket, TimeRange, Vintage,
    WindowEstimator,
};
use carbonfr_core::ports::{
    Eco2mixArchive, Eco2mixSource, ForecastError, ForecastModel, IntensityRepository,
    RepositoryError, SourceError,
};

fn measurement(at: OffsetDateTime, region: Region, g: f64, vintage: Vintage) -> Measurement {
    Measurement {
        at,
        region,
        intensity: CarbonIntensity::new(g).unwrap(),
        methodology: Methodology::rte_direct(),
        vintage,
        mix: None,
    }
}

/// Point de prévision *fake* à bande symétrique de demi-largeur `half`.
fn forecast_point(at: OffsetDateTime, region: Region, g: f64, half: f64) -> ForecastPoint {
    ForecastPoint::new(
        at,
        region,
        CarbonIntensity::new(g).unwrap(),
        CarbonIntensity::new((g - half).max(0.0)).unwrap(),
        CarbonIntensity::new(g + half).unwrap(),
        Methodology::rte_direct(),
        ModelVersion::new("climatology", 1),
    )
}

/// Repository en mémoire, avec upsert conditionnel au millésime (ADR-0006).
/// `Clone` partage le même stockage (Arc interne), pour brancher plusieurs
/// cas d'usage sur la même base dans un test.
#[derive(Clone, Default)]
struct InMemoryRepo {
    store: Arc<Mutex<HashMap<MeasurementKey, Measurement>>>,
}

#[async_trait]
impl IntensityRepository for InMemoryRepo {
    async fn upsert_many(&self, measurements: &[Measurement]) -> Result<usize, RepositoryError> {
        let mut store = self.store.lock().unwrap();
        let mut written = 0;
        for m in measurements {
            let key = m.key();
            match store.get(&key) {
                // On conserve la donnée existante si elle est de meilleure qualité.
                Some(existing) if existing.vintage > m.vintage => {}
                _ => {
                    store.insert(key, m.clone());
                    written += 1;
                }
            }
        }
        Ok(written)
    }

    async fn latest(
        &self,
        region: Region,
        methodology_id: &str,
    ) -> Result<Option<Measurement>, RepositoryError> {
        let store = self.store.lock().unwrap();
        Ok(store
            .values()
            .filter(|m| m.region == region && m.methodology.id == methodology_id)
            .max_by_key(|m| m.at)
            .cloned())
    }

    async fn range(
        &self,
        region: Region,
        methodology_id: &str,
        range: TimeRange,
    ) -> Result<Vec<Measurement>, RepositoryError> {
        let store = self.store.lock().unwrap();
        let mut out: Vec<Measurement> = store
            .values()
            .filter(|m| {
                m.region == region && m.methodology.id == methodology_id && range.contains(m.at)
            })
            .cloned()
            .collect();
        out.sort_by_key(|m| m.at);
        Ok(out)
    }

    async fn stats(
        &self,
        region: Region,
        methodology_id: &str,
        range: TimeRange,
    ) -> Result<Option<IntensityStats>, RepositoryError> {
        let store = self.store.lock().unwrap();
        let values: Vec<f64> = store
            .values()
            .filter(|m| {
                m.region == region && m.methodology.id == methodology_id && range.contains(m.at)
            })
            .map(|m| m.intensity.value())
            .collect();
        Ok(stats_from(&values))
    }

    async fn rollup(
        &self,
        region: Region,
        methodology_id: &str,
        range: TimeRange,
        granularity: Granularity,
    ) -> Result<Vec<RollupBucket>, RepositoryError> {
        use std::collections::BTreeMap;
        let store = self.store.lock().unwrap();
        let mut buckets: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
        for m in store.values().filter(|m| {
            m.region == region && m.methodology.id == methodology_id && range.contains(m.at)
        }) {
            let key = bucket_start(m.at, granularity).unix_timestamp();
            buckets.entry(key).or_default().push(m.intensity.value());
        }
        Ok(buckets
            .into_iter()
            .filter_map(|(ts, values)| {
                let start = OffsetDateTime::from_unix_timestamp(ts).ok()?;
                stats_from(&values).map(|stats| RollupBucket { start, stats })
            })
            .collect())
    }

    async fn refresh_rollups(&self) -> Result<(), RepositoryError> {
        Ok(())
    }
}

/// Statistiques d'une série de valeurs, ou `None` si vide.
fn stats_from(values: &[f64]) -> Option<IntensityStats> {
    if values.is_empty() {
        return None;
    }
    let count = values.len() as u64;
    let average = values.iter().sum::<f64>() / values.len() as f64;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Some(IntensityStats {
        average: CarbonIntensity::new(average)?,
        min: CarbonIntensity::new(min)?,
        max: CarbonIntensity::new(max)?,
        count,
    })
}

/// Début du seau (UTC) couvrant `at` pour un pas donné.
fn bucket_start(at: OffsetDateTime, granularity: Granularity) -> OffsetDateTime {
    let step = match granularity {
        Granularity::Hourly => 3600,
        Granularity::Daily => 86_400,
    };
    let ts = at.unix_timestamp();
    OffsetDateTime::from_unix_timestamp(ts - ts.rem_euclid(step)).unwrap_or(at)
}

struct FakeSource {
    measurement: Measurement,
}

#[async_trait]
impl Eco2mixSource for FakeSource {
    async fn latest(&self, _region: Region) -> Result<Measurement, SourceError> {
        Ok(self.measurement.clone())
    }
    async fn range(
        &self,
        _region: Region,
        _range: TimeRange,
    ) -> Result<Vec<Measurement>, SourceError> {
        Ok(vec![self.measurement.clone()])
    }
}

struct FakeForecast {
    points: Vec<ForecastPoint>,
}

#[async_trait]
impl ForecastModel for FakeForecast {
    async fn forecast(
        &self,
        _region: Region,
        _methodology_id: &str,
        _from: OffsetDateTime,
        _horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError> {
        if self.points.is_empty() {
            return Err(ForecastError::NotEnoughData);
        }
        Ok(self.points.clone())
    }
}

#[tokio::test]
async fn ingest_then_read_current() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let repo = InMemoryRepo::default();
    let source = FakeSource {
        measurement: measurement(t0, Region::National, 42.0, Vintage::Tr),
    };

    let ingest = IngestLatest::new(source, repo.clone());
    assert_eq!(ingest.execute(Region::National).await.unwrap(), 1);

    let get = GetCurrentIntensity::new(repo.clone(), "rte-direct");
    let current = get.execute(Region::National).await.unwrap();
    assert_eq!(current.intensity.value(), 42.0);
}

#[tokio::test]
async fn read_current_without_data_errors() {
    let repo = InMemoryRepo::default();
    let get = GetCurrentIntensity::new(repo, "rte-direct");
    assert!(get.execute(Region::Bretagne).await.is_err());
}

#[tokio::test]
async fn ingest_derives_and_stores_acv_ademe() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let repo = InMemoryRepo::default();
    // Mesure source (rte-direct) portant un mix de production.
    let source_measurement = Measurement {
        at: t0,
        region: Region::National,
        intensity: CarbonIntensity::new(15.0).unwrap(),
        methodology: Methodology::rte_direct(),
        vintage: Vintage::Tr,
        mix: Some(GenerationMix {
            nucleaire: 38815.0,
            gaz: 666.0,
            charbon: 0.0,
            fioul: 34.0,
            hydraulique: 8893.0,
            eolien: 2555.0,
            solaire: 1050.0,
            bioenergies: 1006.0,
            pompage: -76.0,
            echanges: -11574.0,
            thermique: None,
        }),
    };
    let source = FakeSource {
        measurement: source_measurement,
    };

    // Une ingestion → deux écritures (rte-direct + acv-ademe dérivée).
    let ingest = IngestLatest::new(source, repo.clone());
    assert_eq!(ingest.execute(Region::National).await.unwrap(), 2);

    let rte = GetCurrentIntensity::new(repo.clone(), "rte-direct")
        .execute(Region::National)
        .await
        .unwrap();
    let acv = GetCurrentIntensity::new(repo, "acv-ademe")
        .execute(Region::National)
        .await
        .unwrap();

    assert_eq!(rte.intensity.value(), 15.0);
    assert_eq!(acv.methodology, Methodology::acv_ademe());
    // Intensité ACV du mix < taux_co2 publié pour ce mix très bas-carbone.
    assert!(acv.intensity.value() < rte.intensity.value());
}

#[tokio::test]
async fn upsert_respects_vintage_quality() {
    let repo = InMemoryRepo::default();
    let t = OffsetDateTime::UNIX_EPOCH;

    // Temps réel d'abord.
    assert_eq!(
        repo.upsert_many(&[measurement(t, Region::National, 50.0, Vintage::Tr)])
            .await
            .unwrap(),
        1
    );

    // Le consolidé remplace le temps réel.
    assert_eq!(
        repo.upsert_many(&[measurement(
            t,
            Region::National,
            40.0,
            Vintage::Consolidated
        )])
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        repo.latest(Region::National, "rte-direct")
            .await
            .unwrap()
            .unwrap()
            .intensity
            .value(),
        40.0
    );

    // Un temps réel tardif ne doit PAS écraser le consolidé.
    assert_eq!(
        repo.upsert_many(&[measurement(t, Region::National, 99.0, Vintage::Tr)])
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        repo.latest(Region::National, "rte-direct")
            .await
            .unwrap()
            .unwrap()
            .intensity
            .value(),
        40.0
    );
}

/// Source *fake* pour [`IngestRecent`] : `range()` rend un résultat préconfiguré
/// (mesures ou erreur), et trace si elle a été appelée — pour prouver qu'une
/// fenêtre vide/négative court-circuite l'appel réseau (ADR-0003 addendum).
struct RangeFakeSource {
    measurements: Vec<Measurement>,
    fail: bool,
    called: Arc<std::sync::atomic::AtomicBool>,
    /// Dernière plage demandée à la source (preuve du calcul de fenêtre).
    received: Arc<std::sync::Mutex<Option<TimeRange>>>,
}

impl RangeFakeSource {
    fn ok(measurements: Vec<Measurement>) -> Self {
        Self {
            measurements,
            fail: false,
            called: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            received: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    fn erroring() -> Self {
        Self {
            measurements: Vec::new(),
            fail: true,
            called: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            received: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Sonde partagée : à lire *après* que la source a (ou non) été consommée
    /// par le cas d'usage sous test.
    fn call_flag(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.called.clone()
    }
}

#[async_trait]
impl Eco2mixSource for RangeFakeSource {
    async fn latest(&self, _region: Region) -> Result<Measurement, SourceError> {
        unimplemented!("IngestRecent n'appelle jamais latest()")
    }

    async fn range(
        &self,
        _region: Region,
        range: TimeRange,
    ) -> Result<Vec<Measurement>, SourceError> {
        self.called.store(true, std::sync::atomic::Ordering::SeqCst);
        *self.received.lock().expect("verrou de test") = Some(range);
        if self.fail {
            Err(SourceError::Unavailable("source en panne (test)".into()))
        } else {
            Ok(self.measurements.clone())
        }
    }
}

#[tokio::test]
async fn ingest_recent_upserts_all_measurements_and_returns_the_latest() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let repo = InMemoryRepo::default();
    let source = RangeFakeSource::ok(vec![
        measurement(t0, Region::National, 40.0, Vintage::Tr),
        measurement(
            t0 + Duration::minutes(15),
            Region::National,
            42.0,
            Vintage::Tr,
        ),
        measurement(
            t0 + Duration::minutes(30),
            Region::National,
            38.0,
            Vintage::Tr,
        ),
    ]);

    let ingest = IngestRecent::new(source, repo.clone(), Duration::hours(3));
    let report = ingest
        .execute(Region::National, t0 + Duration::hours(1))
        .await
        .unwrap();

    assert_eq!(report.written, 3);
    let latest = report.latest.expect("mesure la plus récente présente");
    assert_eq!(latest.at, t0 + Duration::minutes(30));
    assert_eq!(latest.intensity.value(), 38.0);

    // Les trois points sont bien en base (pas seulement le dernier).
    let range = TimeRange::new(t0, t0 + Duration::hours(1)).unwrap();
    let stored = repo
        .range(Region::National, "rte-direct", range)
        .await
        .unwrap();
    assert_eq!(stored.len(), 3);
}

#[tokio::test]
async fn ingest_recent_fills_a_gap_left_by_a_previous_latest_only_cycle() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let repo = InMemoryRepo::default();

    // Cycle précédent : seul t0 avait été ingéré (comportement d'IngestLatest).
    repo.upsert_many(&[measurement(t0, Region::National, 40.0, Vintage::Tr)])
        .await
        .unwrap();
    let t1 = t0 + Duration::minutes(15);
    let before = repo
        .range(
            Region::National,
            "rte-direct",
            TimeRange::new(t0, t1 + Duration::minutes(1)).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(before.len(), 1, "t1 pas encore ingéré (trou)");

    // Ce cycle : ODRÉ a publié en retard — la fenêtre glissante voit t0 ET t1.
    let source = RangeFakeSource::ok(vec![
        measurement(t0, Region::National, 40.0, Vintage::Tr),
        measurement(t1, Region::National, 41.0, Vintage::Tr),
    ]);
    let ingest = IngestRecent::new(source, repo.clone(), Duration::hours(3));
    let report = ingest
        .execute(Region::National, t1 + Duration::minutes(1))
        .await
        .unwrap();
    assert!(report.written > 0);
    assert_eq!(report.latest.unwrap().at, t1);

    let after = repo
        .range(
            Region::National,
            "rte-direct",
            TimeRange::new(t0, t1 + Duration::minutes(1)).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after.len(), 2, "le trou (t1) est comblé");
}

#[tokio::test]
async fn ingest_recent_empty_window_is_not_an_error() {
    let repo = InMemoryRepo::default();
    // La source ne renvoie rien : retard de publication ODRÉ supérieur à la
    // largeur de la fenêtre.
    let source = RangeFakeSource::ok(Vec::new());
    let ingest = IngestRecent::new(source, repo, Duration::hours(3));

    let report = ingest
        .execute(Region::National, OffsetDateTime::UNIX_EPOCH)
        .await
        .expect("une fenêtre vide n'est pas une erreur");
    assert_eq!(report.written, 0);
    assert!(report.latest.is_none());
}

#[tokio::test]
async fn ingest_recent_requests_exactly_the_window_ending_now() {
    // Le cœur du correctif : la plage demandée à la source est `[now − fenêtre, now)`
    // (un signe inversé passerait sinon inaperçu, la fake ignorant la plage).
    let repo = InMemoryRepo::default();
    let source = RangeFakeSource::ok(Vec::new());
    let received = source.received.clone();
    let now = OffsetDateTime::UNIX_EPOCH + Duration::days(10);
    let ingest = IngestRecent::new(source, repo, Duration::hours(3));

    ingest
        .execute(Region::National, now)
        .await
        .expect("fenêtre vide acceptée");
    let expected = TimeRange::new(now - Duration::hours(3), now).expect("plage valide");
    assert_eq!(*received.lock().expect("verrou de test"), Some(expected));
}

#[tokio::test]
async fn ingest_recent_non_positive_window_short_circuits_without_calling_the_source() {
    let repo = InMemoryRepo::default();
    let source = RangeFakeSource::erroring();
    let called = source.call_flag();
    let ingest = IngestRecent::new(source, repo, Duration::ZERO);

    let report = ingest
        .execute(Region::National, OffsetDateTime::UNIX_EPOCH)
        .await
        .expect("fenêtre nulle : bilan vide, pas d'erreur");
    assert_eq!(report.written, 0);
    assert!(report.latest.is_none());
    assert!(
        !called.load(std::sync::atomic::Ordering::SeqCst),
        "la source n'est jamais interrogée pour une fenêtre non positive"
    );
}

#[tokio::test]
async fn ingest_recent_propagates_source_error() {
    let repo = InMemoryRepo::default();
    let source = RangeFakeSource::erroring();
    let ingest = IngestRecent::new(source, repo, Duration::hours(3));

    let err = ingest
        .execute(Region::National, OffsetDateTime::UNIX_EPOCH)
        .await
        .expect_err("l'erreur de la source doit remonter");
    assert!(matches!(
        err,
        carbonfr_core::application::ApplicationError::Source(_)
    ));
}

#[tokio::test]
async fn ingest_recent_derives_acv_ademe_only_for_measurements_without_one() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let repo = InMemoryRepo::default();
    let mix = GenerationMix {
        nucleaire: 38815.0,
        gaz: 666.0,
        charbon: 0.0,
        fioul: 34.0,
        hydraulique: 8893.0,
        eolien: 2555.0,
        solaire: 1050.0,
        bioenergies: 1006.0,
        pompage: -76.0,
        echanges: -11574.0,
        thermique: None,
    };
    // Une mesure `rte-direct` (à dériver) et une mesure déjà `acv-ademe` (à ne
    // PAS re-dériver), dans la même fenêtre.
    let rte = Measurement {
        at: t0,
        region: Region::National,
        intensity: CarbonIntensity::new(15.0).unwrap(),
        methodology: Methodology::rte_direct(),
        vintage: Vintage::Tr,
        mix: Some(mix),
    };
    let already_acv = Measurement {
        at: t0 + Duration::minutes(15),
        region: Region::National,
        intensity: CarbonIntensity::new(20.0).unwrap(),
        methodology: Methodology::acv_ademe(),
        vintage: Vintage::Tr,
        mix: Some(mix),
    };
    let source = RangeFakeSource::ok(vec![rte, already_acv]);
    let ingest = IngestRecent::new(source, repo.clone(), Duration::hours(3));

    let report = ingest
        .execute(Region::National, t0 + Duration::hours(1))
        .await
        .unwrap();
    // 2 mesures source + 1 seule dérivée (celle qui n'était pas déjà acv-ademe).
    assert_eq!(report.written, 3);

    let acv_series = repo
        .range(
            Region::National,
            "acv-ademe",
            TimeRange::new(t0, t0 + Duration::hours(1)).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        acv_series.len(),
        2,
        "la dérivée de rte-direct + la mesure déjà acv-ademe"
    );
}

#[tokio::test]
async fn find_greenest_window_uses_forecast() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let step = Duration::minutes(15);
    let values = [120.0, 110.0, 15.0, 18.0, 90.0];
    let points: Vec<ForecastPoint> = values
        .iter()
        .enumerate()
        .map(|(i, &g)| forecast_point(t0 + step * (i as i32), Region::National, g, 5.0))
        .collect();

    let uc = FindGreenestWindow::new(FakeForecast { points });
    let window = uc
        .execute(
            Region::National,
            "rte-direct",
            t0,
            Duration::hours(24),
            Duration::minutes(30),
            WindowEstimator::Central,
        )
        .await
        .unwrap();

    assert_eq!(window.start, t0 + step * 2);
    assert!(window.average.value() < 18.0);
}

#[tokio::test]
async fn get_history_returns_window_sorted() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let step = Duration::hours(1);
    let repo = InMemoryRepo::default();

    // Cinq mesures horaires, insérées dans le désordre.
    let points: Vec<Measurement> = [3, 0, 4, 1, 2]
        .into_iter()
        .map(|i| {
            measurement(
                t0 + step * i,
                Region::National,
                20.0 + i as f64,
                Vintage::Tr,
            )
        })
        .collect();
    repo.upsert_many(&points).await.unwrap();

    let history = GetIntensityHistory::new(repo, "rte-direct");
    // Fenêtre couvrant les 3 premières heures → indices 0, 1, 2.
    let window = TimeRange::new(t0, t0 + step * 3).unwrap();
    let series = history.execute(Region::National, window).await.unwrap();

    assert_eq!(series.len(), 3);
    assert!(
        series.windows(2).all(|w| w[0].at < w[1].at),
        "tri croissant"
    );
    assert_eq!(series[0].at, t0);
    assert_eq!(series[2].at, t0 + step * 2);
}

#[tokio::test]
async fn get_stats_summary_and_hourly_rollup() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let repo = InMemoryRepo::default();
    // 0 h : deux mesures (10, 20) ; 1 h : une mesure (60).
    repo.upsert_many(&[
        measurement(t0, Region::National, 10.0, Vintage::Tr),
        measurement(
            t0 + Duration::minutes(30),
            Region::National,
            20.0,
            Vintage::Tr,
        ),
        measurement(t0 + Duration::hours(1), Region::National, 60.0, Vintage::Tr),
    ])
    .await
    .unwrap();

    let stats = GetIntensityStats::new(repo, "rte-direct");
    let window = TimeRange::new(t0, t0 + Duration::hours(2)).unwrap();

    // Résumé exact sur les 3 mesures : moy 30, min 10, max 60.
    let summary = stats
        .summary(Region::National, window)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(summary.count, 3);
    assert_eq!(summary.average.value(), 30.0);
    assert_eq!(summary.min.value(), 10.0);
    assert_eq!(summary.max.value(), 60.0);

    // Rollup horaire : 2 seaux ; le premier moyenne (10, 20) = 15.
    let hourly = stats
        .series(Region::National, window, Granularity::Hourly)
        .await
        .unwrap();
    assert_eq!(hourly.len(), 2);
    assert_eq!(hourly[0].start, t0);
    assert_eq!(hourly[0].stats.average.value(), 15.0);
    assert_eq!(hourly[1].stats.average.value(), 60.0);
}

/// Export de masse simulé : rend une mesure à pas `step` couvrant l'intervalle
/// demandé, et enregistre les bornes de chaque tranche reçue. `export_regional`
/// rend, à chaque pas, une mesure `acv-ademe` pour chacune des 12 régions
/// métropolitaines (comme l'export ODRÉ réel, une tranche = un téléchargement
/// couvrant toutes les régions, ADR-0003 addendum 2026-09-26).
#[derive(Clone, Default)]
struct FakeArchive {
    step: Duration,
    ranges: Arc<Mutex<Vec<TimeRange>>>,
    regional_ranges: Arc<Mutex<Vec<TimeRange>>>,
}

#[async_trait]
impl Eco2mixArchive for FakeArchive {
    async fn export_national(&self, range: TimeRange) -> Result<Vec<Measurement>, SourceError> {
        self.ranges.lock().unwrap().push(range);
        let mut out = Vec::new();
        let mut t = range.start();
        while t < range.end() {
            out.push(measurement(t, Region::National, 30.0, Vintage::Definitive));
            t += self.step;
        }
        Ok(out)
    }

    async fn export_national_loads(
        &self,
        _range: TimeRange,
    ) -> Result<Vec<carbonfr_core::domain::LoadRecord>, SourceError> {
        Ok(Vec::new())
    }

    async fn export_regional(&self, range: TimeRange) -> Result<Vec<Measurement>, SourceError> {
        self.regional_ranges.lock().unwrap().push(range);
        let mut out = Vec::new();
        let mut t = range.start();
        while t < range.end() {
            for region in Region::METROPOLITAN {
                out.push(Measurement {
                    at: t,
                    region,
                    intensity: CarbonIntensity::new(40.0).unwrap(),
                    methodology: Methodology::acv_ademe(),
                    vintage: Vintage::Consolidated,
                    mix: None,
                });
            }
            t += self.step;
        }
        Ok(out)
    }
}

/// Archive minimale n'implémentant que le périmètre national (comme un
/// adapter externe écrit avant l'ajout du port régional) : `export_regional`
/// reste sur la méthode par défaut du trait [`Eco2mixArchive`] — additive,
/// donc sans rupture de compilation pour cet implémenteur (SemVer, ADR-0030).
#[derive(Clone, Default)]
struct NationalOnlyArchive;

#[async_trait]
impl Eco2mixArchive for NationalOnlyArchive {
    async fn export_national(&self, _range: TimeRange) -> Result<Vec<Measurement>, SourceError> {
        Ok(Vec::new())
    }

    async fn export_national_loads(
        &self,
        _range: TimeRange,
    ) -> Result<Vec<carbonfr_core::domain::LoadRecord>, SourceError> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn backfill_slices_range_and_upserts_each_window() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let repo = InMemoryRepo::default();
    let archive = FakeArchive {
        step: Duration::hours(1),
        ranges: Arc::default(),
        regional_ranges: Arc::default(),
    };

    // 24 h découpées en tranches de 6 h → 4 tranches, 6 mesures chacune.
    let backfill = BackfillHistory::new(archive.clone(), repo.clone(), Duration::hours(6));
    let range = TimeRange::new(t0, t0 + Duration::hours(24)).unwrap();
    let report = backfill.execute(range).await.unwrap();

    assert_eq!(report.windows, 4);
    assert_eq!(report.read, 24);
    assert_eq!(report.written, 24);

    // Les tranches couvrent l'intervalle sans trou ni chevauchement.
    let (count, first_start, last_end) = {
        let ranges = archive.ranges.lock().unwrap();
        (ranges.len(), ranges[0].start(), ranges[3].end())
    };
    assert_eq!(count, 4);
    assert_eq!(first_start, t0);
    assert_eq!(last_end, t0 + Duration::hours(24));

    // La donnée a bien atterri dans le repository.
    let stored = repo
        .range(Region::National, "rte-direct", range)
        .await
        .unwrap();
    assert_eq!(stored.len(), 24);
}

/// Périmètre régional (PROD-1) : mêmes tranches que le national, mais chaque
/// export rend les 12 régions d'un coup (un export = toutes les régions,
/// ADR-0003 addendum 2026-09-26) — sans dérivation cycle de vie à l'upsert
/// (déjà `acv-ademe` en sortie de l'export).
#[tokio::test]
async fn backfill_regional_slices_range_and_upserts_each_window() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let repo = InMemoryRepo::default();
    let archive = FakeArchive {
        step: Duration::hours(6),
        ranges: Arc::default(),
        regional_ranges: Arc::default(),
    };

    // 24 h découpées en tranches de 6 h → 4 tranches ; à pas 6 h, chaque
    // tranche ne produit qu'un seul pas × 12 régions = 12 mesures.
    let backfill = BackfillHistory::new(archive.clone(), repo.clone(), Duration::hours(6));
    let range = TimeRange::new(t0, t0 + Duration::hours(24)).unwrap();
    let report = backfill.execute_regional(range).await.unwrap();

    assert_eq!(report.windows, 4);
    assert_eq!(report.read, 48);
    assert_eq!(report.written, 48);

    // Les tranches régionales couvrent l'intervalle sans trou ni chevauchement
    // (bloc : le verrou ne doit pas traverser un point d'`await`, cf.
    // `backfill_slices_range_and_upserts_each_window` ci-dessus).
    let (count, first_start, last_end) = {
        let ranges = archive.regional_ranges.lock().unwrap();
        (ranges.len(), ranges[0].start(), ranges[3].end())
    };
    assert_eq!(count, 4);
    assert_eq!(first_start, t0);
    assert_eq!(last_end, t0 + Duration::hours(24));

    // Le national n'a pas été touché par le backfill régional.
    assert!(archive.ranges.lock().unwrap().is_empty());

    // Les 12 régions métropolitaines sont bien présentes dans le repository,
    // sous la méthodologie `acv-ademe`, une mesure par tranche (4).
    for region in Region::METROPOLITAN {
        let stored = repo.range(region, "acv-ademe", range).await.unwrap();
        assert_eq!(stored.len(), 4, "région {region} : 4 mesures attendues");
    }
}

/// La méthode par défaut du port [`Eco2mixArchive::export_regional`] (non
/// redéfinie par un implémenteur qui ne connaît que le national) renvoie
/// `Unavailable` plutôt que de paniquer ou de renvoyer un vide silencieux.
#[tokio::test]
async fn eco2mix_archive_export_regional_defaults_to_unavailable() {
    let archive = NationalOnlyArchive;
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let range = TimeRange::new(t0, t0 + Duration::hours(1)).unwrap();

    let err = archive.export_regional(range).await.unwrap_err();
    assert!(matches!(err, SourceError::Unavailable(_)));
}

/// Ce défaut se propage tel quel jusqu'au cas d'usage : `execute_regional` ne
/// masque pas l'indisponibilité derrière un bilan vide.
#[tokio::test]
async fn backfill_execute_regional_propagates_default_port_error() {
    let repo = InMemoryRepo::default();
    let backfill = BackfillHistory::new(NationalOnlyArchive, repo, Duration::hours(6));
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let range = TimeRange::new(t0, t0 + Duration::hours(6)).unwrap();

    let err = backfill.execute_regional(range).await.unwrap_err();
    assert!(matches!(
        err,
        ApplicationError::Source(SourceError::Unavailable(_))
    ));
}

/// Modèle de prévision *fake* : valeur constante sur la grille (`from + k·step`),
/// pour piloter l'erreur de façon déterministe dans le backtest.
struct GridForecast {
    value: f64,
    step: Duration,
}

#[async_trait]
impl ForecastModel for GridForecast {
    async fn forecast(
        &self,
        region: Region,
        _methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError> {
        let mut points = Vec::new();
        let mut at = from;
        while at < from + horizon {
            points.push(forecast_point(at, region, self.value, 0.0));
            at += self.step;
        }
        Ok(points)
    }
}

#[tokio::test]
async fn backtest_reports_model_and_persistence_error() {
    use carbonfr_core::application::BacktestForecast;

    let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
    let step = Duration::minutes(15);

    // Observé : plat à 50, du lendemain d'avant l'origine jusqu'au-delà des
    // horizons (couvre l'ancre de persistance et l'observé évalué).
    let repo = InMemoryRepo::default();
    let observed: Vec<Measurement> = (0..(4 * 24 * 4)) // 4 jours au pas 15 min
        .map(|i| {
            measurement(
                t0 - Duration::days(1) + step * i,
                Region::National,
                50.0,
                Vintage::Tr,
            )
        })
        .collect();
    repo.upsert_many(&observed).await.unwrap();

    // Modèle constant à 55 → erreur de +5 partout ; persistance = 50 (plat) → 0.
    let backtest = BacktestForecast::new(GridForecast { value: 55.0, step }, repo, "rte-direct");

    let test = TimeRange::new(t0, t0 + Duration::days(2)).unwrap();
    let report = backtest
        .execute(
            Region::National,
            test,
            Duration::days(1), // une origine par jour → 2 origines
            step,
            &[Duration::hours(1)],
        )
        .await
        .unwrap();

    assert_eq!(report.origins, 2);

    let model = report.model.expect("métriques modèle");
    assert!((model.mae - 5.0).abs() < 1e-9, "MAE modèle = {}", model.mae);
    assert!((model.rmse - 5.0).abs() < 1e-9);

    let persistence = report.persistence.expect("métriques persistance");
    assert!(
        persistence.mae.abs() < 1e-9,
        "la persistance d'un signal plat est parfaite, MAE = {}",
        persistence.mae
    );

    // Détail à h+1.
    let h1 = &report.by_horizon[0];
    assert_eq!(h1.horizon, Duration::hours(1));
    assert!((h1.model.unwrap().mae - 5.0).abs() < 1e-9);
    assert!(h1.persistence.unwrap().mae.abs() < 1e-9);
}

#[tokio::test]
async fn backtest_aligns_origins_to_grid() {
    use carbonfr_core::application::BacktestForecast;

    let step = Duration::minutes(15);
    // Observé sur la grille du quart d'heure ; fenêtre de test décalée de 7 min
    // (origine non alignée) → sans alignement, aucune paire ne serait comparée.
    let grid0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
    let repo = InMemoryRepo::default();
    let observed: Vec<Measurement> = (0..(3 * 24 * 4))
        .map(|i| {
            measurement(
                grid0 - Duration::days(1) + step * i,
                Region::National,
                50.0,
                Vintage::Tr,
            )
        })
        .collect();
    repo.upsert_many(&observed).await.unwrap();

    let backtest = BacktestForecast::new(GridForecast { value: 55.0, step }, repo, "rte-direct");
    let test = TimeRange::new(grid0 + Duration::minutes(7), grid0 + Duration::days(1)).unwrap();
    let report = backtest
        .execute(
            Region::National,
            test,
            Duration::days(1),
            step,
            &[Duration::hours(1)],
        )
        .await
        .unwrap();

    // L'origine décalée est ramenée sur la grille → des paires sont comparées.
    assert_eq!(report.origins, 1);
    let model = report
        .model
        .expect("paires comparées malgré l'origine décalée");
    assert!((model.mae - 5.0).abs() < 1e-9);
}

/// Modèle *fake* indisponible avant `available_from` (simule le démarrage à
/// froid : pas assez d'historique pour les premières origines).
struct ColdStartForecast {
    available_from: OffsetDateTime,
    value: f64,
    step: Duration,
}

#[async_trait]
impl ForecastModel for ColdStartForecast {
    async fn forecast(
        &self,
        region: Region,
        _methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError> {
        if from < self.available_from {
            return Err(ForecastError::NotEnoughData);
        }
        let mut points = Vec::new();
        let mut at = from;
        while at < from + horizon {
            points.push(forecast_point(at, region, self.value, 0.0));
            at += self.step;
        }
        Ok(points)
    }
}

#[tokio::test]
async fn backtest_skips_origins_without_history() {
    use carbonfr_core::application::BacktestForecast;

    let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
    let step = Duration::minutes(15);

    let repo = InMemoryRepo::default();
    let observed: Vec<Measurement> = (0..(5 * 24 * 4))
        .map(|i| {
            measurement(
                t0 - Duration::days(1) + step * i,
                Region::National,
                50.0,
                Vintage::Tr,
            )
        })
        .collect();
    repo.upsert_many(&observed).await.unwrap();

    // La prévision n'est disponible qu'à partir de t0 + 1 jour : la première
    // origine (t0) est sautée, pas fatale.
    let backtest = BacktestForecast::new(
        ColdStartForecast {
            available_from: t0 + Duration::days(1),
            value: 55.0,
            step,
        },
        repo,
        "rte-direct",
    );

    let test = TimeRange::new(t0, t0 + Duration::days(3)).unwrap(); // origines t0, +1j, +2j
    let report = backtest
        .execute(
            Region::National,
            test,
            Duration::days(1),
            step,
            &[Duration::hours(1)],
        )
        .await
        .unwrap();

    // t0 sautée → 2 origines évaluées, l'erreur reste bien définie.
    assert_eq!(report.origins, 2);
    assert!((report.model.unwrap().mae - 5.0).abs() < 1e-9);
}

#[tokio::test]
async fn backtest_calibrate_bands_captures_residual() {
    use carbonfr_core::application::BacktestForecast;

    let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
    let step = Duration::minutes(15);

    // Observé plat à 50 ; modèle constant à 55 → erreur (observed − expected)
    // = −5 à chaque horizon.
    let repo = InMemoryRepo::default();
    let observed: Vec<Measurement> = (0..(4 * 24 * 4))
        .map(|i| {
            measurement(
                t0 - Duration::days(1) + step * i,
                Region::National,
                50.0,
                Vintage::Tr,
            )
        })
        .collect();
    repo.upsert_many(&observed).await.unwrap();

    let backtest = BacktestForecast::new(GridForecast { value: 55.0, step }, repo, "rte-direct");
    let test = TimeRange::new(t0, t0 + Duration::days(2)).unwrap();
    let bands = backtest
        .calibrate_bands(
            Region::National,
            test,
            Duration::days(1),
            step,
            Duration::hours(2),
            0.1,
        )
        .await
        .unwrap();

    assert!(!bands.is_empty());
    let (low, high) = bands.at(Duration::ZERO).unwrap();
    assert!((low + 5.0).abs() < 1e-9, "low = {low}");
    assert!((high + 5.0).abs() < 1e-9, "high = {high}");
}

// ── Backtest acv-ademe@2 (vérité dérivée, ADR-0013) ───────────────────────────

use carbonfr_core::domain::{
    CrossBorderFlow, CrossBorderFlows, CrossBorderSnapshot, EmissionFactors, Neighbor,
    TD_LOSS_FACTOR_V1, acv_ademe_consumption_intensity,
};
use carbonfr_core::ports::CrossBorderRepository;

#[derive(Clone, Default)]
struct InMemoryFlows {
    snapshots: Vec<CrossBorderSnapshot>,
}

#[async_trait]
impl CrossBorderRepository for InMemoryFlows {
    async fn upsert_flows(&self, _: &[CrossBorderSnapshot]) -> Result<usize, RepositoryError> {
        Ok(0)
    }
    async fn flows_at(
        &self,
        at: OffsetDateTime,
    ) -> Result<Option<CrossBorderSnapshot>, RepositoryError> {
        Ok(self
            .snapshots
            .iter()
            .filter(|s| s.at <= at)
            .max_by_key(|s| s.at)
            .cloned())
    }
    async fn flows_range(
        &self,
        range: TimeRange,
    ) -> Result<Vec<CrossBorderSnapshot>, RepositoryError> {
        Ok(self
            .snapshots
            .iter()
            .filter(|s| range.contains(s.at))
            .cloned()
            .collect())
    }
}

/// Prévisionniste `acv-ademe@2` *fake* : renvoie une intensité constante sur tout
/// l'horizon (au pas `step`).
struct ConstantAcvForecast {
    value: f64,
    step: Duration,
}

#[async_trait]
impl ForecastModel for ConstantAcvForecast {
    async fn forecast(
        &self,
        region: Region,
        _methodology_id: &str,
        from: OffsetDateTime,
        horizon: Duration,
    ) -> Result<Vec<ForecastPoint>, ForecastError> {
        let mut points = Vec::new();
        let mut t = from;
        while t < from + horizon {
            points.push(ForecastPoint::new(
                t,
                region,
                CarbonIntensity::new(self.value).unwrap(),
                CarbonIntensity::new(self.value).unwrap(),
                CarbonIntensity::new(self.value).unwrap(),
                Methodology::acv_ademe_consumption(),
                ModelVersion::new("acv-clim", 1),
            ));
            t += self.step;
        }
        Ok(points)
    }
}

fn const_mix() -> GenerationMix {
    GenerationMix {
        nucleaire: 40000.0,
        gaz: 1000.0,
        charbon: 0.0,
        fioul: 0.0,
        hydraulique: 5000.0,
        eolien: 3000.0,
        solaire: 500.0,
        bioenergies: 800.0,
        pompage: 0.0,
        echanges: 0.0,
        thermique: None,
    }
}

#[tokio::test]
async fn backtest_consumption_derives_truth_and_scores_zero_for_perfect_model() {
    let step = Duration::hours(1);
    let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);

    // Entrées constantes → vérité @2 constante = V.
    let flows = CrossBorderFlows::new(vec![CrossBorderFlow {
        neighbor: Neighbor::Germany,
        flow_mw: 3000.0,
        neighbor_intensity: CarbonIntensity::new(400.0).unwrap(),
    }]);
    let v = acv_ademe_consumption_intensity(
        &const_mix(),
        &flows,
        &EmissionFactors::acv_ademe_v1(),
        TD_LOSS_FACTOR_V1,
    )
    .unwrap()
    .value();

    // Historique : mix acv-ademe@1 + contexte d'import, sur ~2 jours.
    let repo = InMemoryRepo::default();
    let mut snapshots = Vec::new();
    let mut measures = Vec::new();
    for i in 0..48 {
        let at = t0 + step * i;
        measures.push(Measurement {
            at,
            region: Region::National,
            intensity: CarbonIntensity::new(12.0).unwrap(),
            methodology: Methodology::acv_ademe(),
            vintage: Vintage::Consolidated,
            mix: Some(const_mix()),
        });
        snapshots.push(CrossBorderSnapshot {
            at,
            flows: flows.clone(),
        });
    }
    repo.upsert_many(&measures).await.unwrap();
    let cross = InMemoryFlows { snapshots };

    // Fenêtre de test après un peu d'historique.
    let test = TimeRange::new(t0 + step * 24, t0 + step * 40).unwrap();
    let backtest =
        BacktestConsumptionForecast::new(ConstantAcvForecast { value: v, step }, repo, cross);
    let report = backtest
        .execute(Region::National, test, step, step, &[Duration::hours(1)])
        .await
        .unwrap();

    assert!(report.origins > 0, "aucune origine évaluée");
    let model = report.model.expect("métriques modèle");
    // Modèle parfait (= vérité dérivée constante) → erreur ~0.
    assert!(model.rmse < 1e-6, "rmse = {}", model.rmse);
}
