// Parité SDK TypeScript ↔ OpenAPI (ADR-0032, item DX-3 du plan), symétrique de
// `crates/sdk/tests/parity.rs` : une table `operationId → méthode du client`
// vérifiée à chaque run contre `crates/adapter-http/tests/openapi.snapshot.json`
// (le contrat `/v1` figé en CI), dans le job `sdk-typescript`, sans dépendance.
//
// Trois garde-fous :
//   1. toute opération `/v1` du snapshot a sa méthode, ou une exclusion
//      documentée (`EXCLUDED`) — une route ajoutée côté serveur sans méthode
//      SDK fait échouer la CI (la dérive constatée à la sortie de la 0.2.0) ;
//   2. la table ne cite ni opération absente du snapshot (route renommée ou
//      retirée), ni méthode absente du client ; et toute méthode du client est
//      soit dans la table, soit une utilitaire tolérée (`UTILITIES`) ;
//   3. chaque méthode REST appelle bien le chemin et la méthode HTTP de son
//      opération — `fetch` injecté, aucun réseau — ce que `tests/rest.rs` fait
//      côté Rust. Le flux SSE est couvert par `stream.test.mjs`.
//
// Importe le build (`dist/`) : lancer par `npm test` (build puis `node --test`).

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { describe, test } from "node:test";

import { CarbonFr } from "../dist/index.js";

const SNAPSHOT = new URL(
  "../../../crates/adapter-http/tests/openapi.snapshot.json",
  import.meta.url,
);

/**
 * `operationId` du snapshot → méthode du client, avec les arguments minimaux de
 * l'appel de vérification (garde-fou 3). `route: false` : opération dont la
 * route est vérifiée ailleurs (flux SSE).
 */
const COVERED = [
  { operation: "cost_reference", method: "costReference" },
  { operation: "create_webhook", method: "createWebhook", args: [{ threshold: 50 }] },
  { operation: "delete_webhook", method: "deleteWebhook", args: ["abc-123"] },
  { operation: "eligibility_rulesets", method: "eligibilityRulesets" },
  { operation: "exchanges", method: "exchanges" },
  { operation: "exchanges_date", method: "exchangesHistory", args: [{ from: "2026-10-19T00:00:00Z", to: "2026-10-20T00:00:00Z" }] },
  { operation: "factors", method: "factors" },
  { operation: "forecast", method: "forecast" },
  { operation: "greenest_window", method: "greenestWindow" },
  { operation: "intensity_below", method: "below", args: [{ threshold: 20 }] },
  { operation: "intensity_date", method: "intensityDate", args: [{ from: "2026-10-19T00:00:00Z", to: "2026-10-20T00:00:00Z" }] },
  { operation: "intensity_index", method: "intensityIndex" },
  { operation: "intensity_now", method: "intensityNow" },
  { operation: "intensity_now_all", method: "intensityNowAll" },
  { operation: "intensity_stats", method: "intensityStats", args: [{ from: "2026-10-19T00:00:00Z", to: "2026-10-20T00:00:00Z" }] },
  { operation: "intensity_stream", method: "stream", route: false },
  { operation: "list_webhooks", method: "listWebhooks" },
  { operation: "methodologies", method: "methodologies" },
  { operation: "mix", method: "mix" },
  { operation: "price", method: "price" },
  { operation: "price_date", method: "priceHistory", args: [{ from: "2026-10-19T00:00:00Z", to: "2026-10-20T00:00:00Z" }] },
  { operation: "record_visit", method: "recordVisit" },
  { operation: "regions", method: "regions" },
  { operation: "renewable", method: "renewable" },
  { operation: "schedule", method: "schedule" },
  { operation: "schedule_slots", method: "scheduleSlots", args: [{ count: 3 }] },
  { operation: "visit_stats", method: "visitStats" },
  { operation: "weather", method: "weather" },
  { operation: "weather_date", method: "weatherHistory", args: [{ from: "2026-10-19T00:00:00Z", to: "2026-10-20T00:00:00Z" }] },
];

/** Hors périmètre v1 par décision explicite (ADR-0031 décision 8, ADR-0032) : sondes d'infra. */
const EXCLUDED = ["health", "health_ready"];

/** Méthodes du client sans opération : plomberie HTTP interne (TypeScript `private`, présentes sur le prototype). */
const UTILITIES = ["get", "request", "buildUrl", "authHeaders"];

