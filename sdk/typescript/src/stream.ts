// Flux SSE `GET /v1/intensity/stream` de `CarbonFr.stream()`, avec délai
// d'inactivité et reconnexion automatique : transposition sur
// `fetch`/`ReadableStream` du patron du SDK Rust (`crates/sdk/src/stream.rs`).
//
// Pas de timeout total : le délai d'inactivité (`idleTimeoutMs`) borne
// l'attente des en-têtes, puis l'écart entre deux paquets reçus, keep-alive
// compris (commentaire `:` du serveur toutes les ~15 s, qui doit rester sous
// ce délai pour qu'une connexion vivante ne soit jamais jugée morte). Fermeture
// du flux par le serveur, erreur de transport, délai dépassé ou réponse
// transitoire : reconnexion volontairement mince (backoff exponentiel plafonné),
// **sans** `Last-Event-ID` — le serveur n'en émet aucun et sa source (diffusion
// éphémère en mémoire, ADR-0014) ne permettrait aucun rejeu : des événements
// peuvent être perdus pendant une coupure.
//
// Une boucle : connexion → lecture → attente de backoff. Chaque tentative a son
// propre `AbortController`, relié au `signal` de l'appelant, qui coupe la
// requête et sa lecture. Chaque attente (en-têtes, paquet, corps d'erreur,
// backoff) cède aussi à ce signal : un arrêt termine le générateur sans lever,
// quel que soit l'état.

import { CarbonFrError, errorFromResponse } from "./errors.js";
import type { Region, StreamEvent } from "./types.js";

/**
 * Délai d'inactivité par défaut du flux SSE (ms) : sans **aucun** octet reçu
 * pendant ce délai (keep-alive compris), la connexion est jugée morte. Vaut 3×
 * l'intervalle de keep-alive du serveur (~15 s), comme `DEFAULT_IDLE_TIMEOUT`
 * du SDK Rust.
 */
export const DEFAULT_IDLE_TIMEOUT_MS = 45_000;

/** Délai par défaut avant la première tentative de reconnexion (ms). */
export const DEFAULT_INITIAL_BACKOFF_MS = 1_000;

/** Plafond par défaut du délai entre deux tentatives de reconnexion (ms). */
export const DEFAULT_MAX_BACKOFF_MS = 30_000;

/**
 * Politique de reconnexion du flux SSE : `initialBackoffMs` avant la première
 * tentative, puis délai doublé à chaque échec consécutif et plafonné à
 * `maxBackoffMs` ; il repart de `initialBackoffMs` dès qu'une connexion
 * aboutit (réponse 2xx avec corps). Nombre de tentatives **illimité** : c'est
 * le délai qui est plafonné, pas les essais — une coupure réseau transitoire ne
 * doit pas finir par arrêter le flux en silence.
 */
export interface ReconnectPolicy {
  /** Délai avant la première tentative, en ms (défaut {@link DEFAULT_INITIAL_BACKOFF_MS}). */
  initialBackoffMs?: number;
  /** Plafond du délai entre deux tentatives, en ms (défaut {@link DEFAULT_MAX_BACKOFF_MS}). */
  maxBackoffMs?: number;
}

/** Options de `CarbonFr.stream()`. */
export interface StreamOptions {
  /** Filtre région (défaut : toutes les régions poussées). */
  region?: Region;
  /** Ne pousser que les mises à jour strictement sous ce seuil (gCO₂eq/kWh). */
  below?: number;
  /**
   * Arrête le flux : le générateur se termine proprement (sans lever), y
   * compris pendant une attente de reconnexion ou une lecture bloquée.
   */
  signal?: AbortSignal;
  /**
   * Délai d'inactivité en ms (défaut {@link DEFAULT_IDLE_TIMEOUT_MS}) : sans
   * aucun octet reçu pendant ce délai — en-têtes de la réponse, puis trames ou
   * keep-alive —, la connexion est jugée morte. Réarmé à chaque paquet reçu.
   * `Infinity` le désactive (déconseillé : une connexion coupée sans fermeture
   * ne serait alors jamais détectée).
   */
  idleTimeoutMs?: number;
  /**
   * Reconnexion automatique (défaut : activée, cf. {@link ReconnectPolicy}).
   * `false` la désactive : une fermeture du flux par le serveur termine alors
   * le générateur (retour normal), une erreur de transport ou toute réponse
   * non 2xx est levée, et le dépassement du délai d'inactivité lève une
   * `DOMException` nommée `TimeoutError`.
   */
  reconnect?: false | ReconnectPolicy;
}

/**
 * Ouvre une connexion SSE (requête `GET` déjà construite par le client),
 * interrompue par `signal`.
 *
 * @internal
 */
export type OpenStream = (signal: AbortSignal) => Promise<Response>;

/**
 * Backoff exponentiel (×2) plafonné, remis au délai initial après une
 * connexion réussie — même logique que `next_backoff`/`reset_backoff` du SDK
 * Rust, le premier délai étant lui aussi plafonné.
 *
 * @internal Exporté pour les tests unitaires (`test/backoff.test.mjs`) ; non
 * exporté par le paquet.
 */
