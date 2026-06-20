import type {
  CostReferenceResponse,
  CreateWebhookRequest,
  CreatedWebhookResponse,
  ProblemDetails,
  EligibilityFramework,
  Estimator,
  ExchangesHistoryResponse,
  ExchangesResponse,
  FactorsResponse,
  PriceHistoryResponse,
  PriceResponse,
  ForecastResponse,
  GreenestWindowResponse,
  HistoryResponse,
  IntensityResponse,
  Interval,
  Methodology,
  MethodologiesResponse,
  MixResponse,
  Region,
  RenewableResponse,
  RulesetsResponse,
  ScheduleResponse,
  SlotsResponse,
  StatsResponse,
  StreamEvent,
  VisitStatsResponse,
  WeatherHistoryResponse,
  WeatherResponse,
  WebhookListResponse,
} from "./types.js";

/** Instance hébergée par défaut. */
export const DEFAULT_BASE_URL = "https://carbon-fr-api.kovelt.fr";

/** Options de construction du client. */
export interface CarbonFrOptions {
  /** URL de base de l'API (défaut : instance hébergée Kovelt). */
  baseUrl?: string;
  /** Clé API `Bearer` (requise seulement pour les webhooks / quotas du tier hébergé). */
  apiKey?: string;
  /** Implémentation `fetch` à utiliser (défaut : `globalThis.fetch`). */
  fetch?: typeof fetch;
}

/**
 * Erreur API : porte le code HTTP et le corps **Problem Details** (RFC 9457).
 * `code` est l'identifiant court stable (`no_data`, `bad_request`, …) ; le
 * message reprend `detail` (sinon `title`).
 */
export class CarbonFrError extends Error {
  readonly status: number;
  readonly code: string;
  /** Corps Problem Details brut, si disponible. */
  readonly problem: Partial<ProblemDetails>;
  constructor(status: number, body: Partial<ProblemDetails>) {
    super(body.detail ?? body.title ?? `HTTP ${status}`);
    this.name = "CarbonFrError";
    this.status = status;
    this.code = body.code ?? "error";
    this.problem = body;
  }
}

type QueryValue = string | number | boolean | undefined | null;
type Query = Record<string, QueryValue>;

/**
 * Client de l'API carbon-fr (intensité carbone de l'électricité française).
 *
 * ```ts
 * const cf = new CarbonFr();
 * const now = await cf.intensityNow();           // national, rte-direct
 * const mix = await cf.mix({ region: "bretagne" });
 * for await (const e of cf.stream()) console.log(e.intensity);
 * ```
 */
export class CarbonFr {
  private readonly baseUrl: string;
  private readonly apiKey?: string;
  private readonly fetchImpl: typeof fetch;

  constructor(options: CarbonFrOptions = {}) {
    this.baseUrl = (options.baseUrl ?? DEFAULT_BASE_URL).replace(/\/+$/, "");
    this.apiKey = options.apiKey;
    const f = options.fetch ?? globalThis.fetch;
    if (!f) {
      throw new Error(
        "fetch indisponible : passez `fetch` dans les options (Node < 18) ou utilisez un environnement avec fetch natif.",
      );
    }
    this.fetchImpl = f.bind(globalThis);
  }

  // ─── Intensité ──────────────────────────────────────────────────────────

  /** Dernière intensité carbone connue. */
  intensityNow(opts: { region?: Region; methodology?: Methodology; version?: number } = {}) {
    return this.get<IntensityResponse>("/v1/intensity/now", {
      region: opts.region,
      methodology: opts.methodology,
      version: opts.version,
    });
  }

  /** Mix de production de la dernière mesure (MW par filière). */
  mix(opts: { region?: Region; methodology?: Methodology } = {}) {
    return this.get<MixResponse>("/v1/mix", {
      region: opts.region,
      methodology: opts.methodology,
    });
  }

  /** Série historique d'intensité sur un intervalle (RFC 3339, fenêtre ≤ 366 j). */
  intensityDate(opts: {
    from: string;
    to: string;
    region?: Region;
    methodology?: Methodology;
    version?: number;
  }) {
    return this.get<HistoryResponse>("/v1/intensity/date", {
      from: opts.from,
      to: opts.to,
      region: opts.region,
      methodology: opts.methodology,
      version: opts.version,
    });
  }

  /** Résumé statistique (+ série agrégée si `interval`). */
  intensityStats(opts: {
    from: string;
    to: string;
    region?: Region;
    methodology?: Methodology;
    version?: number;
    interval?: Interval;
  }) {
    return this.get<StatsResponse>("/v1/intensity/stats", {
      from: opts.from,
      to: opts.to,
      region: opts.region,
      methodology: opts.methodology,
      version: opts.version,
      interval: opts.interval,
    });
  }

  // ─── Prévision & scheduling ─────────────────────────────────────────────

