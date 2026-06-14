//! Tests des cas d'usage avec des adapters *fakes* en mémoire.
//!
//! Démonstration concrète du bénéfice hexagonal : toute la logique se teste
//! sans base de données ni réseau.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use time::{Duration, OffsetDateTime};

use carbonfr_core::application::{
    BackfillHistory, FindGreenestWindow, GetCurrentIntensity, IngestLatest,
};
use carbonfr_core::domain::{
    CarbonIntensity, Measurement, MeasurementKey, Methodology, Region, TimeRange, Vintage,
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
            t0,
            Duration::hours(24),
            Duration::minutes(30),
        )
        .await
        .unwrap();

    assert_eq!(window.start, t0 + step * 2);
    assert!(window.average.value() < 18.0);
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
