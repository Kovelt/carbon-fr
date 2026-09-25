//! Les 25 méthodes REST du contrat `/v1` restant après le socle + le flux SSE
//! (ADR-0031 décision 8 ; `intensity_stream` vit dans `crate::stream`). Un
//! seul `impl CarbonFr` : chaque méthode résout l'URL (`CarbonFr::url`),
//! ajoute ses paramètres de requête (`crate::query::QueryPairsExt`), envoie
//! via `CarbonFr::request_json`/`send_checked` (socle testé de
//! `crate::client`) et retourne un DTO de `crate::dto`.
//!
//! Table de parité `operationId` → méthode : voir
//! `crates/sdk/tests/parity.rs` (ADR-0031 décision 2).

use reqwest::Method;
use time::OffsetDateTime;

use crate::client::CarbonFr;
use crate::dto::{
    CostReferenceResponse, CreateWebhookRequest, CreatedWebhookResponse, ExchangesHistoryResponse,
    ExchangesResponse, FactorsResponse, ForecastResponse, GreenestWindowResponse, HistoryResponse,
    IntensityResponse, MethodologiesResponse, MixResponse, PriceHistoryResponse, PriceResponse,
    RenewableResponse, RulesetsResponse, ScheduleResponse, SlotsResponse, StatsResponse,
    VisitStatsResponse, WeatherHistoryResponse, WeatherResponse, WebhookListResponse,
};
use crate::error::CarbonFrError;
use crate::options::{
    BelowOptions, CostReferenceOptions, FactorsOptions, ForecastOptions, GreenestWindowOptions,
    IntensityDateOptions, IntensityNowOptions, IntensityStatsOptions, MixOptions,
    PriceHistoryOptions, PriceOptions, ScheduleOptions, ScheduleSlotsOptions,
};
use crate::query::QueryPairsExt;
use crate::region::Region;

impl CarbonFr {
    // --- Intensité (`/v1/intensity/*`) -------------------------------------

