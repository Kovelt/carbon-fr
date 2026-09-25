//! Tests hermétiques des 25 méthodes REST (`crate::methods` — le flux SSE a
//! les siens dans `src/stream.rs`) : un serveur `axum` local par test
//! (patron déjà posé par `client.rs`/`stream.rs`, `crates/adapter-webhook`)
//! vérifie (a) la méthode HTTP et le chemin reçus, (b) certains paramètres de
//! requête (ou le corps JSON pour les POST), (c) que la fixture correspondante
//! de `tests/fixtures/` se désérialise avec succès dans le DTO attendu et
//! quelques valeurs plausibles.
//!
//! Aucun appel réseau réel : `CarbonFr::builder().base_url(&base_url)…`
//! pointe toujours vers `127.0.0.1:<port éphémère>`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::Response;
use axum::routing::{delete, get, post};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use carbonfr_sdk::*;

// --- Infrastructure de test partagée ---------------------------------------

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("lecture fixture {path} : {e}"))
}

fn dt(s: &str) -> OffsetDateTime {
    OffsetDateTime::parse(s, &Rfc3339)
        .unwrap_or_else(|e| panic!("horodatage de test « {s} » : {e}"))
}

/// Requête reçue par le serveur de test — méthode, chemin, query string brute
/// (parsée à la demande par les tests via `query_params`), corps et en-tête
/// `Authorization`.
#[derive(Debug, Clone, Default)]
struct CapturedRequest {
    method: String,
    path: String,
    query: String,
    body: String,
    authorization: Option<String>,
}

impl CapturedRequest {
    fn query_params(&self) -> HashMap<String, String> {
        url::form_urlencoded::parse(self.query.as_bytes())
            .into_owned()
            .collect()
    }
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Option<CapturedRequest>>>);

impl Captured {
    fn take(&self) -> CapturedRequest {
        self.0
            .lock()
            .unwrap()
            .clone()
            .expect("aucune requête reçue")
    }
}

/// La fixture à renvoyer : statut, `Content-Type`, corps.
#[derive(Clone)]
struct FixtureResponse {
    status: StatusCode,
    content_type: &'static str,
    body: String,
}

impl FixtureResponse {
    fn json(body: impl Into<String>) -> Self {
        Self {
            status: StatusCode::OK,
            content_type: "application/json",
            body: body.into(),
        }
    }

    fn json_created(body: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CREATED,
            content_type: "application/json",
            body: body.into(),
        }
    }

    fn no_content() -> Self {
        Self {
            status: StatusCode::NO_CONTENT,
            content_type: "text/plain",
            body: String::new(),
        }
    }
}

/// Handler unique réutilisé par toutes les routes de test : capture la
/// requête reçue puis renvoie la fixture configurée dans l'état partagé.
async fn respond_with_fixture(
    State((captured, fixture)): State<(Captured, FixtureResponse)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    *captured.0.lock().unwrap() = Some(CapturedRequest {
        method: method.to_string(),
        path: uri.path().to_string(),
        query: uri.query().unwrap_or_default().to_string(),
        body: String::from_utf8_lossy(&body).into_owned(),
        authorization: headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
    });
    Response::builder()
        .status(fixture.status)
        .header(header::CONTENT_TYPE, fixture.content_type)
        .body(axum::body::Body::from(fixture.body))
        .unwrap()
}

/// Démarre un serveur de test à une seule route (méthode HTTP + gabarit de
/// chemin axum, ex. `/v1/webhooks/{id}`) qui répond toujours `fixture`.
/// Retourne l'URL de base du client, l'état `Captured` à interroger après
/// l'appel, et le `JoinHandle` du serveur (abandonné — donc annulé — à la fin
/// du test).
async fn spawn(
    method: Method,
    route: &str,
    fixture: FixtureResponse,
) -> (String, Captured, tokio::task::JoinHandle<()>) {
    let captured = Captured::default();
    let method_router = match method {
        Method::GET => get(respond_with_fixture),
        Method::POST => post(respond_with_fixture),
        Method::DELETE => delete(respond_with_fixture),
        other => panic!("méthode de test non gérée : {other}"),
    };
    let app = Router::new()
        .route(route, method_router)
        .with_state((captured.clone(), fixture));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind du serveur de test");
    let addr = listener.local_addr().expect("adresse locale");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), captured, handle)
}

