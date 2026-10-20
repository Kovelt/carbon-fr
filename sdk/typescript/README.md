# @carbon-fr/sdk

Client **TypeScript** de l'API [carbon-fr](https://github.com/Kovelt/carbon-fr) — l'intensité carbone de l'électricité française (gCO₂eq/kWh), souveraine et _dev-first_.

- **Zéro dépendance runtime** (utilise `fetch` natif — navigateur, Node ≥ 22, Deno, Bun). Node 22 est le plancher testé en CI (plus ancienne LTS maintenue) ; le code n'utilise que `fetch`, disponible dès Node 18.
- **Typé de bout en bout** : chaque endpoint de **données** `/v1` a sa méthode et son type de réponse (les endpoints d'exploitation/spec — `/health`, `/metrics`, `/v1/openapi.json` — ne sont pas exposés).
- **Flux live** (SSE) exposé en `AsyncGenerator`, avec reconnexion automatique.

> **Versions** : ce README documente la **0.5.0** (publication npm par le tag `sdk-v0.5.0`). Jusqu'à la 0.4.0, ni le champ `index` (index qualitatif, serveur ≥ 0.11) ni `intensityIndex()` n'existent ; jusqu'à la 0.3.0, `stream()` n'a ni reconnexion ni délai d'inactivité (il s'arrête en silence à la première coupure) et un abort y lève une `AbortError` ; la 0.2.0 n'expose ni `intensityNowAll`, ni `regions`, ni `MixResponse.shares` ; la 0.1.0 n'expose ni `price`/`priceHistory`, ni `costReference`, ni `eligibilityRulesets`, ni l'option `eligibility` de `greenestWindow` — vérifier la version installée (`npm ls @carbon-fr/sdk`).

## Installation

```bash
npm install @carbon-fr/sdk
```

## Démarrage

```ts
import { CarbonFr } from "@carbon-fr/sdk";

const cf = new CarbonFr(); // instance hébergée par défaut

// Intensité courante (national, rte-direct)
const now = await cf.intensityNow();
console.log(now.intensity.value, now.intensity.unit); // 14 gCO2eq/kWh

// Mix d'une région
const mix = await cf.mix({ region: "bretagne" });

// acv-ademe consumption-based (imports inclus, national)
const conso = await cf.intensityNow({ methodology: "acv-ademe", version: 2 });

// Prévision avec intervalle d'incertitude
const fc = await cf.forecast({ horizonHours: 24 });
for (const p of fc.data) console.log(p.timestamp, p.lower, p.expected, p.upper);

// Échanges transfrontaliers (carte des voisins)
const ex = await cf.exchanges();
for (const c of ex.exchanges) console.log(c.country_name, c.flow_mw, c.intensity.value);

// Renouvelable estimé depuis la météo (facteur de charge)
const ren = await cf.renewable();
console.log(`éolien ${(ren.wind_capacity_factor * 100).toFixed(0)}% de capacité`);

// Éligibilité électrolyseur : fenêtre verte annotée RFNBO ou bas-carbone (ADR-0025/0026)
const win = await cf.greenestWindow({ horizonHours: 24, eligibility: "low-carbon" });
console.log(win.eligibility?.window_eligible, win.eligibility?.count_eligible);
for (const s of win.eligibility?.slots ?? []) console.log(s.timestamp, s.eligible, s.score);

// Catalogue des cadres d'éligibilité (rulesets versionnés, seuils, base légale)
const rs = await cf.eligibilityRulesets();
for (const r of rs.rulesets) console.log(r.version, r.status, r.legal_basis);
```

## Flux temps réel (SSE)

```ts
const ac = new AbortController();
for await (const e of cf.stream({ below: 50, signal: ac.signal })) {
  console.log(e.timestamp, e.intensity);
}
// ac.abort() pour arrêter : la boucle se termine proprement, sans exception.
```

Options de `stream()` (type `StreamOptions`), toutes facultatives :

```ts
cf.stream({
  region: "bretagne",    // filtre région (défaut : toutes les régions)
  below: 50,             // seulement les mises à jour sous ce seuil (gCO₂eq/kWh)
  signal: ac.signal,     // arrêt propre
  idleTimeoutMs: 45_000, // délai d'inactivité (défaut DEFAULT_IDLE_TIMEOUT_MS)
  reconnect: { initialBackoffMs: 1_000, maxBackoffMs: 30_000 }, // défauts ; false la désactive
});
```

Une durée nulle, négative ou `NaN` lève une `RangeError` dès la première itération ; `idleTimeoutMs: Infinity` désactive le délai d'inactivité (déconseillé, cf. ci-dessous).

## Timeouts et reconnexion

