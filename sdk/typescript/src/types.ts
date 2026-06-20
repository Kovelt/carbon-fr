// Types de l'API carbon-fr (/v1). Reflètent les DTO du serveur ; l'unité carbone
// canonique est gCO₂eq/kWh, les puissances en MW, les horodatages en RFC 3339 UTC.

/** Slug de région. National + 12 régions métropolitaines (codes INSEE aussi acceptés). */
export type Region =
  | "national"
  | "auvergne-rhone-alpes"
  | "bourgogne-franche-comte"
  | "bretagne"
  | "centre-val-de-loire"
  | "grand-est"
  | "hauts-de-france"
  | "ile-de-france"
  | "normandie"
  | "nouvelle-aquitaine"
  | "occitanie"
  | "pays-de-la-loire"
  | "provence-alpes-cote-d-azur"
  // Tolère d'autres valeurs (codes INSEE, futures régions) sans perdre l'autocomplétion.
  | (string & {});

/** Méthodologie de calcul carbone. */
export type Methodology = "rte-direct" | "acv-ademe";

/** Estimateur de prévision : central (attendu) ou prudent (borne haute). */
export type Estimator = "central" | "prudent";

/** Cadre d'éligibilité électrolyseur (ADR-0025/0026). */
export type EligibilityFramework = "rfnbo" | "low-carbon";

/** Pas d'agrégation des statistiques. */
export type Interval = "hour" | "day";

/** Sens d'un flux d'échange (du point de vue France). */
export type FlowDirection = "import" | "export" | "balanced";

/** Sens de franchissement d'un seuil de webhook. */
export type ThresholdDirection = "below" | "above";

export interface IntensityValue {
  value: number;
  unit: string;
}

/** `GET /v1/intensity/now`. */
export interface IntensityResponse {
  region: string;
  timestamp: string;
  intensity: IntensityValue;
  methodology: string;
  methodology_version: number;
  vintage: string;
}

export interface GenerationMix {
  nucleaire: number;
  gaz: number;
  charbon: number;
  fioul: number;
  hydraulique: number;
  eolien: number;
  solaire: number;
  bioenergies: number;
  pompage: number;
  echanges: number;
  /** Thermique fossile agrégé (mix régional uniquement ; absent au national). */
  thermique?: number;
}

/** `GET /v1/mix`. */
export interface MixResponse {
  region: string;
  timestamp: string;
  unit: string;
  mix: GenerationMix;
}

export interface HistoryPoint {
  timestamp: string;
  intensity: number;
  vintage: string;
}

/** `GET /v1/intensity/date`. */
export interface HistoryResponse {
  region: string;
  from: string;
  to: string;
  unit: string;
  methodology: string;
  count: number;
  data: HistoryPoint[];
}

export interface StatsBucket {
  start: string;
  average: number;
  min: number;
  max: number;
  count: number;
}

/** `GET /v1/intensity/stats`. */
export interface StatsResponse {
  region: string;
  from: string;
  to: string;
  unit: string;
  methodology: string;
  average: number;
  min: number;
  max: number;
  count: number;
  interval?: Interval;
  intervals?: StatsBucket[];
}

export interface ForecastPoint {
  timestamp: string;
  /** Estimation centrale. */
  expected: number;
  /** Borne basse de l'intervalle d'incertitude. */
  lower: number;
  /** Borne haute. */
  upper: number;
}

/** `GET /v1/intensity/forecast`. */
export interface ForecastResponse {
  region: string;
  methodology: string;
  /** Identité versionnée du modèle (ex. `climatology@1`). */
  model: string;
  from: string;
  horizon_hours: number;
  unit: string;
  count: number;
  data: ForecastPoint[];
}

/** `GET /v1/intensity/greenest-window`. */
export interface GreenestWindowResponse {
  region: string;
  methodology: string;
  model: string;
  start: string;
  end: string;
  unit: string;
  average_intensity: number;
  /** Overlay d'éligibilité électrolyseur (présent si `?eligibility=` fourni). */
  eligibility?: EligibilityBody;
}

/** Verdict d'un pilier d'éligibilité sur un créneau. */
export interface EligibilitySignal {
  /** `renewable-share`, `surplus-price` ou `low-carbon-intensity`. */
  pillar: string;
  /** `pass`, `fail` ou `indeterminate` (donnée manquante, jamais extrapolée). */
  verdict: "pass" | "fail" | "indeterminate";
  value?: number;
  threshold?: number;
  /** `regulatory` ou `indicative-non-regulatory` (seuil bas-carbone = proxy). */
  basis: string;
}

/** Verdict d'éligibilité d'un créneau. */
export interface EligibilitySlot {
  timestamp: string;
  eligible: boolean;
  intensity: number;
  /** Bornes de l'intervalle de confiance (ADR-0011). */
  intensity_lower: number;
  intensity_upper: number;
  score: number;
  signals: EligibilitySignal[];
}