    /// `GET /v1/intensity/now` — dernière intensité carbone connue.
    pub async fn intensity_now(
        &self,
        options: IntensityNowOptions,
    ) -> Result<IntensityResponse, CarbonFrError> {
        let mut url = self.url("v1/intensity/now")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_str("region", options.region.as_ref().map(Region::as_str))
                .opt_str("methodology", options.methodology.map(|m| m.as_str()))
                .opt_display("version", options.version);
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    /// `GET /v1/intensity/date?from=&to=` — série historique sur
    /// `[from, to)` (RFC 3339, national par défaut).
    pub async fn intensity_date(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
        options: IntensityDateOptions,
    ) -> Result<HistoryResponse, CarbonFrError> {
        let mut url = self.url("v1/intensity/date")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_rfc3339("from", Some(from))?
                .opt_rfc3339("to", Some(to))?
                .opt_str("region", options.region.as_ref().map(Region::as_str))
                .opt_str("methodology", options.methodology.map(|m| m.as_str()))
                .opt_display("version", options.version);
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    /// `GET /v1/intensity/stats?from=&to=` — résumé (moyenne/min/max) sur
    /// `[from, to)`, et série agrégée par pas si `options.interval` est
    /// fourni.
    pub async fn intensity_stats(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
        options: IntensityStatsOptions,
    ) -> Result<StatsResponse, CarbonFrError> {
        let mut url = self.url("v1/intensity/stats")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_rfc3339("from", Some(from))?
                .opt_rfc3339("to", Some(to))?
                .opt_str("region", options.region.as_ref().map(Region::as_str))
                .opt_str("interval", options.interval.map(|i| i.as_str()))
                .opt_str("methodology", options.methodology.map(|m| m.as_str()))
                .opt_display("version", options.version);
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    /// `GET /v1/intensity/below` — tous les créneaux prévus d'intensité sous
    /// `threshold` (gCO₂eq/kWh) sur l'horizon (ADR-0014). `threshold` est un
    /// argument direct (requis en pratique malgré `required: false` dans
    /// l'OpenAPI — cf. `crate::options`).
    pub async fn below(
        &self,
        threshold: f64,
        options: BelowOptions,
    ) -> Result<SlotsResponse, CarbonFrError> {
        let mut url = self.url("v1/intensity/below")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_str("region", options.region.as_ref().map(Region::as_str))
                .opt_str("methodology", options.methodology.map(|m| m.as_str()))
                .opt_display("version", options.version)
                .opt_display("horizon_hours", options.horizon_hours)
                .opt_display("threshold", Some(threshold))
                .opt_str("estimator", options.estimator.map(|e| e.as_str()));
            q.opt_rfc3339("from", options.from)?;
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    // --- Prévision (`/v1/intensity/forecast`, `/v1/intensity/greenest-window`) -

    /// `GET /v1/intensity/forecast` — série d'intensité **prévue** sur
    /// l'horizon (modèle `climatology@1`, ADR-0009).
    pub async fn forecast(
        &self,
        options: ForecastOptions,
    ) -> Result<ForecastResponse, CarbonFrError> {
        let mut url = self.url("v1/intensity/forecast")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_str("region", options.region.as_ref().map(Region::as_str))
                .opt_str("methodology", options.methodology.map(|m| m.as_str()))
                .opt_display("horizon_hours", options.horizon_hours)
                .opt_display("version", options.version);
            q.opt_rfc3339("from", options.from)?;
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    /// `GET /v1/intensity/greenest-window` — créneau le plus bas-carbone à
    /// venir (`climatology@1`, ADR-0009). Avec `options.eligibility`, annote
    /// chaque créneau d'un verdict d'éligibilité électrolyseur
    /// (ADR-0025/0026).
    pub async fn greenest_window(
        &self,
        options: GreenestWindowOptions,
    ) -> Result<GreenestWindowResponse, CarbonFrError> {
        let mut url = self.url("v1/intensity/greenest-window")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_str("region", options.region.as_ref().map(Region::as_str))
                .opt_str("methodology", options.methodology.map(|m| m.as_str()))
                .opt_display("version", options.version)
                .opt_display("horizon_hours", options.horizon_hours)
                .opt_display("window_minutes", options.window_minutes)
                .opt_str("estimator", options.estimator.map(|e| e.as_str()))
                .opt_str("eligibility", options.eligibility.map(|f| f.as_str()))
                .opt_str(
                    "eligibility_version",
                    options.eligibility_version.as_deref(),
                )
                .opt_display("surplus_price_eur_mwh", options.surplus_price_eur_mwh)
                .opt_display(
                    "low_carbon_threshold_g_per_kwh",
                    options.low_carbon_threshold_g_per_kwh,
                )
                .opt_display("electrolyzer_kwh_per_kg", options.electrolyzer_kwh_per_kg);
            q.opt_rfc3339("from", options.from)?;
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    // --- Scheduling (`/v1/schedule`, `/v1/schedule/slots`) -----------------

    /// `GET /v1/schedule` — créneau contigu le plus bas-carbone (avant une
    /// échéance optionnelle) + économie carbone vs « maintenant »
    /// (ADR-0014).
    pub async fn schedule(
        &self,
        options: ScheduleOptions,
    ) -> Result<ScheduleResponse, CarbonFrError> {
        let mut url = self.url("v1/schedule")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_str("region", options.region.as_ref().map(Region::as_str))
                .opt_str("methodology", options.methodology.map(|m| m.as_str()))
                .opt_display("version", options.version)
                .opt_display("horizon_hours", options.horizon_hours)
                .opt_display("duration_minutes", options.duration_minutes)
                .opt_display("energy_kwh", options.energy_kwh)
                .opt_str("estimator", options.estimator.map(|e| e.as_str()));
            q.opt_rfc3339("from", options.from)?
                .opt_rfc3339("deadline", options.deadline)?;
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    /// `GET /v1/schedule/slots` — les `count` créneaux les moins intenses sur
    /// l'horizon (job divisible, ADR-0014). `count` est un argument direct
    /// (requis en pratique malgré `required: false` dans l'OpenAPI — cf.
    /// `crate::options`) ; si `count` dépasse le nombre de créneaux
    /// disponibles, la réponse est plafonnée silencieusement
    /// (`SlotsResponse::count` reflète alors le nombre réellement renvoyé).
    pub async fn schedule_slots(
        &self,
        count: u32,
        options: ScheduleSlotsOptions,
    ) -> Result<SlotsResponse, CarbonFrError> {
        let mut url = self.url("v1/schedule/slots")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_str("region", options.region.as_ref().map(Region::as_str))
                .opt_str("methodology", options.methodology.map(|m| m.as_str()))
                .opt_display("version", options.version)
                .opt_display("horizon_hours", options.horizon_hours)
                .opt_display("count", Some(count))
                .opt_str("estimator", options.estimator.map(|e| e.as_str()));
            q.opt_rfc3339("from", options.from)?;
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    // --- Mix de production (`/v1/mix`) --------------------------------------

    /// `GET /v1/mix` — mix de production de la dernière mesure.
    pub async fn mix(&self, options: MixOptions) -> Result<MixResponse, CarbonFrError> {
        let mut url = self.url("v1/mix")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_str("region", options.region.as_ref().map(Region::as_str))
                .opt_str("methodology", options.methodology.map(|m| m.as_str()))
                .opt_display("version", options.version);
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    // --- Échanges transfrontaliers (`/v1/exchanges*`) -----------------------

    /// `GET /v1/exchanges` — échanges transfrontaliers : flux net signé par
    /// frontière (`> 0` = import vers la France) + intensité carbone du
    /// voisin (ADR-0017).
    pub async fn exchanges(&self) -> Result<ExchangesResponse, CarbonFrError> {
        self.request_json(self.request(Method::GET, "v1/exchanges")?)
            .await
    }

    /// `GET /v1/exchanges/date?from=&to=` — série historique des échanges
    /// transfrontaliers sur `[from, to)` (fenêtre ≤ 92 jours, ADR-0017).
    pub async fn exchanges_history(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
    ) -> Result<ExchangesHistoryResponse, CarbonFrError> {
        let mut url = self.url("v1/exchanges/date")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_rfc3339("from", Some(from))?
                .opt_rfc3339("to", Some(to))?;
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    // --- Météo (`/v1/weather*`) ----------------------------------------------

    /// `GET /v1/weather` — météo nationale courante (vent à 100 m,
    /// irradiance ; moyenne 7 points), ADR-0012/0018.
    pub async fn weather(&self) -> Result<WeatherResponse, CarbonFrError> {
        self.request_json(self.request(Method::GET, "v1/weather")?)
            .await
    }

    /// `GET /v1/weather/date?from=&to=` — série météo historique (vent +
    /// irradiance) sur `[from, to)` (fenêtre ≤ 92 jours, ADR-0012/0018).
    pub async fn weather_history(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
    ) -> Result<WeatherHistoryResponse, CarbonFrError> {
        let mut url = self.url("v1/weather/date")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_rfc3339("from", Some(from))?
                .opt_rfc3339("to", Some(to))?;
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    // --- Renouvelable estimé (`/v1/renewable`) ------------------------------

    /// `GET /v1/renewable` — production renouvelable **estimée** depuis la
    /// météo courante (modèle calibré, ADR-0018) + facteur de charge.
    pub async fn renewable(&self) -> Result<RenewableResponse, CarbonFrError> {
        self.request_json(self.request(Method::GET, "v1/renewable")?)
            .await
    }

    // --- Méthodologies et facteurs (`/v1/methodologies`, `/v1/factors`) ----

    /// `GET /v1/methodologies` — catalogue des méthodes carbone + versions
    /// (ADR-0010 §7).
    pub async fn methodologies(&self) -> Result<MethodologiesResponse, CarbonFrError> {
        self.request_json(self.request(Method::GET, "v1/methodologies")?)
            .await
    }

    /// `GET /v1/factors` — table des facteurs d'émission d'une méthode
    /// (ADR-0010 §7).
    pub async fn factors(&self, options: FactorsOptions) -> Result<FactorsResponse, CarbonFrError> {
        let mut url = self.url("v1/factors")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_str("methodology", options.methodology.map(|m| m.as_str()))
                .opt_display("version", options.version);
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    // --- Prix (`/v1/price*`, `/v1/cost-reference`) --------------------------

    /// `GET /v1/price` — décomposition complète du prix payé (ADR-0023).
    /// `options.region`, si fourni, doit être national (le TRV est
    /// national) — toute autre valeur renvoie une [`CarbonFrError::Api`]
    /// 400.
    pub async fn price(&self, options: PriceOptions) -> Result<PriceResponse, CarbonFrError> {
        let mut url = self.url("v1/price")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_str("region", options.region.as_ref().map(Region::as_str));
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    /// `GET /v1/price/date?from=&to=` — série de décompositions de prix sur
    /// `[from, to)` (fenêtre ≤ 92 jours, ADR-0023). Même contrainte
    /// `region` national que [`CarbonFr::price`].
    pub async fn price_history(
        &self,
        from: OffsetDateTime,
        to: OffsetDateTime,
        options: PriceHistoryOptions,
    ) -> Result<PriceHistoryResponse, CarbonFrError> {
        let mut url = self.url("v1/price/date")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_rfc3339("from", Some(from))?
                .opt_rfc3339("to", Some(to))?
                .opt_str("region", options.region.as_ref().map(Region::as_str));
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    /// `GET /v1/cost-reference` — couche comparative LCOE (ADR-0024).
    pub async fn cost_reference(
        &self,
        options: CostReferenceOptions,
    ) -> Result<CostReferenceResponse, CarbonFrError> {
        let mut url = self.url("v1/cost-reference")?;
        {
            let mut q = url.query_pairs_mut();
            q.opt_str("source", options.source.as_deref())
                .opt_str("technology", options.technology.as_deref())
                .opt_str("perimeter", options.perimeter.as_deref())
                .opt_display("vintage", options.vintage);
        }
        self.request_json(self.request_for_url(Method::GET, url))
            .await
    }

    // --- Éligibilité (`/v1/eligibility/rulesets`) ---------------------------

    /// `GET /v1/eligibility/rulesets` — catalogue des cadres et rulesets
    /// versionnés d'éligibilité électrolyseur (ADR-0025/0026).
    pub async fn eligibility_rulesets(&self) -> Result<RulesetsResponse, CarbonFrError> {
        self.request_json(self.request(Method::GET, "v1/eligibility/rulesets")?)
            .await
    }

    // --- Statistiques de visite (`/v1/stats`, `POST /v1/stats/visit`) ------

    /// `GET /v1/stats` — statistiques de consultation.
    pub async fn visit_stats(&self) -> Result<VisitStatsResponse, CarbonFrError> {
        self.request_json(self.request(Method::GET, "v1/stats")?)
            .await
    }

    /// `POST /v1/stats/visit` — enregistre une visite (unique par IP/jour, IP
    /// hachée jamais stockée) et renvoie les statistiques à jour. Aucun
    /// corps de requête.
    pub async fn record_visit(&self) -> Result<VisitStatsResponse, CarbonFrError> {
        self.request_json(self.request(Method::POST, "v1/stats/visit")?)
            .await
    }

    // --- Webhooks (`/v1/webhooks*`, auth `Bearer` requise) ------------------

    /// Une clé API est requise pour les 3 opérations webhooks
    /// (ADR-0015/0016) : plutôt que de laisser partir une requête vouée à un
    /// 401 `unauthorized` (RFC 9457) — configuration client incomplète
    /// détectable **avant** tout aller-retour réseau —, cette vérification
    /// échoue localement avec [`CarbonFrError::Config`] (choix documenté,
    /// `sdk-spec.json` demandait un choix explicite ici).
    fn require_api_key(&self) -> Result<(), CarbonFrError> {
        if self.api_key.is_none() {
            return Err(CarbonFrError::Config(
                "une clé API (CarbonFrBuilder::api_key) est requise pour les webhooks".to_string(),
            ));
        }
        Ok(())
    }

    /// `GET /v1/webhooks` — liste les abonnements de la clé. **Auth
    /// requise.**
    pub async fn list_webhooks(&self) -> Result<WebhookListResponse, CarbonFrError> {
        self.require_api_key()?;
        self.request_json(self.request(Method::GET, "v1/webhooks")?)
            .await
    }

    /// `POST /v1/webhooks` — crée un abonnement webhook (ADR-0016). **Auth
    /// requise.** Le `secret` de signature n'est renvoyé **qu'une seule
    /// fois**, dans la réponse de cet appel.
    pub async fn create_webhook(
        &self,
        request: CreateWebhookRequest,
    ) -> Result<CreatedWebhookResponse, CarbonFrError> {
        self.require_api_key()?;
        let builder = self.request(Method::POST, "v1/webhooks")?.json(&request);
        self.request_json(builder).await
    }

    /// `DELETE /v1/webhooks/{id}` — supprime un abonnement possédé par la
    /// clé. **Auth requise.** `204 No Content` en cas de succès.
    pub async fn delete_webhook(&self, id: &str) -> Result<(), CarbonFrError> {
        self.require_api_key()?;
        let mut url = self.url("v1/webhooks/")?;
        url.path_segments_mut()
            .map_err(|()| CarbonFrError::Config("URL de base non segmentable".to_string()))?
            // `v1/webhooks/` laisse un segment final vide (trailing slash) :
            // sans `pop_if_empty()`, `push(id)` produirait
            // `.../webhooks//<id>` (double slash, 404 côté serveur — piège
            // rencontré et corrigé pendant l'écriture des tests).
            .pop_if_empty()
            .push(id);
        self.send_checked(self.request_for_url(Method::DELETE, url))
            .await?;
        Ok(())
    }
}
