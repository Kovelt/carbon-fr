# @carbon-fr/sdk

Client **TypeScript** de l'API [carbon-fr](https://github.com/Kovelt/carbon-fr) — l'intensité carbone de l'électricité française (gCO₂eq/kWh), souveraine et _dev-first_.

- **Zéro dépendance runtime** (utilise `fetch` natif — navigateur, Node ≥ 18, Deno, Bun).
- **Typé de bout en bout** : chaque endpoint de **données** `/v1` a sa méthode et son type de réponse (les endpoints d'exploitation/spec — `/health`, `/metrics`, `/v1/openapi.json` — ne sont pas exposés).
- **Flux live** (SSE) exposé en `AsyncGenerator`.

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
```

## Flux temps réel (SSE)

```ts
const ac = new AbortController();
for await (const e of cf.stream({ below: 50, signal: ac.signal })) {
  console.log(e.timestamp, e.intensity);
}
// ac.abort() pour arrêter.
```

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
| `mix` | `GET /v1/mix` |
| `intensityDate` | `GET /v1/intensity/date` |
| `intensityStats` | `GET /v1/intensity/stats` |
| `forecast` | `GET /v1/intensity/forecast` |
| `greenestWindow` | `GET /v1/intensity/greenest-window` |
| `schedule` · `scheduleSlots` · `below` | scheduling carbon-aware |
| `exchanges` · `exchangesHistory` | `GET /v1/exchanges[/date]` |
| `weather` · `weatherHistory` | `GET /v1/weather[/date]` |
| `renewable` | `GET /v1/renewable` |
| `methodologies` · `factors` | méthodes & facteurs |
| `price` · `priceHistory` | `GET /v1/price[/date]` (décomposition TRV) |
| `costReference` | `GET /v1/cost-reference` (LCOE, estimation) |
| `visitStats` · `recordVisit` | compteur RGPD-friendly |
| `createWebhook` · `listWebhooks` · `deleteWebhook` | webhooks (clé API) |
| `stream` | `GET /v1/intensity/stream` (SSE) |

## Gestion d'erreurs

Toute réponse non-2xx lève une `CarbonFrError` (`.status`, `.code`, `.message`) :

```ts
import { CarbonFrError } from "@carbon-fr/sdk";

try {
  await cf.intensityNow({ region: "atlantide" });
} catch (e) {
  if (e instanceof CarbonFrError) console.error(e.status, e.code, e.message);
}
```

## Licence

MIT OU Apache-2.0, au choix.
