# ADR-0014 — Usage : primitives carbon-aware + livraison live par SSE

- **Statut** : Proposé
- **Date** : 2026-06-15
- **S'appuie sur** : ADR-0011 (contrat `ForecastPoint`, sélecteur `expected`/`upper`) ; ADR-0004 (Postgres natif) ; ADR-0007 (déploiement / tier hébergé)

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
- Limites SSE : timeout, nombre max de connexions, *heartbeat*.
- Contrat de `greenest_slots` : l'hypothèse d'interruptibilité parfaite doit être explicite.
