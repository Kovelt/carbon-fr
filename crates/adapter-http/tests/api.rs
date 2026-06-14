//! Tests d'intégration de l'API : routeur monté sur un repository *fake* en
//! mémoire, requêtes envoyées via `tower::ServiceExt::oneshot` (sans réseau).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use carbonfr_adapter_http::{AppState, router};
use carbonfr_core::domain::{
    CarbonIntensity, GenerationMix, Granularity, IntensityStats, Measurement, Methodology, Region,
    RollupBucket, TimeRange, Vintage, VisitStats,
};
use carbonfr_core::ports::{IntensityRepository, RepositoryError, VisitCounter};
use time::{Date, Duration, OffsetDateTime};
use tower::ServiceExt;

/// Repository minimal : une mesure « courante », une série pour les plages, et
/// un compteur de visites en mémoire.
#[derive(Clone, Default)]
struct FakeRepo {
    measurement: Option<Measurement>,
    series: Vec<Measurement>,
    visits: Arc<Mutex<HashSet<(String, Date)>>>,
}

#[async_trait]
impl IntensityRepository for FakeRepo {
    async fn upsert_many(&self, _: &[Measurement]) -> Result<usize, RepositoryError> {
        Ok(0)
    }

    async fn latest(
        &self,
        region: Region,
        methodology_id: &str,
    ) -> Result<Option<Measurement>, RepositoryError> {
        Ok(self
            .measurement
            .clone()
            .filter(|m| m.region == region && m.methodology.id == methodology_id))
    }

    async fn range(
        &self,
        region: Region,
        _methodology_id: &str,
        range: TimeRange,
    ) -> Result<Vec<Measurement>, RepositoryError> {
        let mut out: Vec<Measurement> = self
            .series
            .iter()
            .filter(|m| m.region == region && range.contains(m.at))
            .cloned()
            .collect();
        out.sort_by_key(|m| m.at);
        Ok(out)
    }

    async fn stats(
        &self,
        region: Region,
        _methodology_id: &str,
        range: TimeRange,
    ) -> Result<Option<IntensityStats>, RepositoryError> {
        let values: Vec<f64> = self
            .series
            .iter()
            .filter(|m| m.region == region && range.contains(m.at))
            .map(|m| m.intensity.value())
            .collect();
        Ok(stats_of(&values))
    }

    async fn rollup(
        &self,
        region: Region,
        _methodology_id: &str,
        range: TimeRange,
        granularity: Granularity,
    ) -> Result<Vec<RollupBucket>, RepositoryError> {
        use std::collections::BTreeMap;
        let mut buckets: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
        for m in self
            .series
            .iter()
            .filter(|m| m.region == region && range.contains(m.at))
        {
            let step = match granularity {
                Granularity::Hourly => 3600,
                Granularity::Daily => 86_400,
            };
            let ts = m.at.unix_timestamp();
            buckets
                .entry(ts - ts.rem_euclid(step))
                .or_default()
                .push(m.intensity.value());
        }
        Ok(buckets
            .into_iter()
            .filter_map(|(ts, values)| {
                let start = OffsetDateTime::from_unix_timestamp(ts).ok()?;
                stats_of(&values).map(|stats| RollupBucket { start, stats })
            })
            .collect())
    }

    async fn refresh_rollups(&self) -> Result<(), RepositoryError> {
        Ok(())
    }
}

#[async_trait]
impl VisitCounter for FakeRepo {
    async fn record_visit(&self, visitor: &str, day: Date) -> Result<VisitStats, RepositoryError> {
        self.visits
            .lock()
            .unwrap()
            .insert((visitor.to_string(), day));
        self.visit_stats().await
    }

    async fn visit_stats(&self) -> Result<VisitStats, RepositoryError> {
        let visits = self.visits.lock().unwrap();
        let unique: HashSet<&String> = visits.iter().map(|(v, _)| v).collect();
        let since = visits.iter().map(|(_, d)| *d).min();
        Ok(VisitStats {
            unique: unique.len() as u64,
            total: visits.len() as u64,
            since,
        })
    }
}

fn stats_of(values: &[f64]) -> Option<IntensityStats> {
    if values.is_empty() {
        return None;
    }
    let average = values.iter().sum::<f64>() / values.len() as f64;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Some(IntensityStats {
        average: CarbonIntensity::new(average)?,
        min: CarbonIntensity::new(min)?,
        max: CarbonIntensity::new(max)?,
        count: values.len() as u64,
    })
}

fn national_measurement() -> Measurement {
    Measurement {
        at: OffsetDateTime::UNIX_EPOCH,
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
    }
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn app(measurement: Option<Measurement>) -> axum::Router {
    router(AppState::new(FakeRepo {
        measurement,
        series: Vec::new(),
        visits: Arc::default(),
    }))
}

fn app_with_series(series: Vec<Measurement>) -> axum::Router {
    router(AppState::new(FakeRepo {
        measurement: None,
        series,
        visits: Arc::default(),
    }))
}

async fn get(app: axum::Router, uri: &str) -> axum::response::Response {
    app.oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn intensity_now_returns_latest() {
    let response = get(app(Some(national_measurement())), "/v1/intensity/now").await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["region"], "national");
    assert_eq!(body["intensity"]["value"], 15.0);
    assert_eq!(body["intensity"]["unit"], "gCO2eq/kWh");
    assert_eq!(body["methodology"], "rte-direct");
    assert_eq!(body["vintage"], "tr");
    assert_eq!(body["timestamp"], "1970-01-01T00:00:00Z");
}

#[tokio::test]
async fn methodology_param_selects_series() {
    // La mesure stockée est en `acv-ademe`.
    let mut m = national_measurement();
    m.methodology = Methodology::acv_ademe();

    // Sans paramètre → défaut rte-direct → 404 (rien en rte-direct).
    let response = get(app(Some(m.clone())), "/v1/intensity/now").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Avec ?methodology=acv-ademe → 200.
    let response = get(app(Some(m)), "/v1/intensity/now?methodology=acv-ademe").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["methodology"], "acv-ademe");
}

