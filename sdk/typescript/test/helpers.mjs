// Outils partagés des tests du SDK : serveur HTTP local (`node:http`) et
// trames SSE au format du serveur carbon-fr (`event: intensity` + `data:` JSON,
// keep-alive en commentaire `:`). Pas un fichier de test (`*.test.mjs`).

import http from "node:http";

/**
 * Démarre un serveur HTTP local sur un port libre, arrêté en fin de test
 * (`t.after`) en coupant aussi les connexions encore ouvertes (flux SSE).
 * Renvoie l'URL de base à passer au client.
 */
export async function serve(t, handler) {
  const server = http.createServer(handler);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(
    () =>
      new Promise((resolve) => {
        server.closeAllConnections();
        server.close(() => resolve());
      }),
  );
  const { port } = server.address();
  return `http://127.0.0.1:${port}`;
}

/**
 * `AbortController` aborté en fin de test — succès, échec ou dépassement du
 * `timeout` : un flux resté en reconnexion (régression) ne survit pas à son
 * test et n'empêche pas le fichier de se terminer.
 */
export function abortAfter(t) {
  const controller = new AbortController();
  t.after(() => controller.abort());
  return controller;
}

/** Ouvre une réponse SSE et envoie aussitôt les en-têtes (le corps suit, ou pas). */
export function openSse(res) {
  res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache" });
  res.flushHeaders();
}

/** Bande de l'index qualitatif d'une intensité (barème `index@1`, ADR-0033). */
export function indexOf(intensity) {
  if (intensity < 20) return "very-low";
  if (intensity < 64.8) return "low";
  if (intensity < 100) return "moderate";
  if (intensity < 200) return "high";
  return "very-high";
}

/** Trame SSE d'un événement `intensity` (corps `StreamEventBody` du serveur). */
export function eventFrame(region, intensity) {
  const body = {
    region,
    timestamp: "2026-10-20T12:00:00Z",
    intensity,
    index: indexOf(intensity),
    methodology: "rte-direct",
    methodology_version: 1,
    unit: "gCO2eq/kWh",
  };
  return `event: intensity\ndata: ${JSON.stringify(body)}\n\n`;
}

/** Keep-alive : commentaire SSE, jamais un événement. */
export const KEEP_ALIVE = ": hb\n\n";

/** Répond une erreur Problem Details (RFC 9457), comme le serveur. */
export function problem(res, status, code, detail, headers = {}) {
  const body = { type: "about:blank", title: http.STATUS_CODES[status], status, detail, code };
  res.writeHead(status, { "content-type": "application/problem+json", ...headers });
  res.end(JSON.stringify(body));
}

/**
 * Lit au plus `count` événements du flux puis sort de la boucle (`break`, qui
 * referme la connexion) ; s'arrête plus tôt si le flux se termine.
 */
export async function take(stream, count) {
  const events = [];
  for await (const event of stream) {
    events.push(event);
    if (events.length >= count) break;
  }
  return events;
}

/** Promesse résolue au premier appel de la fonction renvoyée avec elle. */
export function signalled() {
  let resolve;
  const promise = new Promise((r) => (resolve = r));
  return { promise, resolve: () => resolve() };
}

export const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Attend que `predicate()` soit vrai (borné par le `timeout` du test). */
export async function until(predicate) {
  while (!predicate()) await delay(5);
}