/** Meilleur créneau éligible (score le plus bas). */
export interface EligibleSlot {
  timestamp: string;
  intensity: number;
  intensity_lower: number;
  intensity_upper: number;
  score: number;
}

/** Overlay d'éligibilité d'une fenêtre (ADR-0025/0026). */
export interface EligibilityBody {
  framework: EligibilityFramework;
  ruleset_version: string;
  ruleset_status: string;
  overridden: boolean;
  /** Zone de dépôt : toujours `FR` (jamais une sous-région). */
  bidding_zone: string;
  disclaimer: string;
  /** `true` si tous les créneaux de la fenêtre verte retenue sont éligibles. */
  window_eligible: boolean;
  best_eligible?: EligibleSlot;
  count_eligible: number;
  count_indeterminate: number;
  slots: EligibilitySlot[];
}

/** Une entrée du catalogue `GET /v1/eligibility/rulesets`. */
export interface RulesetInfo {
  framework: EligibilityFramework;
  version: string;
  status: string;
  adr: string;
  granularity: string;
  hourly_switchover?: string;
  article4_renewable_threshold: number;
  surplus_price_eur_mwh?: number;
  low_carbon_intensity_threshold_g_per_kwh?: number;
  low_carbon_intensity_is_indicative: boolean;
  electrolyzer_kwh_per_kg: number;
  legal_basis: string;
  description: string;
}

/** `GET /v1/eligibility/rulesets`. */
export interface RulesetsResponse {
  rulesets: RulesetInfo[];
  disclaimer: string;
}

export interface Savings {
  now: number;
  scheduled: number;
  intensity_delta: number;
  reduction_percent: number;
  absolute_saved_g?: number;
}

/** `GET /v1/schedule`. */
export interface ScheduleResponse {
  region: string;
  methodology: string;
  model: string;
  estimator: Estimator;
  unit: string;
  start: string;
  end: string;
  average_intensity: number;
  savings: Savings;
}

export interface Slot {
  timestamp: string;
  intensity: number;
}

/** `GET /v1/schedule/slots` et `GET /v1/intensity/below`. */
export interface SlotsResponse {
  region: string;
  methodology: string;
  model: string;
  estimator: Estimator;
  unit: string;
  count: number;
  slots: Slot[];
}

export interface ExchangeEntry {
  /** Code voisin (`be`, `de-lu`, `es`, `it-north`, `ch`, `gb`). */
  country: string;
  country_name: string;
  /** Flux net (MW) : > 0 = la France importe de ce pays, < 0 = exporte. */
  flow_mw: number;
  direction: FlowDirection;
  /** Intensité carbone (cycle de vie) du voisin. */
  intensity: IntensityValue;
}

/** `GET /v1/exchanges`. */
export interface ExchangesResponse {
  timestamp: string;
  /** Solde net FR (MW) : > 0 = import, < 0 = export. */
  net_flow_mw: number;
  direction: FlowDirection;
  imports_mw: number;
  exports_mw: number;
  exchanges: ExchangeEntry[];
}

/** `GET /v1/exchanges/date`. */
export interface ExchangesHistoryResponse {
  from: string;
  to: string;
  count: number;
  snapshots: ExchangesResponse[];
}

/** `GET /v1/weather`. */
export interface WeatherResponse {
  /** Attribution de la source (Open-Meteo, CC-BY 4.0). */
  source: string;
  valid_at: string;
  run_at: string;
  wind_kmh: number;
  irradiance_wm2: number;
}

export interface WeatherPoint {
  valid_at: string;
  run_at: string;
  wind_kmh: number;
  irradiance_wm2: number;
}

/** `GET /v1/weather/date`. */
export interface WeatherHistoryResponse {
  source: string;
  from: string;
  to: string;
  count: number;
  points: WeatherPoint[];
}

/** `GET /v1/renewable` — production renouvelable estimée (modélisée). */
export interface RenewableResponse {
  source: string;
  at: string;
  wind_mw: number;
  solar_mw: number;
  /** Facteur de charge éolien (0–1). */
  wind_capacity_factor: number;
  /** Facteur de charge solaire (0–1). */
  solar_capacity_factor: number;
  model: {
    wind_capacity_mw: number;
    solar_capacity_mw: number;
  };
}

export interface MethodologyInfo {
  id: string;
  version: number;
  basis: string;
  scope: string;
  default: boolean;
  status: string;
  adr: string;
  description: string;
}

/** `GET /v1/methodologies`. */
export interface MethodologiesResponse {
  methodologies: MethodologyInfo[];
}

export interface FactorEntry {
  filiere: string;
  factor: number;
}

/** `GET /v1/factors`. */
export interface FactorsResponse {
  methodology: string;
  methodology_version: number;
  unit: string;
  source: string;
  factors: FactorEntry[];
  /** Facteur de pertes T&D (uplift consommation), `null` hors méthode consommation. */
  td_loss_factor: number | null;
}