fn client(base_url: &str) -> CarbonFr {
    CarbonFr::builder()
        .base_url(base_url)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("client de test")
}

fn client_with_api_key(base_url: &str, api_key: &str) -> CarbonFr {
    CarbonFr::builder()
        .base_url(base_url)
        .api_key(api_key)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("client de test")
}

// --- Intensité ---------------------------------------------------------

#[tokio::test]
async fn intensity_now_hits_expected_path_and_decodes_fixture() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/intensity/now",
        FixtureResponse::json(fixture("intensity_now.json")),
    )
    .await;

    let resp = client(&base_url)
        .intensity_now(IntensityNowOptions {
            region: Some(Region::Bretagne),
            methodology: Some(Methodology::AcvAdeme),
            version: Some(2),
        })
        .await
        .expect("réponse 200");

    let req = captured.take();
    assert_eq!(req.method, "GET");
    assert_eq!(req.path, "/v1/intensity/now");
    let params = req.query_params();
    assert_eq!(params["region"], "bretagne");
    assert_eq!(params["methodology"], "acv-ademe");
    assert_eq!(params["version"], "2");

    assert_eq!(resp.region, Region::National); // valeur de la fixture, pas de la requête
    assert_eq!(resp.methodology, Methodology::RteDirect);
    assert!((resp.intensity.value - 34.0).abs() < f64::EPSILON);
    assert_eq!(resp.vintage, "tr");
}

#[tokio::test]
async fn intensity_date_sends_required_from_to_and_decodes_series() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/intensity/date",
        FixtureResponse::json(fixture("intensity_date.json")),
    )
    .await;

    let resp = client(&base_url)
        .intensity_date(
            dt("2026-09-24T00:00:00Z"),
            dt("2026-09-25T00:00:00Z"),
            IntensityDateOptions::default(),
        )
        .await
        .expect("réponse 200");

    let req = captured.take();
    let params = req.query_params();
    assert_eq!(params["from"], "2026-09-24T00:00:00Z");
    assert_eq!(params["to"], "2026-09-25T00:00:00Z");
    assert!(!params.contains_key("region"));

    assert_eq!(resp.count, 94);
    assert_eq!(resp.data.len(), 94);
    assert_eq!(resp.data[0].vintage, "tr");
}

#[tokio::test]
async fn intensity_stats_sends_interval_and_decodes_buckets() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/intensity/stats",
        FixtureResponse::json(fixture("intensity_stats.json")),
    )
    .await;

    let resp = client(&base_url)
        .intensity_stats(
            dt("2026-09-24T00:00:00Z"),
            dt("2026-09-25T00:00:00Z"),
            IntensityStatsOptions {
                interval: Some(Interval::Hour),
                ..Default::default()
            },
        )
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["interval"], "hour");

    assert_eq!(resp.count, 94);
    assert_eq!(resp.interval, Some(Interval::Hour));
    let intervals = resp.intervals.expect("intervalles attendus");
    assert!(!intervals.is_empty());
    assert_eq!(intervals[0].start, dt("2026-09-24T00:00:00Z"));
}

#[tokio::test]
async fn below_sends_required_threshold_and_decodes_slots() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/intensity/below",
        FixtureResponse::json(fixture("intensity_below.json")),
    )
    .await;

    let resp = client(&base_url)
        .below(40.0, BelowOptions::default())
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["threshold"], "40");

    assert_eq!(resp.count, 96);
    assert_eq!(resp.estimator, Estimator::Central);
    assert_eq!(resp.slots.len(), 96);
}