/** `operationId` → `{ method, path }` depuis le snapshot OpenAPI. */
function loadOperations() {
  const doc = JSON.parse(readFileSync(SNAPSHOT, "utf8"));
  const operations = new Map();
  for (const [path, item] of Object.entries(doc.paths)) {
    for (const [method, operation] of Object.entries(item)) {
      if (operation && typeof operation === "object" && operation.operationId) {
        assert.equal(
          operations.has(operation.operationId),
          false,
          `operationId en double dans le snapshot : ${operation.operationId}`,
        );
        operations.set(operation.operationId, { method: method.toUpperCase(), path });
      }
    }
  }
  return operations;
}

/** Gabarit OpenAPI (`/v1/webhooks/{id}`) → expression régulière sur le chemin. */
function pathPattern(template) {
  const escaped = template.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/\\\{[^}]+\\\}/g, "[^/]+");
  return new RegExp(`^${escaped}$`);
}

const skip = existsSync(SNAPSHOT) ? false : "snapshot OpenAPI introuvable (hors du dépôt)";

describe("parité SDK TS ↔ OpenAPI", { skip }, () => {
  test("la table couvre exactement les opérations /v1 du snapshot", () => {
    const operations = loadOperations();
    assert.ok(
      operations.size >= 20,
      `snapshot OpenAPI suspicieusement petit (${operations.size} opérations) — le fichier lu est-il le bon ?`,
    );

    const covered = new Set(COVERED.map((c) => c.operation));
    assert.equal(covered.size, COVERED.length, "operationId en double dans la table COVERED");
    const methods = new Set(COVERED.map((c) => c.method));
    assert.equal(methods.size, COVERED.length, "méthode citée deux fois dans la table COVERED");

    // (1) Toute opération du snapshot est couverte, ou explicitement exclue.
    const uncovered = [...operations.keys()].filter((id) => !covered.has(id) && !EXCLUDED.includes(id));
    assert.deepEqual(
      uncovered,
      [],
      `opérations /v1 du snapshot sans méthode SDK ni exclusion documentée : ${uncovered.join(", ")}`,
    );

    // (2) Ni opération fantôme dans la table, ni exclusion périmée.
    const stale = [...covered].filter((id) => !operations.has(id));
    assert.deepEqual(stale, [], `la table COVERED cite des opérations absentes du snapshot : ${stale.join(", ")}`);
    const staleExclusions = EXCLUDED.filter((id) => !operations.has(id));
    assert.deepEqual(staleExclusions, [], `EXCLUDED cite des opérations absentes du snapshot : ${staleExclusions.join(", ")}`);
  });

  test("chaque méthode de la table existe sur le client, et chaque méthode du client est dans la table ou utilitaire", () => {
    const prototype = Object.getOwnPropertyNames(CarbonFr.prototype).filter((n) => n !== "constructor");
    for (const { method } of COVERED) {
      assert.equal(typeof CarbonFr.prototype[method], "function", `méthode absente du client : CarbonFr.${method}()`);
    }
    const expected = new Set([...COVERED.map((c) => c.method), ...UTILITIES]);
    const unknown = prototype.filter((n) => !expected.has(n));
    assert.deepEqual(
      unknown,
      [],
      `méthodes du client ni dans la table de parité ni utilitaires : ${unknown.join(", ")} — ajouter l'opération couverte (ou la méthode à UTILITIES si c'est de la plomberie)`,
    );
    const missingUtilities = UTILITIES.filter((n) => !prototype.includes(n));
    assert.deepEqual(missingUtilities, [], `UTILITIES cite des méthodes absentes du client : ${missingUtilities.join(", ")}`);
  });

  test("chaque méthode REST appelle le chemin et la méthode HTTP de son opération", async () => {
    const operations = loadOperations();
    for (const entry of COVERED) {
      if (entry.route === false) continue;
      const expected = operations.get(entry.operation);
      const calls = [];
      const fetchImpl = async (url, init = {}) => {
        calls.push({ url: new URL(url), method: (init.method ?? "GET").toUpperCase() });
        return expected.method === "DELETE"
          ? new Response(null, { status: 204 })
          : new Response("{}", { status: 200, headers: { "content-type": "application/json" } });
      };
      const cf = new CarbonFr({ baseUrl: "http://127.0.0.1:9", fetch: fetchImpl });

      await cf[entry.method](...(entry.args ?? []));

      assert.equal(calls.length, 1, `${entry.method}() : un seul appel réseau attendu, ${calls.length} vu(s)`);
      assert.equal(calls[0].method, expected.method, `${entry.method}() : méthode HTTP`);
      assert.match(
        calls[0].url.pathname,
        pathPattern(expected.path),
        `${entry.method}() appelle ${calls[0].url.pathname}, l'opération ${entry.operation} est sur ${expected.path}`,
      );
    }
  });
});
