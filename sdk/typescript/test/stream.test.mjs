// Flux SSE `CarbonFr.stream()` contre un serveur HTTP local : réception,
// reconnexion (fermeture, transport, inactivité, réponses transitoires),
// erreurs non rejouées, arrêt, reconnexion désactivée, options. Pendant des
// tests de `crates/sdk/src/stream.rs` (SDK Rust). Importe le build (`dist/`) :
// lancer par `npm test` (build puis `node --test`).
//
// Chaque test borne sa durée (`timeout`), ferme son serveur et aborte ses flux
// à la fin (`abortAfter`) : une régression échoue vite au lieu de bloquer.

import assert from "node:assert/strict";
import { describe, test } from "node:test";

import {
  CarbonFr,
  CarbonFrError,
  DEFAULT_IDLE_TIMEOUT_MS,
  DEFAULT_INITIAL_BACKOFF_MS,
  DEFAULT_MAX_BACKOFF_MS,
} from "../dist/index.js";
import {
  KEEP_ALIVE,
  abortAfter,
  delay,
  eventFrame,
  openSse,
  problem,
  serve,
  signalled,
  take,
  until,
} from "./helpers.mjs";

/** Backoff court (le défaut est 1 s → 30 s). */
const FAST = { initialBackoffMs: 10, maxBackoffMs: 50 };
/** Durée maximale de chaque test. */
const LIMIT = { timeout: 5_000 };

describe("réception", () => {
  test("produit les événements, ignore les keep-alive, transmet filtres et en-têtes", LIMIT, async (t) => {
    let request;
    const baseUrl = await serve(t, (req, res) => {
      request = req;
      openSse(res);
      res.write(KEEP_ALIVE);
      res.write(KEEP_ALIVE);
      res.write(eventFrame("bretagne", 42));
      res.write(KEEP_ALIVE);
      res.write(eventFrame("bretagne", 12.5));
      // Connexion maintenue ensuite : aucune reconnexion pendant la lecture.
    });
    const cf = new CarbonFr({ baseUrl, apiKey: "cle-de-test" });
    const { signal } = abortAfter(t);

    const events = await take(cf.stream({ region: "bretagne", below: 50, signal }), 2);

    assert.deepEqual(
      events.map((e) => [e.region, e.intensity, e.methodology]),
      [
        ["bretagne", 42, "rte-direct"],
        ["bretagne", 12.5, "rte-direct"],
      ],
    );
    const url = new URL(request.url, baseUrl);
    assert.equal(url.pathname, "/v1/intensity/stream");
    assert.equal(url.searchParams.get("region"), "bretagne");
    assert.equal(url.searchParams.get("below"), "50");
    assert.equal(request.headers.accept, "text/event-stream");
    assert.equal(request.headers.authorization, "Bearer cle-de-test");
  });

  test("défauts exportés : 45 s d'inactivité, backoff de 1 s plafonné à 30 s", () => {
    assert.equal(DEFAULT_IDLE_TIMEOUT_MS, 45_000);
    assert.equal(DEFAULT_INITIAL_BACKOFF_MS, 1_000);
    assert.equal(DEFAULT_MAX_BACKOFF_MS, 30_000);
  });
});