  /** Intensité prévue sur l'horizon (intervalle d'incertitude par point). */
  forecast(opts: {
    region?: Region;
    methodology?: Methodology;
    version?: number;
    from?: string;
    horizonHours?: number;
  } = {}) {
    // Pas d'`estimator` ici : /v1/intensity/forecast renvoie toujours l'intervalle
    // complet (expected/lower/upper). L'estimateur ne concerne que greenest-window,
    // schedule, scheduleSlots et below.
    return this.get<ForecastResponse>("/v1/intensity/forecast", {
      region: opts.region,
      methodology: opts.methodology,
      version: opts.version,
      from: opts.from,
      horizon_hours: opts.horizonHours,
    });
  }

  /**
   * Créneau le plus bas-carbone à venir. Avec `eligibility`, annote chaque
   * créneau d'un verdict d'éligibilité électrolyseur (ADR-0025/0026) ; l'axe
   * d'éligibilité est orthogonal à `methodology`.
   */
  greenestWindow(opts: {
    region?: Region;
    methodology?: Methodology;
    from?: string;
    horizonHours?: number;
    windowMinutes?: number;
    estimator?: Estimator;
    eligibility?: EligibilityFramework;
    eligibilityVersion?: string;
    surplusPriceEurMwh?: number;
    lowCarbonThresholdGPerKwh?: number;
    electrolyzerKwhPerKg?: number;
  } = {}) {
    return this.get<GreenestWindowResponse>("/v1/intensity/greenest-window", {
      region: opts.region,
      methodology: opts.methodology,
      from: opts.from,
      horizon_hours: opts.horizonHours,
      window_minutes: opts.windowMinutes,
      estimator: opts.estimator,
      eligibility: opts.eligibility,
      eligibility_version: opts.eligibilityVersion,
      surplus_price_eur_mwh: opts.surplusPriceEurMwh,
      low_carbon_threshold_g_per_kwh: opts.lowCarbonThresholdGPerKwh,
      electrolyzer_kwh_per_kg: opts.electrolyzerKwhPerKg,
    });
  }

  /** Catalogue des cadres et rulesets d'éligibilité électrolyseur (versionnés). */
  eligibilityRulesets() {
    return this.get<RulesetsResponse>("/v1/eligibility/rulesets", {});
  }

  /** Planifie un job sous échéance et chiffre l'économie carbone vs maintenant. */
  schedule(opts: {
    region?: Region;
    methodology?: Methodology;
    from?: string;
    horizonHours?: number;
    durationMinutes?: number;
    deadline?: string;
    energyKwh?: number;
    estimator?: Estimator;
  } = {}) {
    return this.get<ScheduleResponse>("/v1/schedule", {
      region: opts.region,
      methodology: opts.methodology,
      from: opts.from,
      horizon_hours: opts.horizonHours,
      duration_minutes: opts.durationMinutes,
      deadline: opts.deadline,
      energy_kwh: opts.energyKwh,
      estimator: opts.estimator,
    });
  }

  /** Les `count` créneaux les moins intenses de l'horizon. */
  scheduleSlots(opts: {
    count: number;
    region?: Region;
    methodology?: Methodology;
    from?: string;
    horizonHours?: number;
    estimator?: Estimator;
  }) {
    return this.get<SlotsResponse>("/v1/schedule/slots", {
      count: opts.count,
      region: opts.region,
      methodology: opts.methodology,
      from: opts.from,
      horizon_hours: opts.horizonHours,
      estimator: opts.estimator,
    });
  }

  /** Créneaux dont l'intensité prévue passe sous un seuil. */
  below(opts: {
    threshold: number;
    region?: Region;
    methodology?: Methodology;
    from?: string;
    horizonHours?: number;
    estimator?: Estimator;
  }) {
    return this.get<SlotsResponse>("/v1/intensity/below", {
      threshold: opts.threshold,
      region: opts.region,
      methodology: opts.methodology,
      from: opts.from,
      horizon_hours: opts.horizonHours,
      estimator: opts.estimator,
    });
  }

  // ─── Échanges, météo, renouvelable ──────────────────────────────────────

  /** Échanges transfrontaliers courants (flux par frontière + carbone des voisins). */
  exchanges() {
    return this.get<ExchangesResponse>("/v1/exchanges");
  }

  /** Série historique des échanges transfrontaliers. */
  exchangesHistory(opts: { from: string; to: string }) {
    return this.get<ExchangesHistoryResponse>("/v1/exchanges/date", {
      from: opts.from,
      to: opts.to,
    });
  }

  /** Météo nationale courante (vent à 100 m, irradiance ; Open-Meteo CC-BY 4.0). */
  weather() {
    return this.get<WeatherResponse>("/v1/weather");
  }

  /** Série météo historique (depuis 2016). */
  weatherHistory(opts: { from: string; to: string }) {
    return this.get<WeatherHistoryResponse>("/v1/weather/date", {
      from: opts.from,
      to: opts.to,
    });
  }

  /** Production renouvelable estimée depuis la météo + facteur de charge (modélisée). */
  renewable() {
    return this.get<RenewableResponse>("/v1/renewable");
  }

  // ─── Méthodologie & exploitation ────────────────────────────────────────

  /** Catalogue des méthodes de calcul + versions. */
  methodologies() {
    return this.get<MethodologiesResponse>("/v1/methodologies");
  }

