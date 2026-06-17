//! Spécification **OpenAPI 3.1** de l'API `/v1`, dérivée du code via `utoipa`
//! (code-first : les `#[utoipa::path]` des handlers et les `#[derive(ToSchema)]`
//! des DTO sont la source de vérité), et page **Swagger UI**.
//!
//! Servie en JSON sous `/v1/openapi.json` ; `/docs` rend une page Swagger UI
//! qui la charge. Le `core` n'est pas touché : seuls les DTO de l'adapter
//! portent `ToSchema` (frontière de l'hexagone).

use axum::Json;
use axum::response::Html;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDoc;

/// Document OpenAPI agrégé de l'API carbon-fr.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "carbon-fr",
        // La version est câblée dynamiquement dans `document()` sur la version de
        // la crate (`CARGO_PKG_VERSION`) — pas de placeholder qui se périme.
        description = "Intensité carbone (gCO₂eq/kWh) de l'électricité française, \
                       d'après les données ouvertes RTE/éCO2mix (ODRÉ). Couverture : \
                       National + 12 régions. Méthodologies : `rte-direct` (estimation \
                       RTE, combustion directe, national) et `acv-ademe` (cycle de vie \
                       ADEME, national + régions).",
        license(name = "MIT OR Apache-2.0"),
        contact(name = "Kovelt", url = "https://kovelt.fr"),
    ),
    paths(
        crate::handlers::intensity_now,
        crate::handlers::intensity_date,
        crate::handlers::intensity_stats,
        crate::handlers::mix,
        crate::handlers::exchanges,
        crate::handlers::exchanges_date,
        crate::handlers::weather,
        crate::handlers::weather_date,
        crate::handlers::renewable,
        crate::handlers::methodologies,
        crate::handlers::factors,
        crate::handlers::forecast,
        crate::handlers::greenest_window,
        crate::handlers::schedule,
        crate::handlers::schedule_slots,
        crate::handlers::intensity_below,
        crate::handlers::intensity_stream,
        crate::handlers::visit_stats,
        crate::handlers::record_visit,
        crate::handlers::create_webhook,
        crate::handlers::list_webhooks,
        crate::handlers::delete_webhook,
        crate::handlers::health,
        crate::handlers::health_ready,
    ),
    components(schemas(
        crate::dto::IntensityResponse,
        crate::dto::HistoryResponse,
        crate::dto::StatsResponse,
        crate::dto::MixResponse,
        crate::dto::ExchangesResponse,
        crate::dto::ExchangesHistoryResponse,
        crate::dto::WeatherResponse,
        crate::dto::WeatherHistoryResponse,
        crate::dto::RenewableResponse,
        crate::dto::MethodologiesResponse,
        crate::dto::MethodologyInfo,
        crate::dto::FactorsResponse,
        crate::dto::FactorEntry,
        crate::dto::ForecastResponse,
        crate::dto::GreenestWindowResponse,
        crate::dto::ScheduleResponse,
        crate::dto::SavingsBody,
        crate::dto::SlotsResponse,
        crate::dto::SlotBody,
        crate::dto::CreateWebhookRequest,
        crate::dto::CreatedWebhookResponse,
        crate::dto::WebhookListResponse,
        crate::dto::WebhookSummary,
        crate::dto::VisitStatsResponse,
        crate::dto::StreamEventBody,
        crate::error::ErrorBody,
    )),
    tags(
        (name = "intensité", description = "Intensité carbone"),
        (name = "mix", description = "Mix de production"),
        (name = "échanges", description = "Échanges transfrontaliers (ENTSO-E, ADR-0017)"),
        (name = "météo", description = "Météo nationale (Open-Meteo CC-BY 4.0, ADR-0012/0018)"),
        (name = "renouvelable", description = "Dérivation renouvelable météo→production (ADR-0018)"),
        (name = "méthodologie", description = "Méthodes de calcul & facteurs (ADR-0010)"),
        (name = "prévision", description = "Prévision d'intensité (ADR-0009)"),
        (name = "usage", description = "Scheduling carbon-aware (ADR-0014)"),
        (name = "webhooks", description = "Abonnements webhook (ADR-0016, clé requise)"),
        (name = "opérations", description = "Exploitation & statistiques"),
    ),
)]
pub(crate) struct ApiDoc;

