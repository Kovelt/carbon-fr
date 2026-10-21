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
        crate::handlers::intensity_index,
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
        crate::dto::IntensityIndexLabel,
        crate::dto::IntensityIndexResponse,
        crate::dto::IndexBand,
        crate::dto::IndexThreshold,
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
    document_middleware_responses(&mut doc);
    doc
}

/// Documente ce que les **middlewares** ajoutent aux réponses, hors des
/// handlers (PROD-API-6, plan I10), en lisant les mêmes constantes que le
/// code (`CACHEABLE_PATHS`, `STABLE_CACHE_CONTROL`, noms des en-têtes de
/// quota) — les textes, eux, restent écrits ici :
///
/// - `Cache-Control` sur la réponse `200` des lectures stables de
///   [`crate::CACHEABLE_PATHS`] (middleware `cache_control`, `GET` seulement) ;
/// - pour le tier hébergé (ADR-0015, **opt-in** : middleware `enforce`), sur
///   toute opération `/v1` : `RateLimit-Limit` / `RateLimit-Remaining` sur
///   chaque réponse `2xx` (posés sur toute réponse admise par le quota), la
///   réponse `429` `rate_limited` (en-têtes `RateLimit-*` + `Retry-After`) et
///   la réponse `401` `unauthorized` (clé `Bearer` inconnue), partagées dans
///   `components.responses`. Jamais sur `/health*`, `/metrics` ni `/docs`,
///   exclus du quota par construction, ni sur `OPTIONS` (préflight CORS,
///   servi avant le quota).
fn document_middleware_responses(doc: &mut OpenApiDoc) {
    use utoipa::PartialSchema;
    use utoipa::openapi::path::Operation;
    use utoipa::openapi::schema::{Object, Type};
    use utoipa::openapi::{Content, Header, Ref, RefOr, Response};

    const RATE_LIMITED: &str = "RateLimited";
    const UNAUTHORIZED: &str = "Unauthorized";
    const QUOTA_HEADER_NOTE: &str = "Présent seulement si l'instance active le tier hébergé (ADR-0015) ; \
                                     absent sinon.";

    fn string_header(description: String) -> RefOr<Header> {
        let mut header = Header::new(Object::with_type(Type::String));
        header.description = Some(description);
        RefOr::T(header)
    }

    /// Même schéma que les en-têtes déclarés `u32` par les handlers (entier
    /// `int32`, minimum 0) — ex. le `Retry-After` du 503 SSE.
    fn u32_header(description: &str) -> RefOr<Header> {
        let mut header = Header::new(<u32 as PartialSchema>::schema());
        header.description = Some(description.to_string());
        RefOr::T(header)
    }

    fn problem_json(description: &str) -> Response {
        let mut response = Response::new(description);
        response.content.insert(
            "application/problem+json".to_string(),
            RefOr::T(Content::new(Some(Ref::from_schema_name("ProblemDetails")))),
        );
        response
    }

    // Réponses partagées du tier hébergé.
    let components = doc.components.get_or_insert_with(Default::default);
    let mut rate_limited = problem_json(
        "Quota dépassé (`rate_limited`) — tier hébergé **opt-in** (ADR-0015). Fenêtre \
         **fixe** d'une minute : les requêtes anonymes sont comptées par adresse IP \
         quand l'instance est derrière un proxy de confiance (`CARBONFR_TRUST_PROXY`), \
         sinon dans un seul seau anonyme partagé ; celles portant une clé `Bearer` \
         valide sont comptées par clé — mais une clé invalide est débitée du seau de \
         l'IP, et une clé valide utilisée depuis une IP dont le seau anonyme est épuisé \
         reçoit ce 429 jusqu'à la minute suivante. Réessayer après `Retry-After`. \
         Désactivé (défaut), l'instance n'envoie jamais cette réponse ni d'en-tête \
         `RateLimit-*`.",
    );
    rate_limited.headers.insert(
        "RateLimit-Limit".to_string(),
        u32_header("Requêtes autorisées par minute pour ce seau."),
    );
    rate_limited.headers.insert(
        "RateLimit-Remaining".to_string(),
        u32_header("Requêtes restantes dans la minute courante (0 sur un 429)."),
    );
    rate_limited.headers.insert(
        "Retry-After".to_string(),
        u32_header("Secondes à attendre avant une nouvelle tentative (début de la minute suivante, 1 à 60)."),
    );
    components
        .responses
        .entry(RATE_LIMITED.to_string())
        .or_insert(RefOr::T(rate_limited));
    let mut unauthorized = problem_json(
        "Clé API inconnue (`unauthorized`) — tier hébergé **opt-in** (ADR-0015) : seulement \
         quand la requête porte un `Authorization: Bearer` que l'instance ne reconnaît pas \
         (jamais pour une requête anonyme, admise par défaut). L'échec est débité du seau \
         anonyme de l'IP. Si la clé ne peut pas être vérifiée (base indisponible), la \
         réponse est un 503 `unavailable`.",
    );
    unauthorized.headers.insert(
        "WWW-Authenticate".to_string(),
        string_header("`Bearer error=\"invalid_token\"` (RFC 6750 §3).".to_string()),
    );
    components
        .responses
        .entry(UNAUTHORIZED.to_string())
        .or_insert(RefOr::T(unauthorized));

    fn add_cache_control(operation: &mut Operation) {
        if let Some(RefOr::T(response)) = operation.responses.responses.get_mut("200") {
            response
                .headers
                .entry("Cache-Control".to_string())
                .or_insert_with(|| {
                    string_header(format!(
                        "`{}` : lecture stable, qu'un cache HTTP (navigateur, proxy, CDN) peut \
                         servir jusqu'à 60 s — au plus 60 s de retard sur une donnée rafraîchie \
                         au cycle du poller (~15 min par défaut) ou fixe (catalogues). Jamais \
                         posé sur une erreur.",
                        crate::STABLE_CACHE_CONTROL
                    ))
                });
        }
    }

    fn add_hosted_tier(operation: &mut Operation) {
        for (status, response) in operation.responses.responses.iter_mut() {
            if !status.starts_with('2') {
                continue;
            }
            let RefOr::T(response) = response else {
                continue;
            };
            response
                .headers
                .entry("RateLimit-Limit".to_string())
                .or_insert_with(|| {
                    u32_header(&format!(
                        "Requêtes autorisées par minute pour ce seau. {QUOTA_HEADER_NOTE}"
                    ))
                });
            response
                .headers
                .entry("RateLimit-Remaining".to_string())
                .or_insert_with(|| {
                    u32_header(&format!(
                        "Requêtes restantes dans la minute courante, celle-ci déduite. \
                         {QUOTA_HEADER_NOTE}"
                    ))
                });
        }
        operation
            .responses
            .responses
            .entry("429".to_string())
            .or_insert_with(|| RefOr::Ref(Ref::from_response_name(RATE_LIMITED)));
        // Les opérations qui déclarent déjà leur 401 (webhooks : clé requise)
        // gardent leur description ; l'en-tête, posé par `error_response` sur
        // tout 401, y est ajouté s'il manque.
        match operation
            .responses
            .responses
            .entry("401".to_string())
            .or_insert_with(|| RefOr::Ref(Ref::from_response_name(UNAUTHORIZED)))
        {
            RefOr::T(response) => {
                response
                    .headers
                    .entry("WWW-Authenticate".to_string())
                    .or_insert_with(|| {
                        string_header("`Bearer error=\"invalid_token\"` (RFC 6750 §3).".to_string())
                    });
            }
            RefOr::Ref(_) => {}
        }
    }

    for (path, item) in doc.paths.paths.iter_mut() {
        if crate::CACHEABLE_PATHS.contains(&path.as_str())
            && let Some(operation) = item.get.as_mut()
        {
            add_cache_control(operation);
        }
        if !path.starts_with("/v1") {
            continue;
        }
        // Pas `options` : le préflight CORS est servi avant le quota.
        for operation in [
            item.get.as_mut(),
            item.put.as_mut(),
            item.post.as_mut(),
            item.delete.as_mut(),
            item.head.as_mut(),
            item.patch.as_mut(),
            item.trace.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            add_hosted_tier(operation);
        }
    }
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
            "/v1/intensity/index",
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

    /// L'exemple `text/csv` de `/v1/intensity/date` dans le document OpenAPI
    /// commence par l'en-tête réellement écrit (`export::HEADER`) : la doc
    /// publiée ne peut pas dériver du contrat CSV (colonne `index` comprise).
    #[test]
    fn csv_example_header_matches_the_export_contract() {
        let doc = serde_json::to_value(document()).expect("sérialisation OpenAPI");
        let example = doc["paths"]["/v1/intensity/date"]["get"]["responses"]["200"]["content"]
            ["text/csv"]["example"]
            .as_str()
            .expect("exemple text/csv de /v1/intensity/date");
        assert_eq!(
            example.lines().next(),
            Some(crate::export::HEADER.trim_end())
        );
        for row in example.lines().skip(1) {
            assert_eq!(
                row.split(',').count(),
                crate::export::HEADER.trim_end().split(',').count(),
                "{row}"
            );
        }
    }

    /// Méthodes HTTP d'un *path item* OpenAPI (ses autres clés — `parameters`,
    /// `summary`… — ne sont pas des opérations).
    const HTTP_METHODS: [&str; 8] = [
        "get", "put", "post", "delete", "options", "head", "patch", "trace",
    ];

    /// Déréférence un `$ref` (`#/components/<section>/<nom>`) ; sinon renvoie le nœud.
    fn resolve<'a>(
        node: &'a serde_json::Value,
        doc: &'a serde_json::Value,
    ) -> &'a serde_json::Value {
        let Some(reference) = node["$ref"].as_str() else {
            return node;
        };
        let mut target = doc;
        for segment in reference.trim_start_matches("#/").split('/') {
            target = &target[segment];
        }
        assert!(!target.is_null(), "référence introuvable : {reference}");
        resolve(target, doc)
    }

    /// Valide un exemple contre son schéma — le sous-ensemble d'OpenAPI 3.1
    /// qu'utoipa produit ici : `$ref`, `allOf` (parties fusionnées : union des
    /// propriétés, comme `#[serde(flatten)]`), `oneOf`/`anyOf` (au moins une
    /// branche), `type` simple ou multiple, `enum`, `minimum`/`maximum`,
    /// `required`, `properties` (clé inconnue = erreur), `additionalProperties`
    /// (valeurs validées), `items`. Un exemple n'a pas le droit de mentir sur
    /// le contrat : chaque écart est collecté dans `errors` avec son chemin.
    fn validate_example(
        value: &serde_json::Value,
        schema: &serde_json::Value,
        doc: &serde_json::Value,
        at: String,
        errors: &mut Vec<String>,
    ) {
        use serde_json::Value;
        let schema = resolve(schema, doc);
        let merged;
        let schema = if let Some(parts) = schema["allOf"].as_array() {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            let mut kind = Value::Null;
            for part in parts {
                let part = resolve(part, doc);
                if let Some(props) = part["properties"].as_object() {
                    properties.extend(props.iter().map(|(k, v)| (k.clone(), v.clone())));
                }
                if let Some(list) = part["required"].as_array() {
                    required.extend(list.iter().cloned());
                }
                if kind.is_null() {
                    kind = part["type"].clone();
                }
            }
            merged =
                serde_json::json!({ "type": kind, "properties": properties, "required": required });
            &merged
        } else {
            schema
        };
        if let Some(branches) = schema
            .get("oneOf")
            .or_else(|| schema.get("anyOf"))
            .and_then(Value::as_array)
        {
            let matched = branches.iter().any(|branch| {
                let mut local = Vec::new();
                validate_example(value, branch, doc, at.clone(), &mut local);
                local.is_empty()
            });
            if !matched {
                errors.push(format!(
                    "{at} : aucune branche oneOf/anyOf n'accepte {value}"
                ));
            }
            return;
        }
        let types: Vec<&str> = match &schema["type"] {
            Value::String(single) => vec![single.as_str()],
            Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !types.is_empty() {
            let accepted = types.iter().any(|kind| match *kind {
                "object" => value.is_object(),
                "array" => value.is_array(),
                "string" => value.is_string(),
                "integer" => value.is_i64() || value.is_u64(),
                "number" => value.is_number(),
                "boolean" => value.is_boolean(),
                "null" => value.is_null(),
                _ => true,
            });
            if !accepted {
                errors.push(format!("{at} : {value} n'est pas de type {types:?}"));
                return;
            }
        }
        if let Some(allowed) = schema["enum"].as_array()
            && !allowed.contains(value)
        {
            errors.push(format!("{at} : {value} hors de l'enum {allowed:?}"));
        }
        if let Some(number) = value.as_f64() {
            if let Some(minimum) = schema["minimum"].as_f64()
                && number < minimum
            {
                errors.push(format!("{at} : {number} < minimum {minimum}"));
            }
            if let Some(maximum) = schema["maximum"].as_f64()
                && number > maximum
            {
                errors.push(format!("{at} : {number} > maximum {maximum}"));
            }
        }
        if let Some(object) = value.as_object() {
            for required in schema["required"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !object.contains_key(required) {
                    errors.push(format!("{at} : propriété requise absente : {required}"));
                }
            }
            let properties = schema["properties"].as_object();
            let additional = &schema["additionalProperties"];
            for (key, member) in object {
                match properties.and_then(|p| p.get(key)) {
                    Some(member_schema) => {
                        validate_example(member, member_schema, doc, format!("{at}.{key}"), errors)
                    }
                    None if additional.is_object() => {
                        validate_example(member, additional, doc, format!("{at}.{key}"), errors)
                    }
                    None if properties.is_some() && additional != &Value::Bool(true) => {
                        errors.push(format!("{at} : propriété inconnue du schéma : {key}"))
                    }
                    None => {}
                }
            }
        }
        if let Some(items) = value.as_array()
            && !schema["items"].is_null()
        {
            for (i, item) in items.iter().enumerate() {
                validate_example(item, &schema["items"], doc, format!("{at}[{i}]"), errors);
            }
        }
    }

    /// Un exemple de réponse `200` (`application/json`) d'une opération `GET`.
    fn example_of<'a>(doc: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
        doc["paths"][path]["get"]["responses"]["200"]["content"]["application/json"]
            .get("example")
            .unwrap_or_else(|| panic!("{path} : pas d'exemple sur la réponse 200"))
    }

    /// PROD-API-6 (plan I10) : les quatre lectures les plus consultées portent
    /// un exemple de réponse `200`, et cet exemple respecte le schéma déclaré
    /// (clés, types, enum `index`, propriétés requises, bornes).
    #[test]
    fn examples_of_the_four_main_reads_match_their_schemas() {
        let doc = serde_json::to_value(document()).expect("sérialisation OpenAPI");
        for path in [
            "/v1/intensity/now",
            "/v1/mix",
            "/v1/intensity/forecast",
            "/v1/intensity/greenest-window",
        ] {
            let content =
                &doc["paths"][path]["get"]["responses"]["200"]["content"]["application/json"];
            assert!(
                !content["schema"].is_null(),
                "{path} : pas de schéma sur la réponse 200"
            );
            let mut errors = Vec::new();
            validate_example(
                example_of(&doc, path),
                &content["schema"],
                &doc,
                path.to_string(),
                &mut errors,
            );
            assert!(
                errors.is_empty(),
                "{path} : exemple non conforme au schéma :\n  {}",
                errors.join("\n  ")
            );
        }
    }

    /// Ce que le schéma ne peut pas dire, l'exemple doit quand même le
    /// respecter : l'`index` est la bande de la valeur (ADR-0033), `count` est
    /// la taille de `data`, les parts du mix somment à 1.
    #[test]
    fn examples_are_internally_consistent() {
        use carbonfr_core::domain::CarbonIntensity;

        let doc = serde_json::to_value(document()).expect("sérialisation OpenAPI");
        let band_of = |grams: f64| {
            let intensity = CarbonIntensity::new(grams).expect("intensité d'exemple valide");
            serde_json::to_value(crate::dto::IntensityIndexLabel::of(intensity)).unwrap()
        };

        let now = example_of(&doc, "/v1/intensity/now");
        assert_eq!(
            now["index"],
            band_of(now["intensity"]["value"].as_f64().unwrap())
        );

        let forecast = example_of(&doc, "/v1/intensity/forecast");
        let points = forecast["data"].as_array().unwrap();
        assert_eq!(forecast["count"].as_u64(), Some(points.len() as u64));
        assert_eq!(forecast["from"], points[0]["timestamp"]);
        for point in points {
            assert_eq!(
                point["index"],
                band_of(point["expected"].as_f64().unwrap()),
                "{point}"
            );
            assert!(point["lower"].as_f64() <= point["expected"].as_f64());
            assert!(point["expected"].as_f64() <= point["upper"].as_f64());
        }

        let mix = example_of(&doc, "/v1/mix");
        let total: f64 = mix["shares"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["share"].as_f64().unwrap())
            .sum();
        assert!((total - 1.0).abs() < 0.01, "somme des parts = {total}");

        let window = example_of(&doc, "/v1/intensity/greenest-window");
        assert!(window["start"].as_str() < window["end"].as_str());
    }

    /// `Cache-Control` est documenté exactement là où le middleware le pose :
    /// `GET`, réponse `200`, chemin de `CACHEABLE_PATHS` — et nulle part
    /// ailleurs ; avec la valeur réellement envoyée.
    #[test]
    fn cache_control_is_documented_exactly_where_the_middleware_sets_it() {
        let doc = serde_json::to_value(document()).expect("sérialisation OpenAPI");
        assert!(
            crate::CACHEABLE_PATHS
                .iter()
                .all(|path| !path.contains('{')),
            "le middleware compare des chemins concrets : pas de gabarit dans CACHEABLE_PATHS"
        );
        let mut documented = 0;
        for (path, item) in doc["paths"].as_object().expect("paths") {
            for method in HTTP_METHODS {
                let Some(responses) = item[method]["responses"].as_object() else {
                    continue;
                };
                for (status, response) in responses {
                    let header = &resolve(response, &doc)["headers"]["Cache-Control"];
                    let expected = method == "get"
                        && status == "200"
                        && crate::CACHEABLE_PATHS.contains(&path.as_str());
                    assert_eq!(
                        !header.is_null(),
                        expected,
                        "{method} {path} {status} : Cache-Control {}",
                        if expected {
                            "manquant"
                        } else {
                            "documenté à tort"
                        }
                    );
                    if expected {
                        documented += 1;
                        assert!(
                            header["description"]
                                .as_str()
                                .unwrap_or_default()
                                .contains(crate::STABLE_CACHE_CONTROL),
                            "{path} : la description doit citer la valeur réelle"
                        );
                        assert_eq!(header["schema"]["type"], "string", "{path}");
                    }
                }
            }
        }
        assert_eq!(documented, crate::CACHEABLE_PATHS.len());
    }

    /// Le tier hébergé (ADR-0015) est documenté sur toute opération `/v1`, et
    /// seulement là : `RateLimit-Limit`/`RateLimit-Remaining` sur chaque `2xx`
    /// (noms = ceux posés par `enforce`), `429` et `401` partagés dans
    /// `components.responses` avec leurs en-têtes et un corps Problem Details.
    #[test]
    fn v1_operations_document_the_hosted_tier_responses() {
        let doc = serde_json::to_value(document()).expect("sérialisation OpenAPI");
        let quota_headers = ["RateLimit-Limit", "RateLimit-Remaining"];
        assert_eq!(
            quota_headers[0].to_ascii_lowercase(),
            crate::auth::RATELIMIT_LIMIT_HEADER
        );
        assert_eq!(
            quota_headers[1].to_ascii_lowercase(),
            crate::auth::RATELIMIT_REMAINING_HEADER
        );

        let mut seen = 0;
        for (path, item) in doc["paths"].as_object().expect("paths") {
            for method in HTTP_METHODS {
                let Some(responses) = item[method]["responses"].as_object() else {
                    continue;
                };
                let hosted = path.starts_with("/v1");
                if hosted {
                    seen += 1;
                }
                for (status, response) in responses {
                    let response = resolve(response, &doc);
                    let has_quota = quota_headers
                        .iter()
                        .all(|name| response["headers"][*name]["schema"]["type"] == "integer");
                    // Sur toute réponse admise par le quota (2xx) et sur le 429 lui-même.
                    assert_eq!(
                        has_quota,
                        hosted && (status.starts_with('2') || status == "429"),
                        "{method} {path} {status} : en-têtes RateLimit-*"
                    );
                }
                for status in ["429", "401"] {
                    let response = responses.get(status).map(|r| resolve(r, &doc));
                    assert_eq!(
                        response.is_some(),
                        hosted,
                        "{method} {path} : réponse {status}"
                    );
                    let Some(response) = response else {
                        continue;
                    };
                    assert_eq!(
                        response["content"]["application/problem+json"]["schema"]["$ref"],
                        "#/components/schemas/ProblemDetails",
                        "{method} {path} : corps du {status}"
                    );
                    let expected_headers: &[&str] = if status == "429" {
                        &["RateLimit-Limit", "RateLimit-Remaining", "Retry-After"]
                    } else {
                        &["WWW-Authenticate"]
                    };
                    for name in expected_headers {
                        assert!(
                            !response["headers"][*name].is_null(),
                            "{method} {path} : en-tête {name} manquant sur le {status}"
                        );
                    }
                }
            }
        }
        assert!(seen >= 29, "{seen} opérations /v1 vues");
        // Les réponses partagées existent et le Retry-After a le même schéma que
        // celui du 503 SSE (entier `int32` ≥ 0).
        let shared = &doc["components"]["responses"];
        assert_eq!(
            shared["RateLimited"]["headers"]["Retry-After"]["schema"],
            doc["paths"]["/v1/intensity/stream"]["get"]["responses"]["503"]["headers"]["Retry-After"]
                ["schema"]
        );
        assert!(
            shared["Unauthorized"]["description"]
                .as_str()
                .unwrap()
                .contains("Bearer")
        );
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
            "IntensityIndexLabel",
            "IntensityIndexResponse",
            "IndexBand",
            "IndexThreshold",
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
