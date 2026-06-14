//! Tests d'intégration de l'API : routeur monté sur un repository *fake* en
//! mémoire, requêtes envoyées via `tower::ServiceExt::oneshot` (sans réseau).

use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use carbonfr_adapter_http::{AppState, router};
use carbonfr_core::domain::{
    CarbonIntensity, GenerationMix, Measurement, Methodology, Region, TimeRange, Vintage,
};
use carbonfr_core::ports::{IntensityRepository, RepositoryError};
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;

/// Repository minimal : une mesure « courante » et une série pour les plages.
#[derive(Clone, Default)]
struct FakeRepo {
    measurement: Option<Measurement>,
    series: Vec<Measurement>,
}

#[async_trait]
impl IntensityRepository for FakeRepo {
    async fn upsert_many(&self, _: &[Measurement]) -> Result<usize, RepositoryError> {
        Ok(0)
    }

    async fn latest(
        &self,
        region: Region,
        _methodology_id: &str,
    ) -> Result<Option<Measurement>, RepositoryError> {
        Ok(self.measurement.clone().filter(|m| m.region == region))
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
    }))
}

fn app_with_series(series: Vec<Measurement>) -> axum::Router {
    router(AppState::new(FakeRepo {
        measurement: None,
        series,
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