/** Une composante du prix (`GET /v1/price`). */
export interface PriceComponent {
  /** `energie` | `acheminement` | `accise` | `commercialisation` | `tva`. */
  kind: string;
  label: string;
  amount_eur_mwh: number;
  source: string;
}

export interface PriceMixShare {
  filiere: string;
  label: string;
  /** Part dans la production domestique, dans `[0, 1]`. */
  share: number;
  output_mw: number;
}

export interface PriceMarginalTechnology {
  filiere: string;
  label: string;
  /** Toujours `true` : estimée par ordre de mérite, jamais mesurée. */
  estimated: boolean;
  method: string;
}

export interface PriceContext {
  mix: PriceMixShare[];
  marginal_technology: PriceMarginalTechnology | null;
}

/** `GET /v1/price` — décomposition complète du prix payé (TRV, ADR-0023). */
export interface PriceResponse {
  region: string;
  timestamp: string;
  vintage: string;
  unit: string;
  currency: string;
  total_eur_mwh: number;
  total_eur_kwh: number;
  components: PriceComponent[];
  context: PriceContext;
  disclaimer: string;
}

/** Un point d'une série de prix (`GET /v1/price/date`). */
export interface PricePoint {
  timestamp: string;
  energie_eur_mwh: number;
  total_eur_mwh: number;
}

/** `GET /v1/price/date`. */
export interface PriceHistoryResponse {
  from: string;
  to: string;
  count: number;
  unit: string;
  currency: string;
  points: PricePoint[];
}

/** Fourchette LCOE (estimation, `GET /v1/cost-reference`). */
export interface LcoeRange {
  min: number;
  median: number;
  max: number;
  unit: string;
}

export interface CostAssumptions {
  discount_rate: number | null;
  lifetime_years: number | null;
  load_factor: number | null;
}

export interface CostReferenceEntry {
  technology: string;
  technology_label: string;
  source: string;
  source_label: string;
  source_attribution: string;
  /** Périmètre géographique : `"france"` ou `"monde"` (IRENA = mondial, souvent plus bas). */
  geography: string;
  perimeter: string;
  /** Libellé explicitant ce que le périmètre inclut/exclut. */
  perimeter_label: string;
  /** `"accounting-amortized"` (coût comptable amorti) vs `"prospective-lcoe"`. */
  basis: string;
  basis_label: string;
  /** Nombre de sources distinctes pour cette filière (≥ 2 = dispersion inter-sources ; 1 = mono-source). */
  technology_source_count: number;
  vintage: number;
  /** Statut systématique : `"estimation"` (ADR-0024). */
  kind: string;
  range: LcoeRange;
  hypotheses: CostAssumptions;
}

/** `GET /v1/cost-reference` — couche comparative LCOE (estimation, ADR-0024). */
export interface CostReferenceResponse {
  unit: string;
  currency: string;
  /** Statut systématique : `"estimation"`. */
  kind: string;
  /** Note neutre obligatoire (LCOE ≠ coût marginal ≠ prix payé). */
  disclaimer: string;
  count: number;
  entries: CostReferenceEntry[];
}

/** `GET /v1/stats` et `POST /v1/stats/visit`. */
export interface VisitStatsResponse {
  unique: number;
  total: number;
  since?: string;
}

/** Événement du flux SSE `GET /v1/intensity/stream`. */
export interface StreamEvent {
  region: string;
  timestamp: string;
  intensity: number;
  methodology: string;
  methodology_version: number;
  unit: string;
}

/** Corps de `POST /v1/webhooks` (clé API requise). */
export interface CreateWebhookRequest {
  region?: Region;
  threshold: number;
  direction: ThresholdDirection;
  callback_url: string;
}

/** Réponse de création : inclut le `secret` (affiché une seule fois). */
export interface CreatedWebhookResponse {
  id: string;
  secret: string;
  region: string;
  threshold: number;
  direction: ThresholdDirection;
  callback_url: string;
}

export interface WebhookSummary {
  id: string;
  region: string;
  threshold: number;
  direction: ThresholdDirection;
  callback_url: string;
}

/** `GET /v1/webhooks`. */
export interface WebhookListResponse {
  count: number;
  webhooks: WebhookSummary[];
}

/**
 * Corps d'erreur de l'API, au format **Problem Details** (RFC 9457,
 * `application/problem+json`). Le champ d'extension `code` est l'identifiant
 * court **stable** sur lequel s'aligner (`no_data`, `bad_request`, …).
 */
export interface ProblemDetails {
  /** Type de problème (URI) ; `about:blank` quand `status` + `code` suffisent. */
  type: string;
  /** Résumé court et lisible du type de problème. */
  title: string;
  /** Code de statut HTTP. */
  status: number;
  /** Explication lisible spécifique à cette occurrence. */
  detail: string;
  /** Code court stable et machine-lisible (extension carbon-fr). */
  code: string;
}