// --- Prévision -----------------------------------------------------------

#[tokio::test]
async fn forecast_decodes_confidence_interval() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/intensity/forecast",
        FixtureResponse::json(fixture("forecast.json")),
    )
    .await;

    let resp = client(&base_url)
        .forecast(ForecastOptions {
            horizon_hours: Some(24),
            ..Default::default()
        })
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["horizon_hours"], "24");

    assert_eq!(resp.model, "climatology@1");
    assert_eq!(resp.count, 96);
    assert!(resp.data[0].lower <= resp.data[0].expected);
    assert!(resp.data[0].expected <= resp.data[0].upper);
}

#[tokio::test]
async fn greenest_window_sends_window_minutes_and_decodes_window() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/intensity/greenest-window",
        FixtureResponse::json(fixture("greenest_window.json")),
    )
    .await;

    let resp = client(&base_url)
        .greenest_window(GreenestWindowOptions {
            window_minutes: Some(60),
            estimator: Some(Estimator::Prudent),
            ..Default::default()
        })
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["window_minutes"], "60");
    assert_eq!(params["estimator"], "prudent");
    assert!(!params.contains_key("eligibility"));

    assert_eq!(resp.model, "climatology@1");
    assert!(resp.eligibility.is_none());
    assert!(resp.start < resp.end);
}

#[tokio::test]
async fn greenest_window_sends_eligibility_overrides() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/intensity/greenest-window",
        FixtureResponse::json(fixture("greenest_window.json")),
    )
    .await;

    client(&base_url)
        .greenest_window(GreenestWindowOptions {
            eligibility: Some(EligibilityFramework::Rfnbo),
            eligibility_version: Some("rfnbo:2023-1184".to_string()),
            surplus_price_eur_mwh: Some(20.0),
            ..Default::default()
        })
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["eligibility"], "rfnbo");
    assert_eq!(params["eligibility_version"], "rfnbo:2023-1184");
    assert_eq!(params["surplus_price_eur_mwh"], "20");
}

// --- Scheduling ------------------------------------------------------------

#[tokio::test]
async fn schedule_decodes_savings() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/schedule",
        FixtureResponse::json(fixture("schedule.json")),
    )
    .await;

    let resp = client(&base_url)
        .schedule(ScheduleOptions {
            duration_minutes: Some(60),
            energy_kwh: Some(2.5),
            ..Default::default()
        })
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["duration_minutes"], "60");
    assert_eq!(params["energy_kwh"], "2.5");

    assert!(resp.savings.absolute_saved_g.is_some());
    assert!(resp.savings.intensity_delta > 0.0);
}

#[tokio::test]
async fn schedule_slots_sends_required_count_and_caps_response() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/schedule/slots",
        FixtureResponse::json(fixture("schedule_slots.json")),
    )
    .await;

    let resp = client(&base_url)
        .schedule_slots(5, ScheduleSlotsOptions::default())
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["count"], "5");

    assert_eq!(resp.count, 5);
    assert_eq!(resp.slots.len(), 5);
}

// --- Mix ---------------------------------------------------------------

#[tokio::test]
async fn mix_decodes_national_body_without_thermique() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/mix",
        FixtureResponse::json(fixture("mix.json")),
    )
    .await;

    let resp = client(&base_url)
        .mix(MixOptions::default())
        .await
        .expect("réponse 200");

    assert_eq!(captured.take().path, "/v1/mix");
    assert_eq!(resp.region, Region::National);
    assert!((resp.mix.nucleaire - 38967.0).abs() < f64::EPSILON);
    // Le national n'expose pas `thermique` (mix régional uniquement).
    assert!(resp.mix.thermique.is_none());
}

// --- Échanges ------------------------------------------------------------

