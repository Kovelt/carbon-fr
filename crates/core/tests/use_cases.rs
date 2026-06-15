//! Tests des cas d'usage avec des adapters *fakes* en mémoire.
//!
//! Démonstration concrète du bénéfice hexagonal : toute la logique se teste
//! sans base de données ni réseau.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use time::{Duration, OffsetDateTime};

use carbonfr_core::application::{
    BackfillHistory, FindGreenestWindow, GetCurrentIntensity, GetIntensityHistory,
    GetIntensityStats, IngestLatest,
};
use carbonfr_core::domain::{
    CarbonIntensity, GenerationMix, Granularity, IntensityStats, Measurement, MeasurementKey,
    Methodology, Region, RollupBucket, TimeRange, Vintage,
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
    points: Vec<Measurement>,
}

#[async_trait]
impl ForecastModel for FakeForecast {
    async fn forecast(
        &self,
        _region: Region,
        _methodology_id: &str,
        _from: OffsetDateTime,
        _horizon: Duration,
    ) -> Result<Vec<Measurement>, ForecastError> {
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

#[tokio::test]
async fn find_greenest_window_uses_forecast() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let step = Duration::minutes(15);
    let values = [120.0, 110.0, 15.0, 18.0, 90.0];
    let points: Vec<Measurement> = values
        .iter()
        .enumerate()
        .map(|(i, &g)| measurement(t0 + step * (i as i32), Region::National, g, Vintage::Tr))
        .collect();

    let uc = FindGreenestWindow::new(FakeForecast { points });
    let window = uc
        .execute(
            Region::National,
            "rte-direct",
            t0,
            Duration::hours(24),
            Duration::minutes(30),
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
/// demandé, et enregistre les bornes de chaque tranche reçue.
#[derive(Clone, Default)]
struct FakeArchive {
    step: Duration,
    ranges: Arc<Mutex<Vec<TimeRange>>>,
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
}

#[tokio::test]
async fn backfill_slices_range_and_upserts_each_window() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let repo = InMemoryRepo::default();
    let archive = FakeArchive {
        step: Duration::hours(1),
        ranges: Arc::default(),
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
