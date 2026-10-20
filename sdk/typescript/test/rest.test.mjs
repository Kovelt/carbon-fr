// Index qualitatif (ADR-0033) côté REST : `intensityIndex()` lit le barème sur
// `GET /v1/intensity/index`, et le champ optionnel `index` des réponses
// d'intensité passe tel quel (présent avec un serveur ≥ 0.11, absent avant).
// Le catalogue servi est la fixture du SDK Rust (sortie réelle du serveur),
// pour que les deux SDK testent le même document.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, test } from "node:test";

import { CarbonFr } from "../dist/index.js";
import { serve } from "./helpers.mjs";

const CATALOG = readFileSync(
  new URL("../../../crates/sdk/tests/fixtures/intensity_index.json", import.meta.url),
  "utf8",
);
const LIMIT = { timeout: 5_000 };

function json(res, body) {
  res.writeHead(200, { "content-type": "application/json" });
  res.end(typeof body === "string" ? body : JSON.stringify(body));
}

describe("index qualitatif", () => {
  test("intensityIndex() lit le barème sur GET /v1/intensity/index", LIMIT, async (t) => {
    let request;
    const baseUrl = await serve(t, (req, res) => {
      request = req;
      json(res, CATALOG);
    });

    const bareme = await new CarbonFr({ baseUrl }).intensityIndex();

    assert.equal(request.method, "GET");
    assert.equal(new URL(request.url, baseUrl).pathname, "/v1/intensity/index");
    assert.equal(bareme.version, 1);
    assert.equal(bareme.unit, "gCO2eq/kWh");
    assert.deepEqual(
      bareme.bands.map((b) => b.label),
      ["very-low", "low", "moderate", "high", "very-high"],
    );
    assert.equal(bareme.bands[0].min, null);
    assert.equal(bareme.bands[4].max, null);
    for (let i = 1; i < bareme.bands.length; i++) {
      assert.equal(bareme.bands[i].min, bareme.bands[i - 1].max);
    }
    assert.deepEqual(
      bareme.thresholds.map((th) => th.value),
      [20, 64.8, 100, 200],
    );
    assert.equal(bareme.thresholds[1].below, "low");
    assert.equal(bareme.thresholds[1].above, "moderate");
    assert.match(bareme.thresholds[1].url, /^https:\/\/eur-lex\.europa\.eu\//);
  });

  test("le champ index des réponses d'intensité passe tel quel, et reste absent face à un serveur ancien", LIMIT, async (t) => {
    const point = {
      region: "national",
      timestamp: "2026-10-20T12:00:00Z",
      intensity: { value: 34, unit: "gCO2eq/kWh" },
      methodology: "rte-direct",
      methodology_version: 1,
      vintage: "tr",
    };
    const baseUrl = await serve(t, (req, res) => {
      const url = new URL(req.url, baseUrl);
      json(res, url.searchParams.get("region") === "bretagne" ? point : { ...point, index: "low" });
    });
    const cf = new CarbonFr({ baseUrl });

    const recent = await cf.intensityNow();
    assert.equal(recent.index, "low");
    assert.equal(recent.intensity.value, 34);

    const ancien = await cf.intensityNow({ region: "bretagne" });
    assert.equal(ancien.index, undefined);
    assert.equal(ancien.vintage, "tr");
  });
});