describe("reconnexion", () => {
  test("après fermeture du flux par le serveur, en silence", LIMIT, async (t) => {
    let attempts = 0;
    const baseUrl = await serve(t, (req, res) => {
      attempts += 1;
      openSse(res);
      if (attempts === 1) {
        res.end(eventFrame("national", 1)); // un événement, puis fin du flux
      } else {
        res.write(eventFrame("national", 2)); // événement différent, connexion maintenue
      }
    });
    const cf = new CarbonFr({ baseUrl });
    const { signal } = abortAfter(t);

    // Aucune erreur intermédiaire : la boucle reçoit directement l'événement
    // servi par la 2e connexion.
    const events = await take(cf.stream({ reconnect: FAST, signal }), 2);

    assert.deepEqual(
      events.map((e) => e.intensity),
      [1, 2],
    );
    assert.equal(attempts, 2);
  });

  test("après une erreur de transport, à la connexion puis en pleine lecture", LIMIT, async (t) => {
    let attempts = 0;
    const baseUrl = await serve(t, (req, res) => {
      attempts += 1;
      if (attempts === 1) {
        req.socket.destroy(); // avant toute réponse : `fetch` rejeté
        return;
      }
      openSse(res);
      res.write(eventFrame("national", attempts));
      if (attempts === 2) {
        const cut = setTimeout(() => req.socket.destroy(), 20); // lecture rejetée
        res.on("close", () => clearTimeout(cut));
      }
    });
    const cf = new CarbonFr({ baseUrl });
    const { signal } = abortAfter(t);

    const events = await take(cf.stream({ reconnect: FAST, signal }), 2);

    assert.deepEqual(
      events.map((e) => e.intensity),
      [2, 3],
    );
    assert.equal(attempts, 3);
  });

  test("après le délai d'inactivité : 200 sans aucune trame", LIMIT, async (t) => {
    let attempts = 0;
    const baseUrl = await serve(t, (req, res) => {
      attempts += 1;
      openSse(res); // en-têtes envoyés…
      if (attempts > 1) res.write(eventFrame("national", 7)); // … trame à la 2e connexion seulement
    });
    const cf = new CarbonFr({ baseUrl });
    const { signal } = abortAfter(t);
    const started = Date.now();

    const [event] = await take(cf.stream({ idleTimeoutMs: 150, reconnect: FAST, signal }), 1);

    assert.equal(event.intensity, 7);
    assert.equal(attempts, 2);
    assert.ok(Date.now() - started >= 100, "la 1re connexion a attendu le délai d'inactivité");
  });

  test("le délai d'inactivité borne aussi l'attente des en-têtes", LIMIT, async (t) => {
    let attempts = 0;
    const baseUrl = await serve(t, (req, res) => {
      attempts += 1;
      if (attempts === 1) return; // requête reçue, jamais de réponse : `fetch` en attente
      openSse(res);
      res.write(eventFrame("national", 8));
    });
    const cf = new CarbonFr({ baseUrl });
    const { signal } = abortAfter(t);

    const [event] = await take(cf.stream({ idleTimeoutMs: 150, reconnect: FAST, signal }), 1);

    assert.equal(event.intensity, 8);
    assert.equal(attempts, 2);
  });

  test("les keep-alive réarment le délai d'inactivité : aucune reconnexion", LIMIT, async (t) => {
    let attempts = 0;
    const baseUrl = await serve(t, (req, res) => {
      attempts += 1;
      openSse(res);
      // Un keep-alive toutes les 20 ms pendant 500 ms (2,5× le délai de
      // 200 ms), puis un événement.
      const heartbeat = setInterval(() => res.write(KEEP_ALIVE), 20);
      const event = setTimeout(() => {
        clearInterval(heartbeat);
        res.write(eventFrame("national", 9));
      }, 500);
      res.on("close", () => {
        clearInterval(heartbeat);
        clearTimeout(event);
      });
    });
    const cf = new CarbonFr({ baseUrl });
    const { signal } = abortAfter(t);

    const [event] = await take(cf.stream({ idleTimeoutMs: 200, reconnect: FAST, signal }), 1);

    assert.equal(event.intensity, 9);
    assert.equal(attempts, 1);
  });
});

describe("réponses HTTP à la (re)connexion", () => {
  const transient = [
    [408, "timeout"],
    [429, "rate_limited"],
    [500, "internal"],
    [503, "unavailable"], // plafond de connexions SSE du serveur
  ];
  for (const [status, code] of transient) {
    test(`${status} puis 200 : rejoue avec backoff`, LIMIT, async (t) => {
      let attempts = 0;
      const baseUrl = await serve(t, (req, res) => {
        attempts += 1;
        if (attempts === 1) {
          // `Retry-After` n'est pas lu : le backoff (10 ms ici) s'applique.
          problem(res, status, code, "réessayer plus tard", { "retry-after": "30" });
          return;
        }
        openSse(res);
        res.write(eventFrame("national", status));
      });
      const cf = new CarbonFr({ baseUrl });
      const { signal } = abortAfter(t);

      const [event] = await take(cf.stream({ reconnect: FAST, signal }), 1);

      assert.equal(event.intensity, status);
      assert.equal(attempts, 2);
    });
  }

  const fatal = [
    [400, "bad_request"],
    [401, "unauthorized"],
    [403, "forbidden"],
    [404, "not_found"],
  ];
  for (const [status, code] of fatal) {
    test(`${status} : lève une CarbonFrError sans rejouer`, LIMIT, async (t) => {
      let attempts = 0;
      const baseUrl = await serve(t, (req, res) => {
        attempts += 1;
        problem(res, status, code, `refus ${status}`);
      });
      const cf = new CarbonFr({ baseUrl });
      const { signal } = abortAfter(t);

      await assert.rejects(take(cf.stream({ reconnect: FAST, signal }), 1), (err) => {
        assert.ok(err instanceof CarbonFrError);
        assert.equal(err.status, status);
        assert.equal(err.code, code);
        assert.equal(err.message, `refus ${status}`);
        return true;
      });
      await delay(50); // plusieurs backoffs de 10 ms : toujours aucune nouvelle tentative
      assert.equal(attempts, 1);
    });
  }
});

