//! Tests d'intégration de l'API : routeur monté sur un repository *fake* en
//! mémoire, requêtes envoyées via `tower::ServiceExt::oneshot` (sans réseau).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use carbonfr_adapter_forecast::ClimatologyForecaster;
use carbonfr_adapter_http::{AppState, ForecastState, StreamState, router};
use carbonfr_core::domain::{
    CarbonIntensity, CrossBorderSnapshot, GenerationMix, Granularity, IntensityStats, Measurement,
    Methodology, Region, RollupBucket, TimeRange, Vintage, VisitStats,
};
use carbonfr_core::ports::{
    CrossBorderRepository, IntensityRepository, RepositoryError, VisitCounter,
};
use time::{Date, Duration, OffsetDateTime};
use tower::ServiceExt;

/// Repository minimal : une mesure « courante », une série pour les plages, et
/// un compteur de visites en mémoire.
#[derive(Clone, Default)]
struct FakeRepo {
    measurement: Option<Measurement>,
    series: Vec<Measurement>,
    visits: Arc<Mutex<HashSet<(String, Date)>>>,
    flows: Option<CrossBorderSnapshot>,
    flow_series: Vec<CrossBorderSnapshot>,
    /// Empreintes de clés API valides (auth webhooks).
    api_keys: std::collections::HashSet<String>,
    /// Abonnements webhook en mémoire.
    subs: Arc<Mutex<Vec<carbonfr_core::domain::Subscription>>>,
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
impl CrossBorderRepository for FakeRepo {
    async fn upsert_flows(&self, _: &[CrossBorderSnapshot]) -> Result<usize, RepositoryError> {
        Ok(0)
    }

    async fn flows_at(
        &self,
        _: OffsetDateTime,
    ) -> Result<Option<CrossBorderSnapshot>, RepositoryError> {
        Ok(self.flows.clone())
    }