#[tokio::test]
async fn mix_returns_generation_breakdown() {
    let response = get(app(Some(national_measurement())), "/v1/mix").await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["unit"], "MW");
    assert_eq!(body["mix"]["nucleaire"], 38815.0);
    assert_eq!(body["mix"]["echanges"], -11574.0);
}

#[tokio::test]
async fn missing_data_is_404() {
    let response = get(app(None), "/v1/intensity/now").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = json_body(response).await;
    assert_eq!(body["error"], "no_data");
}

#[tokio::test]
async fn unknown_region_is_400() {
    let response = get(
        app(Some(national_measurement())),
        "/v1/intensity/now?region=atlantide",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn health_is_ok() {
    let response = get(app(None), "/health").await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&bytes[..], b"ok");
}

fn point(at: OffsetDateTime, g: f64) -> Measurement {
    Measurement {
        at,
        region: Region::National,
        intensity: CarbonIntensity::new(g).unwrap(),
        methodology: Methodology::rte_direct(),
        vintage: Vintage::Definitive,
        mix: None,
    }
}

#[tokio::test]
async fn intensity_date_returns_series() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let step = Duration::hours(1);
    let series = (0..3)
        .map(|i| point(t0 + step * i, 20.0 + i as f64))
        .collect();

    // Fenêtre [t0, t0+2h) → 2 premiers points (t0+2h exclu).
    let response = get(
        app_with_series(series),
        "/v1/intensity/date?from=1970-01-01T00:00:00Z&to=1970-01-01T02:00:00Z",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["region"], "national");
    assert_eq!(body["unit"], "gCO2eq/kWh");
    assert_eq!(body["count"], 2);
    assert_eq!(body["data"][0]["timestamp"], "1970-01-01T00:00:00Z");
    assert_eq!(body["data"][0]["intensity"], 20.0);
    assert_eq!(body["data"][0]["vintage"], "definitive");
}

#[tokio::test]
async fn intensity_date_missing_param_is_400() {
    let response = get(app(None), "/v1/intensity/date?from=1970-01-01T00:00:00Z").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["error"], "bad_request");
}

#[tokio::test]
async fn intensity_date_inverted_window_is_400() {
    let response = get(
        app(None),
        "/v1/intensity/date?from=1970-01-02T00:00:00Z&to=1970-01-01T00:00:00Z",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn intensity_date_window_too_wide_is_400() {
    let response = get(
        app(None),
        "/v1/intensity/date?from=2020-01-01T00:00:00Z&to=2024-01-01T00:00:00Z",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn stats_summary_returns_aggregates() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let series = vec![point(t0, 10.0), point(t0 + Duration::minutes(30), 30.0)];
    let response = get(
        app_with_series(series),
        "/v1/intensity/stats?from=1970-01-01T00:00:00Z&to=1970-01-01T01:00:00Z",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["average"], 20.0);
    assert_eq!(body["min"], 10.0);
    assert_eq!(body["max"], 30.0);
    assert_eq!(body["count"], 2);
    // Sans `interval`, le champ `intervals` est omis.
    assert!(body.get("intervals").is_none());
}

#[tokio::test]
async fn stats_with_interval_includes_buckets() {
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let series = vec![
        point(t0, 10.0),
        point(t0 + Duration::minutes(30), 30.0),
        point(t0 + Duration::hours(1), 50.0),
    ];
    let response = get(
        app_with_series(series),
        "/v1/intensity/stats?from=1970-01-01T00:00:00Z&to=1970-01-01T02:00:00Z&interval=hour",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["interval"], "hour");
    assert_eq!(body["intervals"].as_array().unwrap().len(), 2);
    assert_eq!(body["intervals"][0]["average"], 20.0);
    assert_eq!(body["intervals"][1]["average"], 50.0);
}

#[tokio::test]
async fn stats_without_data_is_404() {
    let response = get(
        app_with_series(vec![]),
        "/v1/intensity/stats?from=1970-01-01T00:00:00Z&to=1970-01-01T01:00:00Z",
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn openapi_spec_is_served() {
    let response = get(app(None), "/v1/openapi.json").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["openapi"], "3.1.0");
    assert!(body["paths"]["/v1/intensity/now"].is_object());
}

#[tokio::test]
async fn swagger_ui_is_served() {
    let response = get(app(None), "/docs").await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("swagger-ui"));
}

#[tokio::test]
async fn stats_bad_interval_is_400() {
    let series = vec![point(OffsetDateTime::UNIX_EPOCH, 10.0)];
    let response = get(
        app_with_series(series),
        "/v1/intensity/stats?from=1970-01-01T00:00:00Z&to=1970-01-01T01:00:00Z&interval=week",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn visit_counter_records_and_reports() {
    let app = app(None);

    // Enregistre une visite (IP via X-Forwarded-For).
    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/stats/visit")
                .header("x-forwarded-for", "203.0.113.7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["unique"], 1);
    assert_eq!(body["total"], 1);

    // Une 2ᵉ visite de la même IP le même jour ne compte pas deux fois.
    app.clone()
        .oneshot(
            Request::post("/v1/stats/visit")
                .header("x-forwarded-for", "203.0.113.7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = json_body(get(app, "/v1/stats").await).await;
    assert_eq!(body["unique"], 1);
    assert_eq!(body["total"], 1);
}