describe("arrêt par le signal", () => {
  test("pendant une attente de backoff : fin rapide, sans erreur", LIMIT, async (t) => {
    let attempts = 0;
    const baseUrl = await serve(t, (req, res) => {
      attempts += 1;
      problem(res, 503, "unavailable", "plafond atteint", { "retry-after": "30" });
    });
    const cf = new CarbonFr({ baseUrl });
    const ac = abortAfter(t);
    const it = cf.stream({
      signal: ac.signal,
      reconnect: { initialBackoffMs: 60_000, maxBackoffMs: 60_000 },
    });

    const pending = it.next();
    await until(() => attempts === 1);
    await delay(50); // 503 traité : le générateur attend son backoff de 60 s
    const abortedAt = Date.now();
    ac.abort();

    assert.deepEqual(await pending, { done: true, value: undefined });
    assert.ok(Date.now() - abortedAt < 1_000, "fin sans attendre le backoff");
    assert.equal(attempts, 1);
    // Le timer du backoff est annulé : sinon ce fichier de test ne se
    // terminerait qu'au bout des 60 s.
  });

  test("pendant une lecture bloquée : fin rapide, sans erreur, connexion fermée", LIMIT, async (t) => {
    const closed = signalled();
    const baseUrl = await serve(t, (req, res) => {
      openSse(res);
      res.write(eventFrame("national", 5)); // puis plus rien
      res.on("close", closed.resolve);
    });
    const cf = new CarbonFr({ baseUrl });
    const ac = abortAfter(t);
    const it = cf.stream({ signal: ac.signal, idleTimeoutMs: 60_000, reconnect: FAST });

    assert.equal((await it.next()).value.intensity, 5);
    const pending = it.next(); // aucune donnée : lecture en attente
    await delay(50);
    const abortedAt = Date.now();
    ac.abort();

    assert.deepEqual(await pending, { done: true, value: undefined });
    assert.ok(Date.now() - abortedAt < 1_000, "fin sans attendre le délai d'inactivité");
    await closed.promise; // le serveur voit la déconnexion (place SSE rendue)
  });

  test("pendant le traitement d'un événement : connexion coupée aussitôt, fin au next() suivant", LIMIT, async (t) => {
    const closed = signalled();
    const baseUrl = await serve(t, (req, res) => {
      openSse(res);
      res.write(eventFrame("national", 4));
      res.on("close", closed.resolve);
    });
    const cf = new CarbonFr({ baseUrl });
    const ac = abortAfter(t);
    const it = cf.stream({ signal: ac.signal, reconnect: FAST });

    assert.equal((await it.next()).value.intensity, 4);
    ac.abort(); // l'appelant traite encore l'événement : aucun next() en cours
    await closed.promise; // place SSE rendue sans attendre le next() suivant

    assert.deepEqual(await it.next(), { done: true, value: undefined });
  });

  test("pendant la connexion (en-têtes en attente) : fin rapide, sans erreur", LIMIT, async (t) => {
    let attempts = 0;
    const closed = signalled();
    const baseUrl = await serve(t, (req, res) => {
      attempts += 1;
      res.on("close", closed.resolve); // jamais de réponse
    });
    const cf = new CarbonFr({ baseUrl });
    const ac = abortAfter(t);
    const it = cf.stream({ signal: ac.signal, idleTimeoutMs: 60_000, reconnect: FAST });

    const pending = it.next();
    await until(() => attempts === 1);
    ac.abort();

    assert.deepEqual(await pending, { done: true, value: undefined });
    await closed.promise;
    assert.equal(attempts, 1);
  });

  test("signal déjà aborté : aucune requête", LIMIT, async (t) => {
    let attempts = 0;
    const baseUrl = await serve(t, (req, res) => {
      attempts += 1;
      openSse(res);
    });
    const cf = new CarbonFr({ baseUrl });

    const events = await take(cf.stream({ signal: AbortSignal.abort() }), 1);

    assert.deepEqual(events, []);
    assert.equal(attempts, 0);
  });

  test("sortir de la boucle (break) ferme la connexion", LIMIT, async (t) => {
    const closed = signalled();
    const baseUrl = await serve(t, (req, res) => {
      openSse(res);
      res.write(eventFrame("national", 6));
      res.on("close", closed.resolve);
    });
    const cf = new CarbonFr({ baseUrl });
    const { signal } = abortAfter(t);

    for await (const event of cf.stream({ reconnect: FAST, signal })) {
      assert.equal(event.intensity, 6);
      break;
    }

    await closed.promise;
  });
});