    async fn flows_range(
        &self,
        range: TimeRange,
    ) -> Result<Vec<CrossBorderSnapshot>, RepositoryError> {
        // `flow_series` si fourni (chemin prévision), sinon le snapshot unique.
        let source = if self.flow_series.is_empty() {
            self.flows.clone().into_iter().collect()
        } else {
            self.flow_series.clone()
        };
        Ok(source
            .into_iter()
            .filter(|s| range.contains(s.at))
            .collect())
    }
}

#[async_trait]
impl carbonfr_core::ports::ApiKeyRepository for FakeRepo {
    async fn resolve(
        &self,
        key_hash: &str,
    ) -> Result<Option<carbonfr_core::ports::ApiKeyRecord>, RepositoryError> {
        Ok(self
            .api_keys
            .contains(key_hash)
            .then(|| carbonfr_core::ports::ApiKeyRecord {
                tier: carbonfr_core::ports::ApiTier::Free,
                label: "test".to_string(),
            }))
    }
    async fn insert_key(
        &self,
        _: &str,
        _: carbonfr_core::ports::ApiTier,
        _: &str,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
}

#[async_trait]
impl carbonfr_core::ports::SubscriptionRepository for FakeRepo {
    async fn create(
        &self,
        subscription: &carbonfr_core::domain::Subscription,
    ) -> Result<(), RepositoryError> {
        self.subs.lock().unwrap().push(subscription.clone());
        Ok(())
    }
    async fn list_for_owner(
        &self,
        owner_key_hash: &str,
    ) -> Result<Vec<carbonfr_core::domain::Subscription>, RepositoryError> {
        Ok(self
            .subs
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.owner_key_hash == owner_key_hash)
            .cloned()
            .collect())
    }
    async fn delete(&self, id: &str, owner_key_hash: &str) -> Result<bool, RepositoryError> {
        let mut subs = self.subs.lock().unwrap();
        let before = subs.len();
        subs.retain(|s| !(s.id == id && s.owner_key_hash == owner_key_hash));
        Ok(subs.len() < before)
    }
    async fn active(&self) -> Result<Vec<carbonfr_core::domain::Subscription>, RepositoryError> {
        Ok(self.subs.lock().unwrap().clone())
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

/// Monte le routeur complet (lecture + prévision) sur un même repository *fake*.
/// La prévision utilise le vrai `ClimatologyForecaster` (test de bout en bout :
/// handler → cas d'usage/port → forecaster → repo → fonction pure).
fn build(repo: FakeRepo) -> axum::Router {
    let forecast = ForecastState::new(ClimatologyForecaster::new(repo.clone()), "climatology@1");
    let (updates, _) = tokio::sync::broadcast::channel(8);
    router(AppState::new(repo), forecast, StreamState::new(updates))
}

fn app(measurement: Option<Measurement>) -> axum::Router {
    build(FakeRepo {
        measurement,
        ..Default::default()
    })
}

fn app_with_series(series: Vec<Measurement>) -> axum::Router {
    build(FakeRepo {
        series,
        ..Default::default()
    })
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

/// `from` = 1970-03-02T00:00:00Z (UNIX_EPOCH + 60 jours), aligné minuit UTC.
fn forecast_from() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + Duration::days(60)
}

#[tokio::test]
async fn forecast_returns_predicted_series() {
    let from = forecast_from();
    let step = Duration::minutes(15);
    // 14 jours d'historique constant juste avant `from`.
    let series: Vec<Measurement> = (1..=14 * 96)
        .map(|i: i32| point(from - step * i, 40.0))
        .collect();

    let response = get(
        app_with_series(series),
        "/v1/intensity/forecast?from=1970-03-02T00:00:00Z&horizon_hours=24",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["model"], "climatology@1");
    assert_eq!(body["methodology"], "rte-direct");
    assert_eq!(body["horizon_hours"], 24);
    assert_eq!(body["unit"], "gCO2eq/kWh");
    // Pas 15 min sur 24 h → 96 points.
    assert_eq!(body["count"], 96);
    assert_eq!(body["data"][0]["timestamp"], "1970-03-02T00:00:00Z");
    // Historique constant → prévision ≈ 40, intervalle cohérent (lower ≤ expected ≤ upper).
    let pt = &body["data"][0];
    let (expected, lower, upper) = (
        pt["expected"].as_f64().unwrap(),
        pt["lower"].as_f64().unwrap(),
        pt["upper"].as_f64().unwrap(),
    );
    assert!((expected - 40.0).abs() < 1.0);
    assert!(lower <= expected && expected <= upper);
}

#[tokio::test]
async fn forecast_without_history_is_404() {
    let response = get(
        app_with_series(vec![]),
        "/v1/intensity/forecast?from=1970-03-02T00:00:00Z",
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(json_body(response).await["error"], "no_data");
}

#[tokio::test]
async fn forecast_horizon_too_large_is_400() {
    let response = get(app(None), "/v1/intensity/forecast?horizon_hours=100").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["error"], "bad_request");
}

#[tokio::test]
async fn greenest_window_finds_lowest_slot() {
    let from = forecast_from();
    let step = Duration::minutes(15);
    // Motif : creux la nuit (20), pointe le jour (80) — au pas natif pour que
    // tous les créneaux soient observés.
    let series: Vec<Measurement> = (1..=14 * 96)
        .map(|i: i32| {
            let at = from - step * i;
            let g = if (0..=5).contains(&at.hour()) {
                20.0
            } else {
                80.0
            };
            point(at, g)
        })
        .collect();

    let response = get(
        app_with_series(series),
        "/v1/intensity/greenest-window?from=1970-03-02T00:00:00Z&horizon_hours=24&window_minutes=60",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["model"], "climatology@1");
    assert!(body["start"].is_string());
    assert!(body["end"].is_string());
    // Le créneau le plus vert tombe la nuit (≈ 20), bien sous le jour.
    assert!(body["average_intensity"].as_f64().unwrap() < 40.0);
}

#[tokio::test]
async fn greenest_window_invalid_window_is_400() {
    // Créneau plus large que l'horizon → 400.
    let response = get(
        app(None),
        "/v1/intensity/greenest-window?horizon_hours=1&window_minutes=120",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn greenest_window_estimator_selector() {
    let from = forecast_from();
    let step = Duration::minutes(15);
    let series: Vec<Measurement> = (1..=14 * 96)
        .map(|i: i32| point(from - step * i, 50.0))
        .collect();

    // `prudent` est accepté (200).
    let ok = get(
        app_with_series(series),
        "/v1/intensity/greenest-window?from=1970-03-02T00:00:00Z&horizon_hours=24&estimator=prudent",
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);

    // Estimateur inconnu → 400.
    let bad = get(app(None), "/v1/intensity/greenest-window?estimator=bogus").await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn consumption_intensity_v2_uses_import_context() {
    use carbonfr_core::domain::{CrossBorderFlow, CrossBorderFlows, CrossBorderSnapshot, Neighbor};

    // Mesure acv-ademe@1 (porte le mix FR) + contexte d'import carboné.
    let mut measurement = national_measurement();
    measurement.methodology = Methodology::acv_ademe();
    let at = measurement.at;
    let repo = FakeRepo {
        measurement: Some(measurement),
        flows: Some(CrossBorderSnapshot {
            at,
            flows: CrossBorderFlows::new(vec![CrossBorderFlow {
                neighbor: Neighbor::Germany,
                flow_mw: 5000.0,
                neighbor_intensity: CarbonIntensity::new(400.0).unwrap(),
            }]),
        }),
        ..Default::default()
    };

    let response = get(
        build(repo),
        "/v1/intensity/now?methodology=acv-ademe&version=2",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["methodology"], "acv-ademe");
    assert_eq!(body["methodology_version"], 2);
    // Import charbon → intensité consommation nettement au-dessus du mix FR (~12).
    assert!(body["intensity"]["value"].as_f64().unwrap() > 15.0);
}

#[tokio::test]
async fn consumption_intensity_v2_rejects_regional() {
    let response = get(
        app(Some(national_measurement())),
        "/v1/intensity/now?region=bretagne&methodology=acv-ademe&version=2",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Série jour/nuit (creux 20 la nuit, pointe 80 le jour) sur 14 jours avant `from`.
fn day_night_series() -> Vec<Measurement> {
    let from = forecast_from();
    let step = Duration::minutes(15);
    (1..=14 * 96)
        .map(|i: i32| {
            let at = from - step * i;
            let g = if (0..=5).contains(&at.hour()) {
                20.0
            } else {
                80.0
            };
            point(at, g)
        })
        .collect()
}

#[tokio::test]
async fn schedule_returns_window_and_savings() {
    // Job d'1 h lancé « à midi » : le créneau vert tombe la nuit → économie.
    let response = get(
        app_with_series(day_night_series()),
        "/v1/schedule?from=1970-03-02T12:00:00Z&horizon_hours=24&duration_minutes=60&energy_kwh=10",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["model"], "climatology@1");
    assert_eq!(body["estimator"], "central");
    assert!(body["start"].is_string() && body["end"].is_string());
    // « maintenant » (midi, ≈80) au-dessus du créneau planifié (nuit, ≈20).
    let now = body["savings"]["now"].as_f64().unwrap();
    let scheduled = body["savings"]["scheduled"].as_f64().unwrap();
    assert!(now > scheduled);
    assert!(body["savings"]["reduction_percent"].as_f64().unwrap() > 0.0);
    // énergie fournie → économie absolue présente.
    assert!(body["savings"]["absolute_saved_g"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn schedule_slots_returns_k_cheapest() {
    let response = get(
        app_with_series(day_night_series()),
        "/v1/schedule/slots?from=1970-03-02T00:00:00Z&horizon_hours=24&count=4",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["count"], 4);
    assert_eq!(body["slots"].as_array().unwrap().len(), 4);
    // Les moins intenses → tous côté creux nocturne.
    for slot in body["slots"].as_array().unwrap() {
        assert!(slot["intensity"].as_f64().unwrap() < 50.0);
    }
}

#[tokio::test]
async fn schedule_slots_without_count_is_400() {
    let response = get(app_with_series(day_night_series()), "/v1/schedule/slots").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn intensity_below_filters_threshold() {
    let response = get(
        app_with_series(day_night_series()),
        "/v1/intensity/below?from=1970-03-02T00:00:00Z&horizon_hours=24&threshold=50",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body["count"].as_u64().unwrap() > 0);
    for slot in body["slots"].as_array().unwrap() {
        assert!(slot["intensity"].as_f64().unwrap() < 50.0);
    }
}

#[tokio::test]
async fn intensity_below_without_threshold_is_400() {
    let response = get(app_with_series(day_night_series()), "/v1/intensity/below").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn stream_opens_event_stream() {
    let response = get(app(None), "/v1/intensity/stream").await;
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.starts_with("text/event-stream"),
        "content-type = {content_type}"
    );
    // On ne consomme pas le corps : c'est un flux sans fin (keep-alive).
}

#[tokio::test]
async fn stream_rejects_unknown_region() {
    let response = get(app(None), "/v1/intensity/stream?region=pas-une-region").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Série `acv-ademe@1` (porte le mix FR) sur `n` heures depuis l'epoch.
fn consumption_history_repo(n: i32) -> FakeRepo {
    use carbonfr_core::domain::{CrossBorderFlow, CrossBorderFlows, Neighbor};
    let t0 = OffsetDateTime::UNIX_EPOCH;
    let step = Duration::hours(1);
    let mix = national_measurement().mix.unwrap();
    let series: Vec<Measurement> = (0..n)
        .map(|i| Measurement {
            at: t0 + step * i,
            region: Region::National,
            intensity: CarbonIntensity::new(12.0).unwrap(),
            methodology: Methodology::acv_ademe(),
            vintage: Vintage::Consolidated,
            mix: Some(mix),
        })
        .collect();
    FakeRepo {
        series,
        flows: Some(CrossBorderSnapshot {
            at: t0,
            flows: CrossBorderFlows::new(vec![CrossBorderFlow {
                neighbor: Neighbor::Germany,
                flow_mw: 5000.0,
                neighbor_intensity: CarbonIntensity::new(400.0).unwrap(),
            }]),
        }),
        ..Default::default()
    }
}

#[tokio::test]
async fn intensity_date_consumption_v2_series() {
    let response = get(
        build(consumption_history_repo(3)),
        "/v1/intensity/date?from=1970-01-01T00:00:00Z&to=1970-01-01T05:00:00Z&methodology=acv-ademe&version=2",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["methodology"], "acv-ademe");
    assert_eq!(body["count"], 3);
    // Import charbon → chaque point au-dessus de la production seule (~12,56).
    for pt in body["data"].as_array().unwrap() {
        assert!(pt["intensity"].as_f64().unwrap() > 12.56);
    }
}

#[tokio::test]
async fn intensity_stats_consumption_v2_summary() {
    let response = get(
        build(consumption_history_repo(3)),
        "/v1/intensity/stats?from=1970-01-01T00:00:00Z&to=1970-01-01T05:00:00Z&methodology=acv-ademe&version=2",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["methodology"], "acv-ademe");
    assert_eq!(body["count"], 3);
    assert!(body["average"].as_f64().unwrap() > 12.56);
}

#[tokio::test]
async fn intensity_date_consumption_v2_rejects_regional() {
    let response = get(
        app(None),
        "/v1/intensity/date?from=1970-01-01T00:00:00Z&to=1970-01-01T05:00:00Z&region=bretagne&methodology=acv-ademe&version=2",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Monte le routeur avec le modèle de prévision acv-ademe@2 câblé (ADR-0013).
fn build_with_acv(repo: FakeRepo) -> axum::Router {
    use carbonfr_adapter_forecast::AcvAdemeForecaster;
    use std::sync::Arc;
    let forecast = ForecastState::new(ClimatologyForecaster::new(repo.clone()), "climatology@1")
        .with_consumption(
            Arc::new(AcvAdemeForecaster::new(repo.clone(), repo.clone())),
            "acv-clim@1",
        );
    let (updates, _) = tokio::sync::broadcast::channel(8);
    router(AppState::new(repo), forecast, StreamState::new(updates))
}

#[tokio::test]
async fn forecast_consumption_v2_uses_dedicated_model() {
    use carbonfr_core::domain::{CrossBorderFlow, CrossBorderFlows, CrossBorderSnapshot, Neighbor};
    let from = forecast_from();
    let step = Duration::hours(1);
    let mix = national_measurement().mix.unwrap();
    // 14 jours d'historique acv-ademe@1 (porte le mix) + contexte d'import.
    let series: Vec<Measurement> = (1..=14 * 24)
        .map(|i: i32| Measurement {
            at: from - step * i,
            region: Region::National,
            intensity: CarbonIntensity::new(12.0).unwrap(),
            methodology: Methodology::acv_ademe(),
            vintage: Vintage::Consolidated,
            mix: Some(mix),
        })
        .collect();
    let flow_series: Vec<CrossBorderSnapshot> = (1..=14 * 24)
        .map(|i: i32| CrossBorderSnapshot {
            at: from - step * i,
            flows: CrossBorderFlows::new(vec![CrossBorderFlow {
                neighbor: Neighbor::Germany,
                flow_mw: 3000.0,
                neighbor_intensity: CarbonIntensity::new(400.0).unwrap(),
            }]),
        })
        .collect();
    let repo = FakeRepo {
        series,
        flow_series,
        ..Default::default()
    };

    let response = get(
        build_with_acv(repo),
        "/v1/intensity/forecast?from=1970-03-02T00:00:00Z&horizon_hours=24&methodology=acv-ademe&version=2",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["model"], "acv-clim@1");
    assert_eq!(body["methodology"], "acv-ademe");
    assert!(body["count"].as_u64().unwrap() > 0);
    // Intensité consommation plausible (import carboné au-dessus de la prod).
    assert!(body["data"][0]["expected"].as_f64().unwrap() > 12.56);
}

#[tokio::test]
async fn forecast_consumption_v2_unwired_is_404() {
    // Routeur sans modèle @2 câblé → 404 explicite.
    let response = get(
        app(None),
        "/v1/intensity/forecast?methodology=acv-ademe&version=2",
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── Middleware d'auth + quota (ADR-0015) ──────────────────────────────────────

use carbonfr_adapter_http::{AuthConfig, AuthState, enforce, key_fingerprint};
use carbonfr_core::ports::{ApiKeyRecord, ApiKeyRepository, ApiTier};

struct FakeKeys {
    valid_hash: String,
}

#[async_trait]
impl ApiKeyRepository for FakeKeys {
    async fn resolve(&self, key_hash: &str) -> Result<Option<ApiKeyRecord>, RepositoryError> {
        Ok((key_hash == self.valid_hash).then(|| ApiKeyRecord {
            tier: ApiTier::Free,
            label: "test".to_string(),
        }))
    }
    async fn insert_key(&self, _: &str, _: ApiTier, _: &str) -> Result<(), RepositoryError> {
        Ok(())
    }
}

fn guarded_app() -> axum::Router {
    use std::sync::Arc;
    let keys = Arc::new(FakeKeys {
        valid_hash: key_fingerprint("good-key"),
    });
    let state = AuthState::new(
        keys,
        AuthConfig {
            anonymous_per_min: 2,
            free_per_min: 100,
            trust_proxy: false,
        },
    );
    axum::Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(state, enforce))
}

async fn get_auth(app: axum::Router, uri: &str, bearer: Option<&str>) -> axum::response::Response {
    let mut req = Request::get(uri);
    if let Some(token) = bearer {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
}

#[tokio::test]
async fn auth_anonymous_is_rate_limited() {
    let app = guarded_app();
    // Limite anonyme = 2/min (IP « unknown » partagée en test).
    assert_eq!(
        get_auth(app.clone(), "/health", None).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        get_auth(app.clone(), "/health", None).await.status(),
        StatusCode::OK
    );
    let limited = get_auth(app.clone(), "/health", None).await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers().contains_key("ratelimit-limit"));
}

#[tokio::test]
async fn auth_unknown_key_is_401() {
    let response = get_auth(guarded_app(), "/health", Some("wrong")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_valid_key_gets_higher_limit() {
    let app = guarded_app();
    // Avec la bonne clé (limite 100) : la 3e requête passe encore.
    for _ in 0..3 {
        let ok = get_auth(app.clone(), "/health", Some("good-key")).await;
        assert_eq!(ok.status(), StatusCode::OK);
    }
}

// ── Endpoints webhooks (ADR-0016) ─────────────────────────────────────────────

fn webhook_app() -> axum::Router {
    let mut keys = std::collections::HashSet::new();
    keys.insert(key_fingerprint("wh-key"));
    build(FakeRepo {
        api_keys: keys,
        ..Default::default()
    })
}

async fn send(
    app: axum::Router,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    body: Option<&str>,
) -> axum::response::Response {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(token) = bearer {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let body = Body::from(body.unwrap_or("").to_string());
    app.oneshot(req.body(body).unwrap()).await.unwrap()
}

#[tokio::test]
async fn webhook_create_requires_key() {
    let body =
        r#"{"threshold":50,"direction":"below","callback_url":"https://hooks.example.com/c"}"#;
    let resp = send(webhook_app(), "POST", "/v1/webhooks", None, Some(body)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn webhook_create_rejects_ssrf_url() {
    let body = r#"{"threshold":50,"direction":"below","callback_url":"https://127.0.0.1/c"}"#;
    let resp = send(
        webhook_app(),
        "POST",
        "/v1/webhooks",
        Some("wh-key"),
        Some(body),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn webhook_create_list_delete_roundtrip() {
    let app = webhook_app();
    let body =
        r#"{"threshold":50,"direction":"below","callback_url":"https://hooks.example.com/c"}"#;

    // Création → 201 + secret + id.
    let created = send(
        app.clone(),
        "POST",
        "/v1/webhooks",
        Some("wh-key"),
        Some(body),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let cbody = json_body(created).await;
    assert!(cbody["secret"].as_str().unwrap().len() >= 32);
    let id = cbody["id"].as_str().unwrap().to_string();

    // Liste → contient l'abonnement (sans secret).
    let listed = send(app.clone(), "GET", "/v1/webhooks", Some("wh-key"), None).await;
    assert_eq!(listed.status(), StatusCode::OK);
    let lbody = json_body(listed).await;
    assert_eq!(lbody["count"], 1);
    assert!(lbody["webhooks"][0].get("secret").is_none());

    // Suppression → 204, puis liste vide.
    let deleted = send(
        app.clone(),
        "DELETE",
        &format!("/v1/webhooks/{id}"),
        Some("wh-key"),
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let after = send(app, "GET", "/v1/webhooks", Some("wh-key"), None).await;
    assert_eq!(json_body(after).await["count"], 0);
}

#[tokio::test]
async fn health_ready_checks_db() {
    // Repo qui répond (Ok) → 200 « ready ».
    let response = get(app(Some(national_measurement())), "/health/ready").await;
    assert_eq!(response.status(), StatusCode::OK);
    // Même sans donnée (Ok(None)), la base est joignable → 200.
    let empty = get(app(None), "/health/ready").await;
    assert_eq!(empty.status(), StatusCode::OK);
}

#[tokio::test]
async fn rate_limit_not_bypassed_by_spoofed_xff() {
    // trust_proxy = false (défaut) : des X-Forwarded-For différents tombent tous
    // dans le seau « unknown » → le spoofing ne contourne pas le quota.
    let app = guarded_app(); // anonymous_per_min = 2
    let send_xff = |xff: &str| {
        let req = Request::get("/health")
            .header("x-forwarded-for", xff)
            .body(Body::empty())
            .unwrap();
        app.clone().oneshot(req)
    };
    assert_eq!(send_xff("1.2.3.4").await.unwrap().status(), StatusCode::OK);
    assert_eq!(send_xff("5.6.7.8").await.unwrap().status(), StatusCode::OK);
    // 3e requête, IP encore différente → quota épuisé malgré l'IP forgée.
    assert_eq!(
        send_xff("9.9.9.9").await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}