#[tokio::test]
async fn exchanges_decodes_borders() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/exchanges",
        FixtureResponse::json(fixture("exchanges.json")),
    )
    .await;

    let resp = client(&base_url).exchanges().await.expect("réponse 200");

    let req = captured.take();
    assert_eq!(req.method, "GET");
    assert_eq!(req.path, "/v1/exchanges");

    assert_eq!(resp.direction, FlowDirection::Export);
    assert_eq!(resp.exchanges.len(), 5);
    assert_eq!(resp.exchanges[0].country, "be");
}

#[tokio::test]
async fn exchanges_history_sends_required_from_to() {
    let border = fixture("exchanges.json");
    let snapshot: serde_json::Value = serde_json::from_str(&border).unwrap();
    let body = serde_json::json!({
        "from": "2026-09-24T00:00:00Z",
        "to": "2026-09-25T00:00:00Z",
        "count": 1,
        "snapshots": [snapshot],
    })
    .to_string();

    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/exchanges/date",
        FixtureResponse::json(body),
    )
    .await;

    let resp = client(&base_url)
        .exchanges_history(dt("2026-09-24T00:00:00Z"), dt("2026-09-25T00:00:00Z"))
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["from"], "2026-09-24T00:00:00Z");
    assert_eq!(params["to"], "2026-09-25T00:00:00Z");

    assert_eq!(resp.count, 1);
    assert_eq!(resp.snapshots.len(), 1);
}

// --- Météo -----------------------------------------------------------------

#[tokio::test]
async fn weather_decodes_national_average() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/weather",
        FixtureResponse::json(fixture("weather.json")),
    )
    .await;

    let resp = client(&base_url).weather().await.expect("réponse 200");

    assert_eq!(captured.take().path, "/v1/weather");
    assert!(resp.source.contains("Open-Meteo"));
    assert!(resp.wind_kmh > 0.0);
}

#[tokio::test]
async fn weather_history_sends_required_from_to() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/weather/date",
        FixtureResponse::json(fixture("weather_date.json")),
    )
    .await;

    let resp = client(&base_url)
        .weather_history(dt("2026-09-24T00:00:00Z"), dt("2026-09-25T00:00:00Z"))
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["from"], "2026-09-24T00:00:00Z");
    assert_eq!(params["to"], "2026-09-25T00:00:00Z");

    assert_eq!(resp.count, 24);
    assert_eq!(resp.points.len(), 24);
}

// --- Renouvelable ----------------------------------------------------------

#[tokio::test]
async fn renewable_decodes_model_capacities() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/renewable",
        FixtureResponse::json(fixture("renewable.json")),
    )
    .await;

    let resp = client(&base_url).renewable().await.expect("réponse 200");

    assert_eq!(captured.take().path, "/v1/renewable");
    assert!(resp.model.wind_capacity_mw > 0.0);
    assert!(resp.model.solar_capacity_mw > 0.0);
}

// --- Méthodologies et facteurs ----------------------------------------------

#[tokio::test]
async fn methodologies_decodes_catalog() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/methodologies",
        FixtureResponse::json(fixture("methodologies.json")),
    )
    .await;

    let resp = client(&base_url)
        .methodologies()
        .await
        .expect("réponse 200");

    assert_eq!(captured.take().path, "/v1/methodologies");
    assert_eq!(resp.methodologies.len(), 3);
    assert_eq!(resp.methodologies[0].id, "rte-direct");
    assert!(resp.methodologies[0].default);
}

#[tokio::test]
async fn factors_decodes_table() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/factors",
        FixtureResponse::json(fixture("factors.json")),
    )
    .await;

    let resp = client(&base_url)
        .factors(FactorsOptions {
            methodology: Some(Methodology::AcvAdeme),
            version: Some(2),
        })
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["methodology"], "acv-ademe");
    assert_eq!(params["version"], "2");

    assert_eq!(resp.methodology, Methodology::AcvAdeme);
    assert_eq!(resp.td_loss_factor, Some(0.072));
    assert!(resp.factors.iter().any(|f| f.filiere == "nucleaire"));
}

// --- Prix et coût de référence ----------------------------------------------