- **Appels REST** : aucun délai propre au SDK, seuls ceux du `fetch` sous-jacent s'appliquent — écart assumé avec le SDK Rust, qui pose 30 s par défaut.
- **Le flux SSE** (`stream`) n'a **pas de timeout total** : il dure tant qu'on le lit. Il a en revanche un **délai d'inactivité** (`idleTimeoutMs`, défaut 45 s) : sans aucun octet reçu pendant ce délai — en-têtes de la réponse, puis trames ou keep-alive —, la connexion est jugée morte. Le serveur envoie un keep-alive toutes les ~15 s : une connexion vivante n'est jamais prise pour morte. Le même délai borne l'attente des en-têtes (le SDK Rust a pour cela un délai de connexion séparé, 10 s).
- **Reconnexion** activée par défaut (`reconnect`) : fermeture du flux par le serveur, erreur de transport ou délai d'inactivité dépassé déclenchent une nouvelle connexion après un backoff exponentiel — `initialBackoffMs` (1 s) avant la première tentative, doublé à chaque échec, plafonné à `maxBackoffMs` (30 s), remis au délai initial dès qu'une connexion aboutit —, sans limite de tentatives. Elle est **silencieuse** : la boucle `for await` reçoit simplement l'événement suivant, sans erreur intermédiaire.
- **Réponses en erreur à la (re)connexion** : `408`, `429` et `5xx` sont rejouées avec ce backoff — dont le `503` que renvoie le serveur une fois son plafond de connexions SSE atteint (`Retry-After: 30`, non lu : le backoff s'applique). Les **autres 4xx** (clé invalide `401`, paramètre invalide `400`…) lèvent aussitôt une `CarbonFrError`, sans rejouer : c'est une erreur de configuration, pas une panne passagère. Écart assumé avec le SDK Rust, qui rejoue toute réponse en erreur tant que sa reconnexion est active.
- **Pas de `Last-Event-ID`** : le serveur n'en émet pas, et sa source (un canal de diffusion éphémère en mémoire) ne permettrait de toute façon aucun rejeu. **Des événements peuvent être perdus pendant une coupure** : la reconnexion reprend le flux en direct, elle ne rattrape pas ce qui a été manqué (pour combler un trou, interroger `intensityDate`).
- **Arrêt** : aborter `signal` termine la boucle proprement, sans exception, y compris pendant une attente de reconnexion ou une lecture bloquée ; sortir de la boucle (`break`) ferme aussi la connexion. Jusqu'à la 0.3.0, l'abort levait une `AbortError`.
- **Désactivation** (`reconnect: false`) : une fermeture du flux par le serveur termine la boucle normalement ; une erreur de transport ou toute réponse non 2xx (`503` compris) est levée, comme en 0.3.0 ; le dépassement du délai d'inactivité lève une `DOMException` nommée `TimeoutError`.

## Configuration

```ts
const cf = new CarbonFr({
  baseUrl: "https://carbon-fr-api.kovelt.fr", // défaut
  apiKey: "…",   // requis seulement pour les webhooks / quota du tier hébergé
  fetch: customFetch, // optionnel (Node < 18, mocks de test)
});
```

## Endpoints couverts

| Méthode | Endpoint |
| --- | --- |
| `intensityNow` | `GET /v1/intensity/now` |
| `intensityNowAll` | `GET /v1/intensity/now/all` (les 13 régions en un appel ; `methodology: "acv-ademe"` — `rte-direct` n'existe qu'au national) |
| `mix` | `GET /v1/mix` (MW par filière + parts `shares`) |
| `intensityDate` | `GET /v1/intensity/date` |
| `intensityStats` | `GET /v1/intensity/stats` |
| `forecast` | `GET /v1/intensity/forecast` |
| `greenestWindow` | `GET /v1/intensity/greenest-window` (overlay électrolyseur via `eligibility: "rfnbo" \| "low-carbon"` + overrides) |
| `eligibilityRulesets` | `GET /v1/eligibility/rulesets` (cadres d'éligibilité électrolyseur) |
| `schedule` · `scheduleSlots` · `below` | scheduling carbon-aware |
| `exchanges` · `exchangesHistory` | `GET /v1/exchanges[/date]` |
| `weather` · `weatherHistory` | `GET /v1/weather[/date]` |
| `renewable` | `GET /v1/renewable` |
| `methodologies` · `factors` · `regions` | méthodes, facteurs & catalogue des régions (`GET /v1/regions`) |
| `intensityIndex` | `GET /v1/intensity/index` : barème de l'**index qualitatif** (`very-low` → `very-high`, bornes sourcées) — le champ `index` porté par `intensityNow`, `intensityNowAll`, `intensityDate`, `forecast` et `stream` (serveur ≥ 0.11) |
| `price` · `priceHistory` | `GET /v1/price[/date]` (décomposition TRV, national uniquement — `region` accepté mais toute valeur non nationale renvoie une 400) |
| `costReference` | `GET /v1/cost-reference` (LCOE, estimation) |
| `visitStats` · `recordVisit` | compteur RGPD-friendly |
| `createWebhook` · `listWebhooks` · `deleteWebhook` | webhooks (clé API) |
| `stream` | `GET /v1/intensity/stream` (SSE, reconnexion automatique) |

## Gestion d'erreurs

Toute réponse non-2xx lève une `CarbonFrError` (`.status`, `.code`, `.message`) — sauf, pour `stream()`, les `408`, `429` et `5xx` absorbés par la reconnexion (cf. [Timeouts et reconnexion](#timeouts-et-reconnexion)) :

```ts
import { CarbonFrError } from "@carbon-fr/sdk";

try {
  await cf.intensityNow({ region: "atlantide" });
} catch (e) {
  if (e instanceof CarbonFrError) console.error(e.status, e.code, e.message);
}
```

## Développement

`npm test` compile le SDK puis lance ses tests (`node:test`, sans dépendance ni réseau : serveur SSE local, `fetch` injecté). `test/parity.test.mjs` vérifie la parité avec l'OpenAPI du serveur (`crates/adapter-http/tests/openapi.snapshot.json`, dans le dépôt) : toute opération `/v1` a sa méthode, et chaque méthode appelle la bonne route — il se saute hors du dépôt.

## Licence

MIT OU Apache-2.0, au choix.
