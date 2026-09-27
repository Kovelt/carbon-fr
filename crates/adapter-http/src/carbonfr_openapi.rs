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
    servers(
        (url = "https://carbon-fr-api.kovelt.fr", description = "Instance hébergée (Kovelt)"),
        (url = "http://localhost:8080", description = "Instance locale (cargo run -p server)"),
    ),
    paths(
        crate::handlers::intensity_now,
        crate::handlers::intensity_now_all,
        crate::handlers::intensity_date,
        crate::handlers::intensity_stats,
        crate::handlers::mix,
        crate::handlers::exchanges,
        crate::handlers::exchanges_date,
        crate::handlers::weather,
        crate::handlers::weather_date,
        crate::handlers::renewable,
        crate::handlers::methodologies,
        crate::handlers::regions,
        crate::handlers::eligibility_rulesets,
        crate::handlers::factors,
        crate::handlers::price,
        crate::handlers::price_date,
        crate::handlers::cost_reference,
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
        crate::dto::IntensityAllResponse,
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
        crate::dto::RegionsResponse,
        crate::dto::RegionInfo,
        crate::dto::FactorsResponse,
        crate::dto::FactorEntry,
        crate::dto::PriceResponse,
        crate::dto::PriceHistoryResponse,
        crate::dto::CostReferenceResponse,
        crate::dto::ForecastResponse,
        crate::dto::GreenestWindowResponse,
        crate::dto::EligibilityBody,
        crate::dto::EligibilitySlotBody,
        crate::dto::EligibilitySignalBody,
        crate::dto::EligibleSlotBody,
        crate::dto::RulesetsResponse,
        crate::dto::RulesetInfo,
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
        crate::error::ProblemDetails,
    )),
    tags(
        (name = "intensité", description = "Intensité carbone"),
        (name = "mix", description = "Mix de production"),
        (name = "échanges", description = "Échanges transfrontaliers (ENTSO-E, ADR-0017)"),
        (name = "météo", description = "Météo nationale (Open-Meteo CC-BY 4.0, ADR-0012/0018)"),
        (name = "renouvelable", description = "Dérivation renouvelable météo→production (ADR-0018)"),
        (name = "méthodologie", description = "Méthodes de calcul & facteurs (ADR-0010)"),
        (name = "régions", description = "Catalogue des régions servies (national + 12 régions, ADR-0008)"),
        (name = "éligibilité", description = "Éligibilité électrolyseur RFNBO / bas-carbone (ADR-0025/0026, neutre)"),
        (name = "prix", description = "Prix de l'électricité & couche LCOE (ADR-0023/0024)"),
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
    restore_parameter_descriptions(&mut doc);
    doc
}

/// Remonte au niveau du **paramètre** la description portée par son schéma.
///
/// `#[param(schema_with = …)]` (paramètre `region`, plan I8 : enum dérivé de
/// `Region`) fait générer par utoipa 6 un `Parameter` **sans** `description`
/// — la branche `schema_with` de la macro court-circuite la description tirée
/// du commentaire `///` ; seule celle de l'`Object` retourné subsiste
/// (`schema.description`). Or Swagger UI et la plupart des générateurs de
/// clients lisent `parameter.description`. On la recopie donc, pour tout
/// paramètre qui en manque et dont le schéma inline en porte une — sans rien
/// écraser d'existant.
fn restore_parameter_descriptions(doc: &mut OpenApiDoc) {
    use utoipa::openapi::path::Operation;
    use utoipa::openapi::{RefOr, Schema};

    fn fix(operation: &mut Operation) {
        let Some(parameters) = operation.parameters.as_mut() else {
            return;
        };
        for parameter in parameters.iter_mut() {
            let RefOr::T(parameter) = parameter else {
                continue;
            };
            if parameter.description.is_some() {
                continue;
            }
            if let Some(RefOr::T(Schema::Object(object))) = parameter.schema.as_ref() {
                parameter.description = object.description.clone();
            }
        }
    }

    for item in doc.paths.paths.values_mut() {
        for operation in [
            item.get.as_mut(),
            item.put.as_mut(),
            item.post.as_mut(),
            item.delete.as_mut(),
            item.options.as_mut(),
            item.head.as_mut(),
            item.patch.as_mut(),
            item.trace.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            fix(operation);
        }
    }
}

/// `GET /v1/openapi.json` — la spécification OpenAPI.
pub(crate) async fn openapi() -> Json<OpenApiDoc> {
    Json(document())
}

/// `GET /docs` — page Swagger UI (assets chargés depuis le CDN jsDelivr).
///
/// Version **épinglée exacte** + Subresource Integrity (audit 2026-08) : la
/// version flottante `@5` laissait le CDN servir n'importe quel contenu futur
/// dans une page de notre origine. Hashes SHA-384 calculés sur les fichiers
/// npm `swagger-ui-dist@5.32.13` (identiques via jsDelivr et unpkg,
/// vérification croisée du 2026-08-15). Toute montée de version doit
/// recalculer les deux hashes (`openssl dgst -sha384 -binary | openssl base64 -A`).
pub(crate) async fn swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_UI_HTML)
}