#[tokio::test]
async fn price_decodes_components_and_context() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/price",
        FixtureResponse::json(fixture("price.json")),
    )
    .await;

    let resp = client(&base_url)
        .price(PriceOptions::default())
        .await
        .expect("réponse 200");

    assert_eq!(captured.take().path, "/v1/price");
    assert_eq!(resp.vintage, "2026-H2");
    assert!(!resp.components.is_empty());
    assert!(!resp.context.mix.is_empty());
}

#[tokio::test]
async fn price_history_sends_required_from_to() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/price/date",
        FixtureResponse::json(fixture("price_date.json")),
    )
    .await;

    let resp = client(&base_url)
        .price_history(
            dt("2026-09-24T00:00:00Z"),
            dt("2026-09-25T00:00:00Z"),
            PriceHistoryOptions::default(),
        )
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["from"], "2026-09-24T00:00:00Z");
    assert_eq!(params["to"], "2026-09-25T00:00:00Z");

    assert_eq!(resp.count, 94);
    assert_eq!(resp.points.len(), 94);
}

#[tokio::test]
async fn cost_reference_sends_filters_and_decodes_entries() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/cost-reference",
        FixtureResponse::json(fixture("cost_reference.json")),
    )
    .await;

    let resp = client(&base_url)
        .cost_reference(CostReferenceOptions {
            source: Some("ademe".to_string()),
            technology: Some("solaire-pv".to_string()),
            ..Default::default()
        })
        .await
        .expect("réponse 200");

    let params = captured.take().query_params();
    assert_eq!(params["source"], "ademe");
    assert_eq!(params["technology"], "solaire-pv");

    assert_eq!(resp.kind, "estimation");
    assert_eq!(resp.count, 13);
    assert_eq!(resp.entries.len(), 13);
}

// --- Éligibilité -------------------------------------------------------

#[tokio::test]
async fn eligibility_rulesets_decodes_catalog() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/eligibility/rulesets",
        FixtureResponse::json(fixture("eligibility_rulesets.json")),
    )
    .await;

    let resp = client(&base_url)
        .eligibility_rulesets()
        .await
        .expect("réponse 200");

    assert_eq!(captured.take().path, "/v1/eligibility/rulesets");
    assert_eq!(resp.rulesets.len(), 3);
    assert_eq!(resp.rulesets[0].framework, EligibilityFramework::Rfnbo);
    assert_eq!(
        resp.rulesets[0].hourly_switchover.as_deref(),
        Some("2030-01-01")
    );
    assert_eq!(resp.rulesets[1].framework, EligibilityFramework::LowCarbon);
    assert!(resp.rulesets[1].hourly_switchover.is_none());
}

// --- Statistiques de visite --------------------------------------------

#[tokio::test]
async fn visit_stats_decodes_counters() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/stats",
        FixtureResponse::json(fixture("visit_stats.json")),
    )
    .await;

    let resp = client(&base_url).visit_stats().await.expect("réponse 200");

    assert_eq!(captured.take().method, "GET");
    assert_eq!(resp.unique, 507);
    assert_eq!(resp.total, 600);
    assert_eq!(resp.since.as_deref(), Some("2026-06-16"));
}

#[tokio::test]
async fn record_visit_posts_without_body_and_decodes_updated_counters() {
    let (base_url, captured, _server) = spawn(
        Method::POST,
        "/v1/stats/visit",
        FixtureResponse::json(fixture("record_visit.synthetic.json")),
    )
    .await;

    let resp = client(&base_url).record_visit().await.expect("réponse 200");

    let req = captured.take();
    assert_eq!(req.method, "POST");
    assert_eq!(req.path, "/v1/stats/visit");
    assert!(req.body.is_empty());

    assert_eq!(resp.unique, 508);
    assert_eq!(resp.total, 601);
}

// --- Webhooks ------------------------------------------------------------