export class Backoff {
  private readonly initialMs: number;
  private readonly maxMs: number;
  private currentMs: number;

  constructor(initialMs: number, maxMs: number) {
    this.initialMs = initialMs;
    this.maxMs = maxMs;
    this.currentMs = initialMs;
  }

  /** Délai de la prochaine attente ; double (dans la limite du plafond) le suivant. */
  next(): number {
    const delay = Math.min(this.currentMs, this.maxMs);
    this.currentMs = Math.min(this.currentMs * 2, this.maxMs);
    return delay;
  }

  /** Une connexion a abouti : la prochaine attente repart du délai initial. */
  reset(): void {
    this.currentMs = this.initialMs;
  }
}

/**
 * Moteur de `CarbonFr.stream()` : connexion → lecture → attente de backoff,
 * en boucle, jusqu'à l'arrêt par `signal` — ou jusqu'à la première coupure si
 * la reconnexion est désactivée.
 *
 * @internal
 */
export async function* streamEvents(
  open: OpenStream,
  opts: StreamOptions,
): AsyncGenerator<StreamEvent, void, unknown> {
  const idleTimeoutMs = positiveMs("idleTimeoutMs", opts.idleTimeoutMs ?? DEFAULT_IDLE_TIMEOUT_MS);
  const backoff = opts.reconnect === false ? undefined : backoffFrom(opts.reconnect);
  const { signal } = opts;

  while (!signal?.aborted) {
    // Un contrôleur par tentative, qui relaie l'arrêt de l'appelant ; il est
    // aborté en fin de tentative quoi qu'il arrive (délai dépassé, `break` de
    // l'appelant, erreur), ce qui libère la connexion.
    const attempt = new AbortController();
    const relay = () => attempt.abort();
    signal?.addEventListener("abort", relay, { once: true });
    let end: Disconnect;
    try {
      const opened = await settle(callOpen(open, attempt.signal), idleTimeoutMs, signal);
      if (opened.kind === "aborted") return;
      if (opened.kind !== "value") {
        // Erreur de transport, ou en-têtes non reçus dans le délai.
        end = opened;
      } else if (opened.value.ok && opened.value.body) {
        // Connexion réussie : le backoff repart du délai initial (parité Rust).
        backoff?.reset();
        const read = yield* readEvents(opened.value.body, idleTimeoutMs, signal);
        if (read === undefined) return;
        end = read;
      } else if (backoff && isTransient(opened.value.status)) {
        // Réponse transitoire : corps abandonné, nouvelle tentative.
        opened.value.body?.cancel().catch(() => {});
        end = { kind: "status" };
      } else {
        // Autre statut (4xx hors 408/429, 2xx sans corps…) ou reconnexion
        // désactivée : erreur levée sans rejouer, corps Problem Details lu
        // avant de couper la connexion.
        const error = await settle(errorFromResponse(opened.value), idleTimeoutMs, signal);
        if (error.kind === "aborted") return;
        throw error.kind === "value" ? error.value : new CarbonFrError(opened.value.status, {});
      }
    } finally {
      signal?.removeEventListener("abort", relay);
      attempt.abort();
    }

    if (!backoff) {
      // Reconnexion désactivée (une réponse non 2xx a déjà été levée plus haut).
      if (end.kind === "closed") return;
      if (end.kind === "error") throw end.error;
      throw new DOMException(
        `flux SSE : aucun octet reçu depuis ${idleTimeoutMs} ms (délai d'inactivité dépassé)`,
        "TimeoutError",
      );
    }
    if (!(await sleep(backoff.next(), signal))) return;
  }
}

// ─── Internes ─────────────────────────────────────────────────────────────

/** Plus grand délai accepté par `setTimeout` (2³¹ − 1 ms ≈ 24,8 jours) : au-delà, il retomberait à ~1 ms. */
const MAX_TIMER_MS = 2_147_483_647;

/**
 * Cause de la fin d'une tentative de connexion :
 * - `closed` : le serveur a fermé le flux (un flux SSE est censé durer, c'est
 *   donc une coupure, pas une fin normale) ;
 * - `idle` : aucun octet reçu pendant le délai d'inactivité (en-têtes ou corps) ;
 * - `error` : `fetch` ou lecture du corps rejetés (réseau, TLS, connexion
 *   réinitialisée…) ;
 * - `status` : réponse transitoire (408, 429, 5xx), seulement avec la
 *   reconnexion active.
 */
type Disconnect =
  | { kind: "closed" }
  | { kind: "idle" }
  | { kind: "error"; error: unknown }
  | { kind: "status" };

/** Issue d'une attente bornée par {@link settle}. */
type Settled<T> =
  | { kind: "value"; value: T }
  | { kind: "error"; error: unknown }
  | { kind: "idle" }
  | { kind: "aborted" };

/**
 * Lit le corps d'une connexion établie et produit ses événements, jusqu'à la
 * coupure — dont la cause est renvoyée — ou jusqu'à l'arrêt par `signal`
 * (`undefined`).
 */