const SWAGGER_UI_HTML: &str = r##"<!doctype html>
<html lang="fr">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>carbon-fr — API</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.32.13/swagger-ui.css" integrity="sha384-tRpWwikYYdk1+1Mu0osh0Tz/Ay5xgS+s/Nf2Aa7GVAFtZLFdJlAbozfrq4g+xHBK" crossorigin="anonymous" />
  </head>
  <body>
    <div id="swagger-ui"></div>
    <script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.32.13/swagger-ui-bundle.js" integrity="sha384-PsJla434CobCNv3y1K4wRavOqkUAvwGEQEfbUmI98CCqqGCJsmuDsgIjM6ZQQODP" crossorigin="anonymous"></script>
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
            "/v1/intensity/now/all",
            "/v1/intensity/date",
            "/v1/intensity/stats",
            "/v1/mix",
            "/v1/regions",
            "/v1/exchanges",
            "/v1/exchanges/date",
            "/v1/weather",
            "/v1/weather/date",
            "/v1/renewable",
            "/v1/methodologies",
            "/v1/eligibility/rulesets",
            "/v1/factors",
            "/v1/price",
            "/v1/price/date",
            "/v1/cost-reference",
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

    /// Paramètre `region` documenté en **enum** (plan I8, PROD-API-5), avec la
    /// description propre à chaque contexte — `schema_with` court-circuite la
    /// description du commentaire `///`, ce test garantit qu'elle n'est pas
    /// perdue : 13 valeurs et défaut national sur les lectures, filtre du flux
    /// SSE, `national` seul sur les prix.
    #[test]
    fn region_parameter_is_an_enum_with_contextual_descriptions() {
        let doc = serde_json::to_value(document()).expect("sérialisation OpenAPI");
        let region_param = |path: &str| -> serde_json::Value {
            doc["paths"][path]["get"]["parameters"]
                .as_array()
                .unwrap_or_else(|| panic!("paramètres de {path}"))
                .iter()
                .find(|p| p["name"] == "region")
                .unwrap_or_else(|| panic!("paramètre region de {path}"))
                .clone()
        };

        let now = region_param("/v1/intensity/now");
        let values = now["schema"]["enum"].as_array().expect("enum");
        assert_eq!(values.len(), 13);
        assert_eq!(values[0], "national");
        assert!(values.contains(&serde_json::json!("bretagne")));
        assert!(now["required"] != true, "region reste optionnel");
        assert!(
            now["schema"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("National par défaut")
        );
        // Recopiée au niveau du paramètre (`restore_parameter_descriptions`),
        // là où Swagger UI et les générateurs de clients la lisent.
        assert_eq!(now["description"], now["schema"]["description"]);
        // Les paramètres sans `schema_with` gardent leur propre description.
        let methodology = doc["paths"]["/v1/intensity/now"]["get"]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "methodology")
            .cloned()
            .expect("paramètre methodology");
        assert!(
            methodology["description"]
                .as_str()
                .unwrap_or_default()
                .contains("rte-direct")
        );

        let stream = region_param("/v1/intensity/stream");
        assert_eq!(stream["schema"]["enum"].as_array().expect("enum").len(), 13);
        assert!(
            stream["schema"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("toutes les régions sont poussées")
        );

        for path in ["/v1/price", "/v1/price/date"] {
            let price = region_param(path);
            assert_eq!(price["schema"]["enum"], serde_json::json!(["national"]));
            assert_eq!(price["description"], price["schema"]["description"]);
            assert!(
                price["schema"]["description"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("National uniquement")
            );
        }

        // Pas de paramètre `region` sur la route « toutes régions ».
        let all = doc["paths"]["/v1/intensity/now/all"]["get"]["parameters"]
            .as_array()
            .expect("paramètres");
        assert!(all.iter().all(|p| p["name"] != "region"));
    }

    #[test]
    fn document_lists_servers() {
        let doc = document();
        let servers = doc.servers.expect("servers");
        let urls: Vec<&str> = servers.iter().map(|s| s.url.as_str()).collect();
        assert_eq!(
            urls,
            ["https://carbon-fr-api.kovelt.fr", "http://localhost:8080"],
            "l'instance hébergée doit être déclarée en premier (défaut de Swagger UI)"
        );
    }

    #[test]
    fn document_lists_schemas() {
        let doc = document();
        let components = doc.components.expect("components");
        for schema in [
            "IntensityResponse",
            "IntensityAllResponse",
            "RegionsResponse",
            "RegionInfo",
            "MixResponse",
            "ExchangesResponse",
            "WeatherResponse",
            "RenewableResponse",
            "PriceResponse",
            "CostReferenceResponse",
            "ForecastResponse",
            "EligibilityBody",
            "RulesetsResponse",
            "StreamEventBody",
            "ProblemDetails",
        ] {
            assert!(
                components.schemas.contains_key(schema),
                "schéma manquant : {schema}"
            );
        }
    }
}