#[tokio::test]
async fn webhook_methods_require_api_key_before_any_network_call() {
    // Pas de serveur démarré du tout : si l'un de ces appels tentait une
    // requête réseau, il échouerait par timeout/erreur de connexion, pas par
    // `CarbonFrError::Config` — la distinction est le test.
    let unconfigured = CarbonFr::builder()
        .base_url("http://127.0.0.1:1") // port 1 : rien n'écoute jamais ici
        .timeout(std::time::Duration::from_millis(50))
        .build()
        .expect("client de test");

    let err = unconfigured.list_webhooks().await.unwrap_err();
    assert!(matches!(err, CarbonFrError::Config(_)));

    let err = unconfigured
        .create_webhook(CreateWebhookRequest {
            threshold: 50.0,
            direction: ThresholdDirection::Below,
            callback_url: "https://example.com/hook".to_string(),
            region: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, CarbonFrError::Config(_)));

    let err = unconfigured.delete_webhook("abc").await.unwrap_err();
    assert!(matches!(err, CarbonFrError::Config(_)));
}

#[tokio::test]
async fn list_webhooks_sends_bearer_and_decodes_summaries() {
    let (base_url, captured, _server) = spawn(
        Method::GET,
        "/v1/webhooks",
        FixtureResponse::json(fixture("list_webhooks.synthetic.json")),
    )
    .await;

    let resp = client_with_api_key(&base_url, "test-key-123")
        .list_webhooks()
        .await
        .expect("réponse 200");

    let req = captured.take();
    assert_eq!(req.authorization.as_deref(), Some("Bearer test-key-123"));

    assert_eq!(resp.count, 2);
    assert_eq!(resp.webhooks[0].status, WebhookStatus::Active);
    assert!(resp.webhooks[0].disabled_at.is_none());
    assert_eq!(resp.webhooks[1].status, WebhookStatus::Disabled);
    assert_eq!(
        resp.webhooks[1].disabled_at,
        Some(dt("2026-09-20T08:15:00Z"))
    );
}

#[tokio::test]
async fn create_webhook_sends_json_body_and_decodes_secret() {
    let (base_url, captured, _server) = spawn(
        Method::POST,
        "/v1/webhooks",
        FixtureResponse::json_created(fixture("create_webhook.synthetic.json")),
    )
    .await;

    let resp = client_with_api_key(&base_url, "test-key-123")
        .create_webhook(CreateWebhookRequest {
            threshold: 50.0,
            direction: ThresholdDirection::Below,
            callback_url: "https://example.com/webhooks/carbon-fr".to_string(),
            region: None,
        })
        .await
        .expect("réponse 201");

    let req = captured.take();
    assert_eq!(req.method, "POST");
    assert_eq!(req.authorization.as_deref(), Some("Bearer test-key-123"));
    let sent: serde_json::Value = serde_json::from_str(&req.body).expect("corps JSON");
    assert_eq!(sent["threshold"], 50.0);
    assert_eq!(sent["direction"], "below");
    assert_eq!(
        sent["callback_url"],
        "https://example.com/webhooks/carbon-fr"
    );
    assert!(sent["region"].is_null());

    assert!(resp.secret.starts_with("whsec_"));
    assert_eq!(resp.direction, ThresholdDirection::Below);
}

#[tokio::test]
async fn delete_webhook_sends_id_in_path_and_returns_unit_on_204() {
    let (base_url, captured, _server) = spawn(
        Method::DELETE,
        "/v1/webhooks/{id}",
        FixtureResponse::no_content(),
    )
    .await;

    client_with_api_key(&base_url, "test-key-123")
        .delete_webhook("00000000-0000-4000-8000-000000000001")
        .await
        .expect("réponse 204");

    let req = captured.take();
    assert_eq!(req.method, "DELETE");
    assert_eq!(
        req.path,
        "/v1/webhooks/00000000-0000-4000-8000-000000000001"
    );
    assert_eq!(req.authorization.as_deref(), Some("Bearer test-key-123"));
}
