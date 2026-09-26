# ADR-0014 — Usage : primitives carbon-aware + livraison live par SSE

- **Statut** : Accepté (mis en œuvre — primitives de scheduling **et** SSE livrés ; webhooks reportés)
- **Date** : 2026-06-15
- **S'appuie sur** : ADR-0011 (contrat `ForecastPoint`, sélecteur `expected`/`upper`) ; ADR-0004 (Postgres natif) ; ADR-0007 (déploiement / tier hébergé)

## État d'implémentation (2026-06-15)

**Tranche A — primitives de scheduling (§1) : livrée.** Fonctions **pures** du
domaine (`crates/core/src/domain/schedule.rs`), zéro nouveau port, réutilisant le
sélecteur `central`/`prudent` :

- `greenest_window_before` — créneau contigu le plus bas-carbone **avant une
  échéance** (généralise `greenest_window`) ;
- `lowest_slots` — les `k` créneaux les moins intenses (job **divisible**,
  interruptibilité parfaite supposée — hypothèse documentée) ;
- `slots_below` — tous les créneaux sous un seuil d'intensité ;
- `savings_vs_now` / `Savings` — Δ vs « maintenant » : delta + %, et économie
  **absolue** (gCO₂eq) si l'énergie du job (`kWh`) est fournie.

Cas d'usage `CarbonAwareScheduler` (façade sur `ForecastModel`) + endpoints
`/v1` : **`GET /v1/schedule`** (créneau sous échéance + économie),
**`GET /v1/schedule/slots`** (lowest-k), **`GET /v1/intensity/below`** (seuil).
OpenAPI + collection Bruno à jour. Posture **anonyme/sans état** préservée.

**Tranche B — livraison live SSE (§2) : livrée.** `GET /v1/intensity/stream`
(`text/event-stream`) : le client ouvre la connexion, le serveur pousse un
événement `intensity` à chaque mise à jour nationale du read-model (cadence du
poller). Filtres optionnels `region` et `below=X`. Heartbeat keep-alive. **Sans
état par-client**, anonyme.

**Choix de mécanisme (question ouverte §73 tranchée) : canal mémoire
`tokio::broadcast`.** Le poller est **intégré** au même process que l'API ; il
publie chaque mise à jour sur un canal de diffusion, les connexions SSE s'y
abonnent (un seul producteur, fan-out à N abonnés). Pour un **`bin/poller`
séparé** (ADR-0007), basculer la source du canal sur Postgres `LISTEN`/`NOTIFY`
(le poller `NOTIFY`, une tâche de l'API `LISTEN` et réinjecte dans le même canal
de fan-out) — **l'abonnement SSE et les filtres restent identiques**. La frontière
est posée pour rendre ce remplacement local.

Webhooks toujours **reportés** (§3, gated ADR-0015).

## Contexte

Les prévisions (ADR-0011/0012/0013) et `greenest_window` existent. L'axe « usage » transforme cette prévision en **décisions** (scheduling) et la **livre en continu** (streaming). Jusqu'ici, tout est **pull** : le client interroge un read-model **anonyme, sans état, auto-hébergeable**.

Deux préoccupations très différentes sous le même mot :

- le **scheduling carbon-aware** est du *calcul pur* sur la prévision — léger, dans la continuité directe ;
- les **notifications en push** font entrer, avec les webhooks, de l'**état utilisateur**, de l'**auth**, du **SSRF**, de l'**abus** et de la **fiabilité de livraison** — un sous-système lourd qui dépend de la décision « tier hébergé » (ADR-0007) — **depuis tranchée par l'ADR-0015**.

Cadrage retenu : **quatre primitives** de scheduling ; livraison live par **SSE** ; **webhooks reportés**.

## Décision

### 1. Scheduling carbon-aware = primitives **pures du domaine** + cas d'usage. **Aucun nouveau port.**

