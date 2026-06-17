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