  /** Table des facteurs d'émission d'une méthode (vérifiabilité). */
  factors(opts: { methodology?: Methodology; version?: number } = {}) {
    return this.get<FactorsResponse>("/v1/factors", {
      methodology: opts.methodology,
      version: opts.version,
    });
  }

  // ─── Prix (ADR-0023/0024) ───────────────────────────────────────────────

  /** Décomposition du prix payé (TRV : énergie spot + TURPE + accise + TVA + résidu). */
  price() {
    return this.get<PriceResponse>("/v1/price");
  }

  /** Série de décompositions de prix (primitive « cheapest + greenest window »). */
  priceHistory(opts: { from: string; to: string }) {
    return this.get<PriceHistoryResponse>("/v1/price/date", {
      from: opts.from,
      to: opts.to,
    });
  }

  /**
   * Couche comparative LCOE (coût de production) — **estimation** versionnée en
   * fourchette, jamais soustraite du prix de marché (ADR-0024).
   */
  costReference(
    opts: {
      source?: string;
      technology?: string;
      perimeter?: string;
      vintage?: number;
    } = {},
  ) {
    return this.get<CostReferenceResponse>("/v1/cost-reference", {
      source: opts.source,
      technology: opts.technology,
      perimeter: opts.perimeter,
      vintage: opts.vintage,
    });
  }

  /** Statistiques de consultation (RGPD : aucune IP stockée). */
  visitStats() {
    return this.get<VisitStatsResponse>("/v1/stats");
  }

  /** Enregistre une visite (compteur dédupliqué par IP/jour côté serveur). */
  recordVisit() {
    return this.request<VisitStatsResponse>("POST", "/v1/stats/visit");
  }

  // ─── Webhooks (clé API requise) ─────────────────────────────────────────

  /** Crée un abonnement webhook signé. Le `secret` n'est renvoyé qu'ici. */
  createWebhook(req: CreateWebhookRequest) {
    return this.request<CreatedWebhookResponse>("POST", "/v1/webhooks", undefined, req);
  }

  /** Liste les abonnements de la clé. */
  listWebhooks() {
    return this.get<WebhookListResponse>("/v1/webhooks");
  }

  /** Supprime un abonnement. */
  async deleteWebhook(id: string): Promise<void> {
    await this.request<unknown>("DELETE", `/v1/webhooks/${encodeURIComponent(id)}`);
  }

  // ─── Flux live (SSE) ────────────────────────────────────────────────────

  /**
   * Flux temps réel des mises à jour d'intensité (Server-Sent Events).
   *
   * ```ts
   * const ac = new AbortController();
   * for await (const e of cf.stream({ below: 50, signal: ac.signal })) { ... }
   * ```
   */
  async *stream(
    opts: { region?: Region; below?: number; signal?: AbortSignal } = {},
  ): AsyncGenerator<StreamEvent, void, unknown> {
    const res = await this.fetchImpl(
      this.buildUrl("/v1/intensity/stream", { region: opts.region, below: opts.below }),
      { headers: { Accept: "text/event-stream", ...this.authHeaders() }, signal: opts.signal },
    );
    if (!res.ok || !res.body) throw await this.toError(res);

    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let sep: number;
      // Une trame SSE est terminée par une ligne vide (\n\n).
      while ((sep = buffer.indexOf("\n\n")) >= 0) {
        const frame = buffer.slice(0, sep);
        buffer = buffer.slice(sep + 2);
        const data = frame
          .split("\n")
          .filter((l) => l.startsWith("data:"))
          .map((l) => l.slice(5).trimStart())
          .join("\n");
        if (data) {
          try {
            yield JSON.parse(data) as StreamEvent;
          } catch {
            // Commentaire/keep-alive non-JSON : ignoré.
          }
        }
      }
    }
  }

  // ─── Internes ───────────────────────────────────────────────────────────

  private get<T>(path: string, query?: Query): Promise<T> {
    return this.request<T>("GET", path, query);
  }

  private async request<T>(
    method: string,
    path: string,
    query?: Query,
    body?: unknown,
  ): Promise<T> {
    const headers: Record<string, string> = { Accept: "application/json", ...this.authHeaders() };
    if (body !== undefined) headers["Content-Type"] = "application/json";
    const res = await this.fetchImpl(this.buildUrl(path, query), {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    if (!res.ok) throw await this.toError(res);
    if (res.status === 204) return undefined as T;
    return (await res.json()) as T;
  }

  private buildUrl(path: string, query?: Query): string {
    const url = new URL(this.baseUrl + path);
    if (query) {
      for (const [k, v] of Object.entries(query)) {
        if (v !== undefined && v !== null) url.searchParams.set(k, String(v));
      }
    }
    return url.toString();
  }

  private authHeaders(): Record<string, string> {
    return this.apiKey ? { Authorization: `Bearer ${this.apiKey}` } : {};
  }

  private async toError(res: Response): Promise<CarbonFrError> {
    let body: Partial<ProblemDetails> = {};
    try {
      body = (await res.json()) as Partial<ProblemDetails>;
    } catch {
      // Corps non-JSON.
    }
    return new CarbonFrError(res.status, body);
  }
}