Toutes consomment `ForecastModel` et opèrent sur `Vec<ForecastPoint>`, dans `core` (fonctions) + `application` (cas d'usage), testées avec des fakes. Elles réutilisent le **sélecteur `expected`/`upper`** (ADR-0011) → un scheduling « optimiste » ou « prudent ».

- **Contraint par échéance** : généralise `greenest_window` en le bornant à `[from, deadline − duration]`. (On peut faire de `greenest_window` une fonction prenant une échéance optionnelle.)
- **Divisible / lowest-k** : `k` quarts d'heure (pas forcément contigus) avant `deadline` → les `k` créneaux les moins intenses. Algorithme distinct du créneau contigu. **Hypothèse à documenter** : interruptibilité parfaite (créneaux indépendants).
- **Requête par seuil** : tous les créneaux sous `X` gCO₂/kWh dans l'horizon.
- **Annotation d'économie** : Δ vs « maintenant ». Avec une énergie de job (`kWh`) → gCO₂ **absolus** ; sans elle → delta d'intensité + %. C'est ce qui rend l'API **actionnable**, pas seulement informative.

Surface API (sous `/v1`, forme exacte = question ouverte) : un `/v1/schedule` regroupant les cas orientés job (échéance / divisible / économie), et une liste par seuil. `/v1/greenest-window` reste le cas simple.

**Deux limites assumées, affichées et non masquées :**
- ce sont des **conseils sur prévision, pas du pilotage** (non-objectif « pas un outil de contrôle réseau » préservé — c'est l'usage phare de carbonintensity.org.uk) ;
- on sert l'intensité **moyenne** (estimation RTE), **pas marginale**. Le marginal serait une *méthode* à part entière (façon `acv-ademe`), pas une primitive de scheduling.

### 2. Livraison live par **SSE**, client-initié, sans état

`GET /v1/intensity/stream` (`text/event-stream`) : le **client ouvre** la connexion, le serveur pousse un événement à chaque mise à jour du read-model (cadence du poller : 15 min national, horaire régional). Filtres optionnels (`region`, `below=X`) pour des événements du type « créneau vert imminent ».

- C'est une préoccupation d'**adapter entrant** (`adapter-http`) lisant le read-model. **Aucun nouveau domaine.**
- Comme le client initie : **pas d'URL stockée → pas de SSRF, pas d'amplification, état minimal** (les connexions ouvertes).
- **Déc+ouplage poller → API** : Postgres **`LISTEN`/`NOTIFY`** (le poller `NOTIFY`, l'API `LISTEN`) — natif, souverain, cohérent ADR-0004, et compatible avec un `bin/poller` séparé (ADR-0007).
- **Auto-hébergeable et anonyme** : SSE ne casse pas la posture.

### 3. Webhooks **reportés**, gated sur la décision « tier hébergé »

Les webhooks (action **sortante initiée par le serveur** vers des URL fournies) forcent : **état par-utilisateur** (abonnements), **ownership/auth**, **filtrage SSRF**, **anti-amplification** (rate-limit + vérification d'endpoint), **fiabilité** (retries, backoff, dead-letter, signatures HMAC). Tout cela dépend de **qui possède un abonnement** — donc de l'existence d'un tier hébergé avec comptes (ADR-0007 ; **tier décidé depuis par l'ADR-0015**).

On **reporte** sciemment. Le tier étant désormais tranché (ADR-0015, qui **lève ce blocage** en fournissant le propriétaire d'abonnement), un ADR dédié spécifiera les webhooks (port `SubscriptionRepository`, port sortant `Notifier`, cas d'usage *watcher*, HMAC, deny-list SSRF, quotas).

### 4. Posture : v1 reste **sans état et anonyme**

Aucun compte, aucun stockage par-utilisateur. C'est une **décision**, et un atout (souveraineté, auto-hébergement).

## Conséquences

- **Domaine** : ajout des fonctions de scheduling (échéance, lowest-k, seuil, économie) — toutes **pures**, sans nouveau port. `greenest_window` éventuellement généralisé (échéance optionnelle).
- **Infra** : SSE dans `adapter-http` + mécanisme `LISTEN`/`NOTIFY` poller→API. **Aucun adapter d'action sortante** (c'est ce qu'on évite en ne faisant pas de webhooks).
- **Posture préservée** : stateless, anonyme, auto-hébergeable. **Aucune surface SSRF/abus.**
- **Surface API** : `/v1/schedule`, liste par seuil, `/v1/intensity/stream` — à versionner sous `/v1`.
- **Reporté & tracé** : webhooks → ADR futur, **gated sur le tier hébergé**. Déféré, pas oublié.
- **Coût** : gestion des connexions SSE (timeout, heartbeat, nb max, backpressure) ; l'économie absolue exige l'énergie du job en entrée (sinon relative).

## Alternatives envisagées

- **Webhooks dès le v1** : forcent état/auth/SSRF/abus et dépendent d'une décision non prise. Reportés, pas abandonnés.
- **Polling seul (pas de stream)** : le plus simple, mais un dashboard live martèle alors `/now` ; SSE est la réponse standard et efficace, et reste client-initiée. Les primitives pull restent disponibles de toute façon.
- **WebSockets plutôt que SSE** : bidirectionnel, plus lourd ; le besoin est un flux **unidirectionnel** serveur→client → SSE est le bon choix, plus léger et ami des proxys. Écarté (surdimensionné).
- **Scheduling sur intensité marginale** : hors périmètre — le marginal est une *méthode*, pas une primitive ; affiché comme limite connue.

## Questions ouvertes (implémentation — n'impactent pas le principe)

- REST exact : params de `/v1/schedule` vs endpoints séparés ; forme de la liste par seuil.
- Mécanisme interne de notification poller→API (`LISTEN`/`NOTIFY` vs canal mémoire) selon la forme du poller (intégré vs `bin/poller`, ADR-0007).
- Limites SSE : timeout, nombre max de connexions, *heartbeat* — tranchée, cf. addendum 2026-09-26.
- Contrat de `greenest_slots` : l'hypothèse d'interruptibilité parfaite doit être explicite.

## Addendum (2026-09-26) — limites SSE : plafond de connexions, timeouts, heartbeat

Clôt la question ouverte « Limites SSE : timeout, nombre max de connexions,
*heartbeat* » ci-dessus (item I8 du plan, SEC-1/PERF-2/SEC-4).

### Décision

**Plafond global de connexions, pas de durée de vie de flux.** Un
`tokio::sync::Semaphore` (type public `SseLimiter`, `carbonfr-adapter-http`)
porté par `StreamState` : le handler de `GET /v1/intensity/stream` tente
`try_acquire_owned()` **après** la validation des paramètres (un 400 ne
consomme rien) et **avant** l'abonnement au canal `broadcast`. Défaut **300**
connexions (`CARBONFR_SSE_MAX_CONNECTIONS`). Au-delà : `503`
`application/problem+json`, code stable `unavailable` (réutilisé, pas de
nouveau code), en-tête `Retry-After: 30`, documenté dans l'OpenAPI (réponse
503 ajoutée sur ce seul chemin, snapshot régénéré).

Le permis est **tenu par le flux lui-même** (combinateur `hold_permit` : il
vit dans le stream de la réponse), avec deux régimes de libération : (1)
fermeture propre par le client — FIN TCP détectée quasi immédiatement par
hyper via lecture concurrente — ou arrêt gracieux du serveur (le
`take_until(shutdown)` existant termine le flux) ; (2) client disparu **sans**
FIN (coupure réseau, veille, NAT qui expire) — le keep-alive SSE (~15 s,
`KeepAlive::default()` d'axum, inchangé) garantit une écriture régulière, mais
son échec n'est constaté qu'à l'épuisement des retransmissions TCP du noyau,
soit **≈ 15 min** avec `tcp_retries2 = 15` (défaut Linux) : aucune option
`SO_KEEPALIVE`/`TCP_USER_TIMEOUT` n'est posée sur les sockets acceptées (le
keepalive TCP n'agirait de toute façon pas tant que des octets restent non
acquittés). Un « permis fantôme » est donc compté jusqu'à un quart d'heure —
même ordre de grandeur que pour tout serveur SSE derrière un proxy Go. Plafond
**global au processus**, pas par IP ni par clé — le plan demandait un seuil
généreux ; le dimensionner en tenant compte de ces fantômes et resserrer
seulement métriques à l'appui (cf. Observabilité).

**Pas de durée de vie maximale forcée d'un flux SSE**, décision explicite :
elle couperait le SDK TypeScript (aucune reconnexion à ce jour, item DX-2
d'I9) en silence, sans bénéfice de sécurité supplémentaire — seule la
*concurrence* est plafonnée, pas la durée. Le *heartbeat* (part de cette
même question ouverte) était déjà couvert par `Sse::keep_alive(KeepAlive::default())`
(~15 s) et reste inchangé.

Les timeouts de requête et de lecture d'en-têtes livrés dans la même PR
(ADR-0014 non concerné directement, cf. `CHANGELOG.md` et
`deploy/README.md` §1/§2) **n'affectent jamais un flux SSE déjà ouvert** : le
timeout de requête borne le futur qui construit la réponse (résolu dès
`Sse::new(...)`, pas le corps du flux) ; le timeout d'en-têtes hyper ne
s'arme qu'entre deux requêtes d'une connexion, jamais pendant l'écriture
d'un corps de réponse.

### Observabilité

Métriques Prometheus (`bin/server/src/metrics.rs`, miroir du motif
`QuotaGauge`/`render_odre_quota` : `SseGauge`/`render_sse`) :
`carbonfr_sse_connections_active` (gauge), `carbonfr_sse_connections_max`
(gauge, = le plafond configuré) et `carbonfr_sse_connections_rejected_total`
(counter). Deux alertes dans `deploy/prometheus/alerts.yml` :
`CarbonfrSseNearCap` (> 80 % du plafond pendant 10 min, `warning`) et
`CarbonfrSseRejected` (au moins un refus en 15 min, `warning`) — la première
sert à **relever le plafond avant** qu'un client légitime reçoive un 503
(mesurer avant de resserrer, même principe que le quota ODRÉ de l'addendum
ADR-0022 2026-09-26).

### Comportements clients (vérifiés, sans changement de SDK dans cette PR)

- **`EventSource` navigateur natif** : ne reconnecte **jamais** sur un statut
  non-200 à la connexion (spec WHATWG : `readyState` passe à `CLOSED`) — un
  503 de plafond est une fin définitive pour un dashboard `EventSource` nu,
  sauf reconnexion applicative.
- **SDK Rust** (`carbonfr-sdk`) : reconnexion active par défaut (backoff
  fixe puis exponentiel, 1 → 30 s, tentatives illimitées) → un 503 à la
  connexion est **absorbé silencieusement** dans le backoff, jamais remonté
  comme erreur tant que la reconnexion est active ; avec
  `Reconnect::Disabled`, `CarbonFrError::Api { status: 503 }` sort
  immédiatement. Le SDK ne lit pas `Retry-After` (son palier de 30 s
  coïncide avec la valeur envoyée).
- **SDK TypeScript** (`@carbon-fr/sdk`) : jette une `CarbonFrError` (503,
  `code: "unavailable"`) et ne reconnecte pas — la reconnexion automatique
  est l'item DX-2 de l'itération I9 du plan, pas encore livrée.

### Pistes non retenues

- **Sous-quota par IP ou par clé API** : écarté pour cette PR — à envisager
  seulement si `carbonfr_sse_connections_rejected_total` montre un abus
  (une source concentrant les refus), pas par anticipation.
- **`TCP_USER_TIMEOUT` sur les sockets acceptées** (bornerait la détection
  d'un client disparu sans FIN à quelques dizaines de secondes au lieu de
  ~15 min) : option noyau Linux non testable hermétiquement, écartée pour
  l'instant ; à envisager si `carbonfr_sse_connections_active` reste durablement
  au-dessus du nombre de clients réels (permis fantômes visibles).