describe("reconnexion désactivée (reconnect: false)", () => {
  test("la fermeture du flux termine le générateur, sans erreur ni reconnexion", LIMIT, async (t) => {
    let attempts = 0;
    const baseUrl = await serve(t, (req, res) => {
      attempts += 1;
      openSse(res);
      res.end(eventFrame("national", 3));
    });
    const cf = new CarbonFr({ baseUrl });
    const { signal } = abortAfter(t);

    const events = await take(cf.stream({ reconnect: false, signal }), 10);

    assert.deepEqual(
      events.map((e) => e.intensity),
      [3],
    );
    assert.equal(attempts, 1);
  });

  test("une réponse 503 est levée sans rejouer", LIMIT, async (t) => {
    let attempts = 0;
    const baseUrl = await serve(t, (req, res) => {
      attempts += 1;
      problem(res, 503, "unavailable", "plafond atteint", { "retry-after": "30" });
    });
    const cf = new CarbonFr({ baseUrl });
    const { signal } = abortAfter(t);

    await assert.rejects(take(cf.stream({ reconnect: false, signal }), 1), (err) => {
      assert.ok(err instanceof CarbonFrError);
      assert.equal(err.status, 503);
      assert.equal(err.code, "unavailable");
      return true;
    });
    assert.equal(attempts, 1);
  });

  test("une erreur de transport est levée", LIMIT, async (t) => {
    let attempts = 0;
    const baseUrl = await serve(t, (req) => {
      attempts += 1;
      req.socket.destroy();
    });
    const cf = new CarbonFr({ baseUrl });
    const { signal } = abortAfter(t);

    await assert.rejects(take(cf.stream({ reconnect: false, signal }), 1), (err) => {
      assert.ok(err instanceof Error);
      assert.ok(!(err instanceof CarbonFrError), "erreur de transport, pas une erreur HTTP");
      return true;
    });
    assert.equal(attempts, 1);
  });

  test("le délai d'inactivité lève une DOMException TimeoutError", LIMIT, async (t) => {
    const baseUrl = await serve(t, (req, res) => openSse(res)); // aucune trame
    const cf = new CarbonFr({ baseUrl });
    const { signal } = abortAfter(t);

    await assert.rejects(take(cf.stream({ reconnect: false, idleTimeoutMs: 100, signal }), 1), (err) => {
      assert.ok(err instanceof DOMException);
      assert.equal(err.name, "TimeoutError");
      return true;
    });
  });
});

describe("options", () => {
  const invalid = [
    ["idleTimeoutMs: 0", { idleTimeoutMs: 0 }],
    ["idleTimeoutMs: NaN", { idleTimeoutMs: Number.NaN }],
    ["initialBackoffMs négatif", { reconnect: { initialBackoffMs: -1 } }],
    ["maxBackoffMs: 0", { reconnect: { maxBackoffMs: 0 } }],
  ];
  for (const [name, opts] of invalid) {
    test(`${name} : RangeError avant toute requête`, LIMIT, async (t) => {
      let attempts = 0;
      const baseUrl = await serve(t, (req, res) => {
        attempts += 1;
        openSse(res);
      });
      const cf = new CarbonFr({ baseUrl });
      const { signal } = abortAfter(t);

      await assert.rejects(take(cf.stream({ ...opts, signal }), 1), RangeError);
      assert.equal(attempts, 0);
    });
  }
});