async function* readEvents(
  body: NonNullable<Response["body"]>,
  idleTimeoutMs: number,
  signal: AbortSignal | undefined,
): AsyncGenerator<StreamEvent, Disconnect | undefined, unknown> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  try {
    while (true) {
      // Délai réarmé à chaque paquet brut, keep-alive compris : un commentaire
      // `: …` ne produit aucun événement mais prouve que la connexion vit.
      const chunk = await settle(reader.read(), idleTimeoutMs, signal);
      if (chunk.kind === "aborted") return undefined;
      if (chunk.kind !== "value") return chunk;
      if (chunk.value.done) return { kind: "closed" };
      buffer += decoder.decode(chunk.value.value, { stream: true });
      let sep: number;
      // Une trame SSE est terminée par une ligne vide (\n\n).
      while ((sep = buffer.indexOf("\n\n")) >= 0) {
        const event = parseFrame(buffer.slice(0, sep));
        buffer = buffer.slice(sep + 2);
        if (event === undefined) continue;
        yield event;
        if (signal?.aborted) return undefined;
      }
    }
  } finally {
    // Libère la connexion même si le `fetch` fourni ignore son signal ; sans
    // attendre (une annulation peut rester pendante).
    reader.cancel().catch(() => {});
  }
}

/** Événement porté par une trame SSE ; `undefined` pour un keep-alive (`: …`), une trame sans `data:` ou un JSON invalide. */
function parseFrame(frame: string): StreamEvent | undefined {
  const data = frame
    .split("\n")
    .filter((l) => l.startsWith("data:"))
    .map((l) => l.slice(5).trimStart())
    .join("\n");
  if (!data) return undefined;
  try {
    return JSON.parse(data) as StreamEvent;
  } catch {
    // Trame non-JSON : ignorée.
    return undefined;
  }
}

/**
 * Attend `promise`, bornée par `timeoutMs` (`Infinity` : sans limite) et par
 * l'arrêt de l'appelant ; ne rejette jamais (issue discriminée). Le timer et
 * l'écouteur sont retirés dès l'issue connue, et la promesse abandonnée garde
 * ses gestionnaires : son rejet tardif ne devient jamais un rejet non géré.
 */
function settle<T>(
  promise: Promise<T>,
  timeoutMs: number,
  signal: AbortSignal | undefined,
): Promise<Settled<T>> {
  return new Promise((resolve) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const onAbort = () => finish({ kind: "aborted" });
    const finish = (outcome: Settled<T>) => {
      clearTimeout(timer);
      signal?.removeEventListener("abort", onAbort);
      resolve(outcome);
    };
    promise.then(
      (value) => finish({ kind: "value", value }),
      (error: unknown) => finish({ kind: "error", error }),
    );
    if (signal?.aborted) {
      finish({ kind: "aborted" });
      return;
    }
    signal?.addEventListener("abort", onAbort, { once: true });
    if (Number.isFinite(timeoutMs)) {
      timer = setTimeout(() => finish({ kind: "idle" }), Math.min(timeoutMs, MAX_TIMER_MS));
    }
  });
}

/** Attend `ms` ; `false` si l'appelant arrête le flux pendant l'attente (timer annulé aussitôt). */
function sleep(ms: number, signal: AbortSignal | undefined): Promise<boolean> {
  return new Promise((resolve) => {
    if (signal?.aborted) {
      resolve(false);
      return;
    }
    const onAbort = () => {
      clearTimeout(timer);
      resolve(false);
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve(true);
    }, Math.min(ms, MAX_TIMER_MS));
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

/** Appelle `open` en convertissant une exception synchrone (`fetch` fourni par l'appelant) en rejet. */
async function callOpen(open: OpenStream, signal: AbortSignal): Promise<Response> {
  return open(signal);
}

/**
 * Réponse à rejouer : 408, 429 et 5xx — dont le `503` du plafond de connexions
 * SSE du serveur (`Retry-After: 30`, non lu : le backoff s'applique). Les
 * autres 4xx (clé invalide, paramètre invalide…) sont des erreurs de
 * configuration, levées sans rejouer — écart assumé avec le SDK Rust, qui
 * rejoue toute réponse en erreur.
 */
function isTransient(status: number): boolean {
  return status === 408 || status === 429 || status >= 500;
}

function backoffFrom(policy: ReconnectPolicy | undefined): Backoff {
  const p: ReconnectPolicy = typeof policy === "object" && policy !== null ? policy : {};
  return new Backoff(
    positiveMs("reconnect.initialBackoffMs", p.initialBackoffMs ?? DEFAULT_INITIAL_BACKOFF_MS),
    positiveMs("reconnect.maxBackoffMs", p.maxBackoffMs ?? DEFAULT_MAX_BACKOFF_MS),
  );
}

/** Valide une durée : un nombre > 0 (`Infinity` accepté) ; 0, négatif ou `NaN` lèvent une `RangeError`. */
function positiveMs(name: string, value: number): number {
  if (typeof value !== "number" || !(value > 0)) {
    throw new RangeError(
      `stream() : \`${name}\` doit être une durée en millisecondes > 0 (reçu : ${String(value)})`,
    );
  }
  return value;
}