/// Document OpenAPI généré depuis le code.
pub(crate) fn document() -> OpenApiDoc {
    let mut doc = ApiDoc::openapi();
    // Version de l'API = version de la crate (ADR-0019, version unique de
    // workspace). Évite tout placeholder figé visible sur `/docs`.
    doc.info.version = env!("CARGO_PKG_VERSION").to_string();
    doc
}

/// `GET /v1/openapi.json` — la spécification OpenAPI.
pub(crate) async fn openapi() -> Json<OpenApiDoc> {
    Json(document())
}

/// `GET /docs` — page Swagger UI (assets chargés depuis le CDN jsDelivr).
pub(crate) async fn swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_UI_HTML)
}

const SWAGGER_UI_HTML: &str = r##"<!doctype html>
<html lang="fr">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>carbon-fr — API</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui.css" />
  </head>
  <body>
    <div id="swagger-ui"></div>
    <script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui-bundle.js" crossorigin></script>
    <script>
      window.ui = SwaggerUIBundle({ url: "/v1/openapi.json", dom_id: "#swagger-ui" });
    </script>
  </body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_lists_all_paths() {
        let doc = document();
        for path in [
            "/v1/intensity/now",
            "/v1/intensity/date",
            "/v1/intensity/stats",
            "/v1/mix",
            "/v1/exchanges",
            "/v1/exchanges/date",
            "/v1/weather",
            "/v1/weather/date",
            "/v1/renewable",
            "/v1/methodologies",
            "/v1/factors",
            "/v1/intensity/forecast",
            "/v1/intensity/greenest-window",
            "/v1/schedule",
            "/v1/schedule/slots",
            "/v1/intensity/below",
            "/v1/intensity/stream",
            "/v1/stats",
            "/v1/stats/visit",
            "/v1/webhooks",
            "/v1/webhooks/{id}",
            "/health",
            "/health/ready",
        ] {
            assert!(
                doc.paths.paths.contains_key(path),
                "chemin manquant : {path}"
            );
        }
    }

    /// Garde-fou de **contrat**. L'OpenAPI servi sous `/v1` est une promesse
    /// faite à des consommateurs externes qu'on ne peut pas prévenir. Ce test
    /// fige le document généré dans un instantané commité
    /// (`tests/openapi.snapshot.json`) : toute évolution du contrat (chemin,
    /// schéma, champ, code de retour) fait échouer la CI et devient un acte
    /// *volontaire*, visible dans le diff de la PR. La version applicative —
    /// axe orthogonal au contrat (ADR-0019) — est neutralisée pour ne pas casser
    /// l'instantané à chaque release.
    ///
    /// Après un changement de contrat **intentionnel**, régénérer puis relire le
    /// diff dans la PR :
    /// `UPDATE_OPENAPI_SNAPSHOT=1 cargo test -p carbonfr-adapter-http openapi_contract_snapshot`
    #[test]
    fn openapi_contract_snapshot() {
        let mut doc = document();
        // Découple le contrat de la version applicative (ADR-0019).
        doc.info.version = "{{version}}".to_string();
        let actual = serde_json::to_string_pretty(&doc).expect("sérialisation OpenAPI");

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/openapi.snapshot.json");

        if std::env::var_os("UPDATE_OPENAPI_SNAPSHOT").is_some() {
            std::fs::write(path, format!("{actual}\n")).expect("écriture de l'instantané");
            return;
        }

        let expected = std::fs::read_to_string(path).unwrap_or_default();
        assert_eq!(
            actual.trim(),
            expected.trim(),
            "\n\nLe contrat OpenAPI /v1 a changé par rapport à l'instantané commité.\n\
             • si c'est INVOLONTAIRE : c'est une rupture de contrat, corrige le code ;\n\
             • si c'est VOLONTAIRE : régénère l'instantané et relis son diff dans ta PR :\n    \
             UPDATE_OPENAPI_SNAPSHOT=1 cargo test -p carbonfr-adapter-http openapi_contract_snapshot\n"
        );
    }

    #[test]
    fn document_lists_schemas() {
        let doc = document();
        let components = doc.components.expect("components");
        for schema in [
            "IntensityResponse",
            "MixResponse",
            "ExchangesResponse",
            "WeatherResponse",
            "RenewableResponse",
            "ForecastResponse",
            "StreamEventBody",
            "ErrorBody",
        ] {
            assert!(
                components.schemas.contains_key(schema),
                "schéma manquant : {schema}"
            );
        }
    }
}
