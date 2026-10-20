// Backoff de reconnexion du flux SSE, sans I/O — pendant des tests
// `backoff_doubles_and_caps_then_resets` du SDK Rust. `Backoff` est interne
// (non exporté par le paquet) : import direct du module compilé.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Backoff } from "../dist/stream.js";

test("double à chaque échec, plafonne, puis repart du délai initial après une connexion réussie", () => {
  const backoff = new Backoff(10, 45);

  const delays = Array.from({ length: 5 }, () => backoff.next());

  assert.deepEqual(delays, [10, 20, 40, 45, 45]);
  backoff.reset();
  assert.equal(backoff.next(), 10);
});

test("un délai initial au-dessus du plafond est plafonné dès la première attente", () => {
  const backoff = new Backoff(1_000, 500);

  assert.deepEqual([backoff.next(), backoff.next()], [500, 500]);
});
