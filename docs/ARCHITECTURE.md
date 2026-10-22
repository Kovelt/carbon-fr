# Architecture — `carbon-fr`

Ce document décrit la conception du système : la vision, les contraintes, le découpage hexagonal, le modèle de données et la roadmap. Les décisions ponctuelles et leurs alternatives sont tracées dans les [ADR](adr/).

## 1. Vision & contraintes

`carbon-fr` rend la donnée carbone du réseau électrique français **directement consommable par des développeurs et des machines**, là où la source officielle reste brute et plafonnée.

Contraintes structurantes :

- **Souveraineté** : aucune dépendance propriétaire obligatoire, auto-hébergeable, licences OSI.
- **Dev-first** : API lisible, index humainement interprétable, SDK, OpenAPI.
- **Résilience au quota** : la source impose 50 000 appels/mois par jeu de données et par client ; l'architecture ne doit pas exposer ce plafond aux clients.
- **Évolutivité méthodologique** : la façon de calculer le carbone va évoluer ; elle doit être isolée et versionnée.

### Non-objectifs

`carbon-fr` n'est **pas** :

- un outil de **pilotage réseau temps réel** ou de sûreté du système électrique ;
- un outil de **facturation, de comptage commercial ou de trading** de MWh ;
- une source couvrant les **territoires non métropolitains** (DOM-TOM) au lancement ;
- un **substitut à la source officielle** RTE : il la re-traite et la rend consommable, sans s'y substituer.

## 2. Le constat clé : la prévision n'existe pas dans la source

C'est le point le plus important du projet.

RTE/ODRÉ publie une **estimation des émissions** au pas quart d'heure, mais uniquement pour le passé et le présent. Les seules **prévisions** présentes dans le jeu de données temps réel sont des prévisions de **consommation** (J-1 et J), *pas* d'intensité carbone.

Conséquence sur le découpage de la valeur :

| Endpoint | Nature | Difficulté |
| --- | --- | --- |
| `/intensity/now`, `/mix`, `/intensity/date`, `/intensity/stats` | Repackaging propre de données existantes | Faible |
| `/exchanges`, `/weather`, `/factors`, `/methodologies`, `/eligibility/rulesets` | Donnée externe ou catalogue, exposés tels quels | Faible |
| `/renewable`, `/price`, `/cost-reference`, carte `/hydrogene` (hors `/v1`) | Donnée dérivée/composée (modèle physique, décomposition TRV, fourchettes LCOE, carte électrolyseurs × carbone live) | Moyenne |
| `/intensity/forecast`, `/intensity/greenest-window` (+ overlay `?eligibility=`), `/schedule*`, `/intensity/below`, `/intensity/stream` | **Valeur créée** : il faut modéliser (prévision) puis l'outiller (carbon-aware, live, éligibilité électrolyseur) | Élevée |

La prévision est donc une **responsabilité du service**, pas un simple proxy. Le modèle est branché derrière un port (`ForecastModel`) : on a démarré avec un modèle statistique (`climatology@1`) et un modèle ML (`gbdt@1`) a été exploré derrière le même port sans toucher au reste — il n'est pas servi car il ne bat pas la climatologie au backtest. À titre de référence, le modèle britannique repose sur du machine learning et de la modélisation réseau, avec une prévision à 96 h et plus.

## 3. Quota, ingestion & cache

Le quota ODRÉ (50 000 appels/mois **par jeu de données et par client**, lu dans les en-têtes `x-ratelimit-dataset-*`) est un faux problème **à une condition** : un seul composant tape la source.

- Le national temps réel se rafraîchit toutes les 15 min → ~96 récupérations/jour.
- Le régional est ingéré **au même cycle que le national** (`CARBONFR_POLL_SECS`, défaut 15 min), à raison d'**un appel ODRÉ par région** (12 appels/cycle, tous sur le jeu `eco2mix-regional-tr`). Le quota réel de chaque jeu est lu dans les en-têtes de réponse d'ODRÉ et exposé en quatre jauges `carbonfr_odre_quota_{limit,remaining,reset_timestamp_seconds,observed_timestamp_seconds}{dataset}` avec deux alertes (ADR-0022, addendum 2026-09-26) ; `carbonfr_upstream_requests_total{source="odre"}` reste le proxy historique des appels initiés par le poller (14 par cycle, tous jeux confondus — les exports de l'auto-réparation ne s'y comptent pas).
- Chaque cycle relit une **fenêtre glissante** des dernières heures (`IngestRecent`, `CARBONFR_POLL_WINDOW_HOURS`, 3 h par défaut) — toujours un seul appel `range()` par zone, donc le même budget — et une **auto-réparation quotidienne** réimporte les `CARBONFR_SELF_HEAL_DAYS` derniers jours par un export de masse du jeu temps réel (national ; régional aussi si `CARBONFR_SELF_HEAL_REGIONAL=1`), soit +1 appel par jour et par jeu (ADR-0003, addenda 2026-09-25 et 2026-09-26).
- Un **poller unique** (singleton) alimente la base ; l'API sert ensuite **tous** les clients depuis la base. Ce poller est **intégré** au `server` (§9) et le déploiement est **mono-instance** : il n'existe pas d'élection de leader — plusieurs instances lanceraient chacune leur poller et multiplieraient les appels ; l'évolution documentée pour plusieurs instances d'API est un `bin/poller` séparé (§9).

Budget indicatif : 14 appels ODRÉ par cycle (1 national + 12 régions + 1 charge) × 96 cycles/jour ≈ **~1 350 appels/jour**, répartis sur deux jeux aux quotas distincts : **~1 150/jour sur `eco2mix-regional-tr`, soit ≈ 70 % de ses 50 000 appels/mois** — le poste à surveiller —, et ~190/jour sur `eco2mix-national-tr` (≈ 12 %), plus un export par jour et par jeu pour l'auto-réparation ; suivi par les jauges `carbonfr_odre_quota_remaining{dataset}` (ADR-0022) ; `CARBONFR_POLL_SECS` permet de desserrer la cadence si besoin. Le plafond ne concerne que les intégrateurs qui taperaient RTE en direct, précisément ce que `carbon-fr` leur évite. C'est aussi ce qui justifie l'existence du service par rapport à « tape l'open data toi-même ».

> La base joue le rôle de **read-model** (le « cache »). Des couches de cache mémoire s'y ajoutent depuis l'audit perf 2026-08 : cache TTL des séries prévues (`CachedForecaster`, TTL = intervalle de poll), en-tête `Cache-Control: public, max-age=60` sur les lectures stables, et caches positif/négatif de résolution des clés API.

**Backfill historique** : le rapatriement de l'historique (2012→ au national, 2013→ au régional, cf. §5) ne passe **pas** par l'API paginée — cela brûlerait le quota — mais par l'**export en masse** du jeu de données ODRÉ, par tranches (`CARBONFR_BACKFILL_WINDOW_DAYS`, 90 jours par défaut ; par tranche, deux exports au national — mix/intensité puis charge réalisée — et un seul au régional, pour les 12 régions d'un coup), rejouable sur une période (`CARBONFR_BACKFILL_SOURCE=realtime` pour un trou récent pas encore consolidé, `CARBONFR_BACKFILL_SCOPE=regional` ou `all` pour les 12 régions) ; l'entretien courant revient ensuite au poller (fenêtre glissante) et à l'auto-réparation quotidienne, qui ne lisent que les jeux temps réel : la montée au millésime consolidé puis définitif (ADR-0006) reste un `backfill` à rejouer sur les mois que RTE a consolidés depuis (`deploy/README.md` §5).

## 4. Architecture hexagonale (ports & adapters)

```
              Adapters entrants                 Adapters sortants
              (driving)                          (driven)
        ┌─────────────────────┐          ┌────────────────────────────┐
        │  API HTTP (axum, /v1)│          │  OdreClient  → Eco2mixSource│
        │  + SSE + webhooks    │          │  PgRepository→ IntensityRepo│
        └──────────┬──────────┘          │  Climatology→ ForecastModel │
                   │                      └──────────────┬─────────────┘
                   │ appelle les cas d'usage             │ implémentent les ports
                   ▼                                     ▼
        ┌───────────────────────────────────────────────────────────┐
        │                        core (lib pure)                     │
        │   application/  cas d'usage (ports entrants)               │
        │   ports/        traits sortants (Eco2mixSource, …)         │
        │   domain/       CarbonIntensity, GenerationMix,            │
        │                 Region, Measurement, ForecastPoint,        │
        │                 Methodology, greenest_window()             │
        │                  ── aucune IO, aucune dépendance infra ──   │
        └───────────────────────────────────────────────────────────┘
                   ▲
                   │ assemble tout
        ┌──────────┴──────────┐
        │  bin/server         │  ← composition root
        │  (le seul à connaître les implémentations concrètes)
        └─────────────────────┘
```

**Ports sortants** (le domaine *demande*, l'infra *fournit*) :

- `Eco2mixSource` / `Eco2mixArchive` — récupérer la donnée RTE (dernier point, plage ; export de masse pour le backfill).
- `IntensityRepository` — lire/écrire les mesures (chaud + historique ; upsert conditionnel au millésime, rollups ; dernière mesure d'une région ou de **toutes** en un appel, `latest_all`).
- `ForecastModel` — produire une prévision (rend des `ForecastPoint` avec intervalle).
- `ConsumptionRepository` / `ConsumptionSource` — charge réalisée/prévue (entrée du futur ML).
- `WeatherRepository` / `WeatherForecastSource` — météo (vent 100 m + irradiance, Open-Meteo).
- `CrossBorderSource` / `CrossBorderRepository` — flux transfrontaliers + intensité du voisin (ENTSO-E, pour `acv-ademe@2` et `/exchanges`).
- `SpotPriceSource` / `SpotPriceRepository` — prix spot day-ahead (ENTSO-E A44, pour `/price`).
- `ApiKeyRepository` — résolution des clés API (port **de bord** : consommé par le middleware, jamais par un cas d'usage).
- `SubscriptionRepository` / `Notifier` — abonnements webhooks + livraison signée.
- `VisitCounter` — compteur de visiteurs (IP jamais stockée).
- `Clock` — fournir l'instant courant (testabilité).

**Ports entrants** (cas d'usage exposés, génériques sur leurs ports) : `GetCurrentIntensity`, `GetIntensityHistory`, `GetIntensityStats`, `IngestRecent` (le poller : fenêtre glissante des dernières heures, `CARBONFR_POLL_WINDOW_HOURS`, ADR-0003 addendum 2026-09-25 ; `IngestLatest` est conservé pour la compatibilité SemVer mais n'est plus appelé), `BackfillHistory`, `FindGreenestWindow` (conservé et testé dans le `core`, mais **plus appelé** par `/intensity/greenest-window` : le handler calcule le créneau via `domain::greenest_window` sur la prévision **unique** partagée avec l'overlay d'éligibilité, ADR-0026 D16), `CarbonAwareScheduler` (planification carbon-aware), `GetConsumptionIntensity` (`acv-ademe@2` à la lecture), `GetCrossBorderExchanges` (`/exchanges`), `GetElectricityPrice` (`/price`), `GetWeather`, `CalibrateRenewable`, `AnalyzeRenewableSignal`, plus les backtests (`BacktestForecast`, `BacktestConsumptionForecast`, `BacktestRenewable`).

**Pourquoi ce pattern ici précisément** :

1. *Risque source* (RTE change, quota, panne) → ajouter un adapter de secours (ENTSO-E, voire Electricity Maps) derrière le même `Eco2mixSource`, sans toucher au domaine.
2. *Roadmap incrémentale* → la phase 3 (prévision) n'est qu'un nouvel adapter derrière `ForecastModel`. La séparation « données repackagées » / « valeur créée » est matérialisée dans le code.
3. *Testabilité* → le `core` se teste avec des fakes en mémoire, sans base ni réseau.

## 5. Modèle de données

Unité d'observation : une **mesure** = `{ horodatage, région, intensité (gCO₂eq/kWh), méthodologie (identifiant + version), millésime, mix de production (optionnel) }`, au pas quart d'heure en temps réel (30 min pour l'historique consolidé/définitif). Les champs **méthodologie** (voir §6 et ADR-0005) et **millésime** (voir ci-dessous et ADR-0006) sont portés dès le départ.

**Clé d'unicité** : `(région, horodatage, méthodologie)`. La méthodologie fait partie de la clé car deux méthodes produisent deux valeurs distinctes pour le même instant (§6).

Périmètre : `National` + 12 régions métropolitaines (couverture éCO2mix régional). Historique : les jeux consolidés/définitifs RTE remontent à 2012 (national) / 2013 (régional) — base du modèle de prévision en phase 3.

### Cycle de vie de la donnée (révisions)

RTE **révise** ses données : le temps réel du mois M est remplacé par des données « consolidées » (annoncées par RTE « en milieu de M+1 », publiées en pratique avec ~3 mois de retard, cf. ADR-0003 addendum 2026-09-26), puis « définitives » (A+1). La donnée n'est donc **pas purement append-only**. Chaque mesure porte un **millésime** (`tr` | `consolidated` | `definitive`) ; l'ingestion fait un **upsert** sur la clé d'unicité et n'écrase une valeur que par un millésime de qualité supérieure ou égale (`definitive` > `consolidated` > `tr`). L'API sert toujours la meilleure version présente en base et expose le millésime (`vintage`) sur `/intensity/now`, `/intensity/now/all` et `/intensity/date` ; le poller et l'auto-réparation ne lisant que le temps réel, le consolidé et le définitif n'entrent en base que par la sous-commande `backfill` (§3). Détails et alternatives : **ADR-0006**.

**Stockage : PostgreSQL natif, sans extension** (voir ADR-0004).

- Table `measurement` simple : clé primaire `(region, at, methodology_id, methodology_version)` et index `(region, methodology_id, at DESC)`. Le partitionnement **déclaratif par plage temporelle** (mensuel) reste **reporté** — mesuré en I7 et non retenu à ce jour (relevé du 2026-09-25 : 148 Mo, requête SQL de `/intensity/date` sur 366 j ≤ 8,5 ms en `EXPLAIN ANALYZE` ; 221 Mo après le comblement régional du 2026-09-26, loin du seuil de 4 Go), seuils de ré-évaluation fixés par l'addendum ADR-0004 du 2026-09-25. Les écritures sont des upserts conditionnels au millésime : le poller ajoute le récent, le backfill et les rattrapages (fenêtre glissante, auto-réparation, comblements) écrivent du passé derrière du plus récent, sans remise en cause de l'ordre physique. L'index `BRIN` sur l'horodatage est en place depuis la migration `0012` (audit perf 2026-08 : il borne le scan du rafraîchissement des rollups aux blocs des 7 derniers jours).
- **Rollups** (horaire/journalier) pour les statistiques et le modèle : initialement des **vues matérialisées** (migration `0002`), désormais de **vraies tables incrémentales** upsertées par seau (migration `0010`, lecture inchangée) et rafraîchies par le poller. Le rafraîchissement doit être déclenché après toute révision touchant la période agrégée.
- Autour de `measurement` gravitent les **tables satellites** : `consumption` (charge), `weather_forecast` (météo, clé `(run_at, valid_at)` anti-fuite), `cross_border_flow` (ENTSO-E), `spot_price` (prix spot A44), `api_key`, `webhook_subscription` (clé étrangère vers `api_key`, `ON DELETE CASCADE`, migration `0013` : pas d'abonnement orphelin après une révocation) et `visit` — cf. `crates/adapter-postgres/migrations/`.
- Choix **réversible** : le port `IntensityRepository` permet d'ajouter un adapter TimescaleDB plus tard si le volume ou l'ingestion l'exigent.

## 6. Méthodologie carbone

Un chiffre d'intensité n'est comparable que si sa méthode est explicite. Distinctions à exposer dans l'API :

- émissions **directes** (combustion) vs **cycle de vie** (ACV) ;
- imports/exports inclus ou non ;
- périmètre géographique.

**Décision actée (ADR-0005)** :

- **Base MVP** : reprendre l'estimation RTE telle quelle (émissions de la production en France), directement comparable à éCO2mix.
- **La méthodologie est un attribut de premier ordre, versionné et sélectionnable**, dès le MVP : chaque mesure (domaine) et chaque réponse (API) porte un identifiant (`methodology`, ex. `rte-direct` + version), même s'il n'existe qu'une valeur au lancement.
- **Enrichissement livré** : une méthode additionnelle `acv-ademe` (cycle de vie via Base Carbone ADEME + imports interconnexions, façon modèle UK) **coexiste** avec `rte-direct`, sans rupture de contrat : `@1` basée production (national + 12 régions, ADR-0008) et `@2` basée consommation (national, ADR-0010).

Conséquence : le type `Measurement` du domaine porte un champ `methodology` dès la phase 1, de sorte que l'ajout d'une méthode n'altère jamais le sens d'une donnée déjà publiée.

## 7. Découpage en crates

Les crates publiables sont préfixées `carbonfr-*` même si les dossiers restent courts (`crates/core`, `crates/adapter-odre`, …).

| Crate | Rôle | Dépendances notables |
| --- | --- | --- |
| `core` | domaine + cas d'usage + ports | aucune IO (`sha2` = HMAC et `url` = validation anti-SSRF des webhooks : calculs purs, tolérés) |
| `eligibility` | domaine pur : éligibilité électrolyseur réseau (rulesets `rfnbo`/`low-carbon`, ADR-0025/0026) + part renouvelable prévue `share-clim@1` (ADR-0028) et expérience `share-meteo@2` (non servie) | aucune IO (carbonfr-core, time) |
| `adapter-odre` | `Eco2mixSource` + `ConsumptionSource` + `Eco2mixArchive` (eCO2mix RTE, national et régional) via HTTP ; suivi du quota réel (`QuotaTracker`) | reqwest, serde |
| `adapter-postgres` | repositories via SQL (Intensity, Consumption, Weather, CrossBorder, ApiKey, Subscription, SpotPrice, Visit) + migrations | sqlx |
| `adapter-http` | API HTTP `/v1` (DTO, OpenAPI/utoipa, SSE, middleware auth/quota) | axum, serde, utoipa |
| `adapter-forecast` | `ForecastModel` : climatologie (`climatology@1`) + prévision `acv-ademe@2` (`acv-clim`) | — |
| `adapter-meteo` | `WeatherForecastSource` via Open-Meteo (vent 100 m + irradiance) | reqwest |
| `adapter-entsoe` | `CrossBorderSource` (flux A11 + intensité voisine A75) + `SpotPriceSource` (spot A44) | reqwest, quick-xml |
| `adapter-webhook` | `Notifier` (livraison HMAC signée, anti-SSRF) | reqwest |
| `adapter-gbdt` | `ForecastModel` ML (`gbdt@1`, gardé par backtest — non servi) | gbdt |
| `sdk` | client HTTP Rust `carbonfr-sdk` de l'API publique (**hors hexagone** : c'est un client de `/v1`, pas un adapter du domaine — il ne dépend pas de `core`, ADR-0031 décision 5) ; 29 opérations `/v1` (dont le flux SSE), publié sur crates.io (I6 ; 0.3.0 depuis le 2026-10-20) | reqwest, rustls, time |
| `server` (bin) | composition root + poller unique + auto-réparation quotidienne + watcher de webhooks + purge des abonnements désactivés + registre `/metrics` + sous-commandes | toutes les précédentes sauf `sdk` |

## 8. Roadmap

Les sept phases sont **livrées** ; l'itération I10 du [plan](plan-iterations.md) est en cours (parité SDK TS ↔ OpenAPI, exemples OpenAPI et rattrapage documentaire livrés ; restent, vers la v0.12.0, les échéances datées de décembre à février : revue LCOE du 2026-12-15, backfill consolidé de juillet–septembre 2026, millésime TRV 2027-H1). État réel (version de workspace : cf. `[workspace.package] version` dans le `Cargo.toml` racine ; contrat d'API `/v1`) :

1. **Socle** ✅ — `core` + poller unique (`IngestLatest`, remplacé en 0.9.5 par `IngestRecent`) + `/intensity/now` + `/mix` + `/health`, national.
2. **Historique + régional** ✅ — backfill par export de masse (2012→), `/intensity/date`, `/intensity/stats`, 12 régions (servies en `acv-ademe`), rollups (passés de vues matérialisées à tables incrémentales).
3. **Prévision** ✅ — `ClimatologyForecaster` (`climatology@1`) derrière `ForecastModel` → `/intensity/forecast` + `/intensity/greenest-window`, intervalles `lower`/`expected`/`upper` (contrat `ForecastPoint`, ADR-0011), calage par backtest (N=10 sem., τ=14 j). Modèle ML `GbdtForecaster` (`gbdt@1`) exploré derrière le même port mais **non servi** (ne bat pas la climatologie au backtest).
4. **Enrichissement & usage** ✅ — `acv-ademe@2` consumption-based + `adapter-entsoe` + `/v1/methodologies` & `/v1/factors` (ADR-0010) ; store météo (`adapter-meteo`) ; primitives carbon-aware (`/schedule`, `/schedule/slots`, `/intensity/below`) + flux live **SSE** (`/intensity/stream`, ADR-0014) ; clés API en middleware de bord **opt-in**, anonyme par défaut (ADR-0015) ; webhooks signés (`POST`/`GET`/`DELETE /v1/webhooks`, ADR-0016).
5. **Enrichissement, déploiement & SDK** ✅ — échanges transfrontaliers (`/exchanges`, ADR-0017), météo (`/weather`) & dérivation renouvelable (`/renewable`, ADR-0018), prix de l'électricité (`/price`, ADR-0023), couche comparative LCOE (`/cost-reference`, ADR-0024) ; **SDK TypeScript** `@carbon-fr/sdk` ; observabilité `/metrics` (ADR-0022) ; déploiement live (voir §9).
6. **Couche électrolyseur / hydrogène** ✅ (ADR-0025/0026/0028/0029) — overlay `?eligibility=rfnbo|low-carbon` sur `/intensity/greenest-window` (part renouvelable **prévue** `share-clim@1` servie ; expérience `share-meteo@2` mesurée mais **non servie** — décision du 2026-07-04, addendum ADR-0028), catalogue `GET /v1/eligibility/rulesets`, carte « électrolyseurs × carbone live » `GET /hydrogene` (hors `/v1`, comme `/docs`).

7. **Fiabilité & écosystème Rust** ✅ (v0.7.0 → v0.11.0, août–octobre 2026 : audit 2026-08 puis itérations I0→I9 du [plan](plan-iterations.md) ; plus, sur `main` et non encore publiés (→ v0.12.0), les premiers items d'I10 : parité SDK TS ↔ OpenAPI, exemples et en-têtes OpenAPI) — audit 2026-08 (poller et arrêt gracieux robustes, fraîcheur bornée des jointures, `CachedForecaster`, sécurité de bord) ; ingestion par **fenêtre glissante** (`IngestRecent`) + **auto-réparation quotidienne** national/régional et quota ODRÉ réel en jauges `/metrics` (ADR-0003/0022, addenda de septembre 2026) ; plafond SSE (ADR-0014, addendum 2026-09-26), délais HTTP entrants (SEC-4, v0.10.0), compression gzip/brotli des réponses hors flux SSE (PERF-1, v0.11.0) ; historique régional `acv-ademe` comblé par export de masse ; millésime TRV `2026-H2` ; **crates.io** : `carbonfr-core` + `carbonfr-eligibility` (ADR-0030) et **SDK Rust** `carbonfr-sdk` (ADR-0031), MSRV 1.88, `cargo-semver-checks` ; montées `utoipa` 6 / `reqwest` 0.13 / `sqlx` 0.9 ; API groupée (`/intensity/now/all`, `/regions`, `shares` de `/mix`), **index qualitatif** (`index`, `/intensity/index`, ADR-0033), **export CSV**, `servers` dans l'OpenAPI puis (sur `main`) exemples et en-têtes des middlewares, tests de parité SDK ↔ OpenAPI — Rust dès `carbonfr-sdk` 0.1 (ADR-0031), TypeScript sur `main` (ADR-0032) ; gouvernance : ruleset de `main` (ADR-0027, en place depuis juin 2026) porté de 5 à 8 contrôles requis le 2026-09-24, Trusted Publishing npm et crates.io.

**Méthodologie enrichie** (transverse aux phases) : `acv-ademe` (cycle de vie Base Carbone ADEME + imports interconnexions) coexiste avec `rte-direct`, livrée en production `@1` (national + 12 régions) et consommation `@2` (national, ADR-0008/0010).

## 9. Déploiement

L'API est **live** sur un VPS géré par Kovelt (détails et alternatives : **ADR-0007**) :

- **API** : service `carbonfr-server` (poller **intégré**) + PostgreSQL dédié, déployé en conteneur sur le **VPS Kovelt**, derrière **Traefik** (terminaison TLS Let's Encrypt, en-têtes de sécurité, `X-Forwarded-For` de confiance). Le service tire une **image taguée depuis GHCR** (`ghcr.io/kovelt/carbon-fr:X.Y.Z`, épinglée — pas de build sur place) ; backfill réalisé, sauvegardes quotidiennes hors serveur avec **restauration testée** (procédure et mesures : [`deploy/README.md` §4](../deploy/README.md#4-sauvegarde--restauration)), supervision Prometheus + sonde externe ([§3](../deploy/README.md#3-supervision--alertes)).
- **Image & release** : `Dockerfile` multi-stage (build `rust:1-bookworm` via rustls sans OpenSSL, runtime `debian:bookworm-slim` non-root). Le workflow `release.yml` se déclenche sur tag `v*` (garde-fou : tag == version de workspace) et publie l'image GHCR taguée `X.Y.Z`/`X.Y`/`latest` (publique).
- **Forme du poller** : **intégré** au `server` (un seul binaire ; le SSE passe par un canal mémoire `tokio::broadcast`). Un `bin/poller` séparé sur `LISTEN`/`NOTIFY` reste documenté comme évolution si plusieurs instances API sont nécessaires. Le même binaire porte aussi le **watcher de webhooks** (ADR-0016, détecte les franchissements et livre) et la **purge périodique** (6 h) des abonnements désactivés (ADR-0016, addendum « purge »).
- **Limites & compression** (I8/I9) : au plus 300 flux SSE simultanés (`CARBONFR_SSE_MAX_CONNECTIONS`, `503` + `Retry-After: 30` au-delà), délai de traitement d'une requête de 60 s (`CARBONFR_REQUEST_TIMEOUT_SECS`, flux SSE exclu), délai de lecture des en-têtes et d'inactivité keep-alive de 180 s (`CARBONFR_HEADER_READ_TIMEOUT_SECS`, 120 s en prod derrière Traefik), 10 000 connexions TCP au plus (`CARBONFR_MAX_CONNECTIONS` — relever `ulimit -n` en conséquence, cf. `deploy/README.md` §2) ; réponses compressées en gzip ou brotli à la demande, jamais le flux SSE.
- **Variantes self-host fournies** dans `deploy/` (exemples, pas la prod) : un `Caddyfile` (terminaison TLS + reverse proxy, sonde `/health/ready`) et une unité `carbonfr.service` durcie (systemd, `Restart=on-failure`). À coupler avec `CARBONFR_TRUST_PROXY=1` et un `CARBONFR_VISIT_SALT` non-défaut (le serveur refuse de démarrer sans, derrière proxy).
- **DNS** : sous-domaine Kovelt (`carbon-fr-api.kovelt.fr`).
- **Contrat d'URL** : l'API est versionnée dans le chemin (`/v1/…`) dès le départ, pour migrer de domaine ou faire évoluer l'API sans casser les intégrations.

> Configuration par variables d'environnement (`DATABASE_URL` requis ; `CARBONFR_BIND`, `CARBONFR_POLL_SECS`, `CARBONFR_TRUST_PROXY`, `CARBONFR_VISIT_SALT`, `CARBONFR_RATELIMIT_ENABLED`, `CARBONFR_ENTSOE_TOKEN` (active `acv-ademe@2` + `/price`), `CARBONFR_*_CALIBRATE_WEEKS`, …) — voir `.env.example`. Sous-commandes : `backfill`, `backtest`/`-sweep`/`-bands`/`-acv`/`-renewable`/`-share`/`-share-meteo`, `analyze-renewable-signal`, `train`, `mint-key`, `list-keys`, `revoke-key`.

## 10. Sources de données & références

Tout est **re-traité et cité, jamais approprié** (cf. ADR-0003). Chaque source est isolée derrière un adapter.

**Données ingérées par le poller** (alimentent le read-model) :

- **RTE — éCO2mix**, via **ODRÉ** (licence ouverte) — intensité carbone + mix de production :
  - émissions de CO₂ (national, `rte-direct`) : <https://www.rte-france.com/donnees-publications/eco2mix-donnees-temps-reel/emissions-co2-par-kwh-produit-france>
  - national temps réel : <https://odre.opendatasoft.com/explore/dataset/eco2mix-national-tr/>
  - régional temps réel : <https://odre.opendatasoft.com/explore/dataset/eco2mix-regional-tr/>
  - national consolidé/définitif (export de masse pour le backfill) : <https://odre.opendatasoft.com/explore/dataset/eco2mix-national-cons-def/>
  - régional consolidé/définitif (export de masse du backfill régional, pas de 30 min) : <https://odre.opendatasoft.com/explore/dataset/eco2mix-regional-cons-def/>
- **ENTSO-E — Transparency Platform** (si `CARBONFR_ENTSOE_TOKEN`) — flux physiques transfrontaliers (`A11`), génération par type des pays voisins (`A75`) et prix de gros day-ahead (`A44`) ; alimente `acv-ademe@2`, `/exchanges` et `/price` (ADR-0010/0017/0023). <https://transparency.entsoe.eu/>
- **Open-Meteo** (CC-BY 4.0, attribué) — prévision **et** archive météo (vent 100 m + irradiance) ; alimente `/weather`, `/renewable` et les features ML (ADR-0012/0018). <https://open-meteo.com/>

**Référentiels versionnés** (constantes de domaine, pas des flux live) :

- **ADEME — Base Carbone / Base Empreinte** — facteurs d'émission cycle de vie par filière (méthodologie `acv-ademe`, ADR-0008). <https://base-empreinte.ademe.fr/>
- **CRE** (délibérations TURPE 7, TRV, accise) & **BOFiP** (accise, TVA) — décomposition du prix `/price` (ADR-0023). <https://www.cre.fr/> · <https://bofip.impots.gouv.fr/>
- **Couche comparative LCOE** `/cost-reference` (ADR-0024) : **Cour des comptes** & **CRE** (nucléaire existant), **IRENA** (LCOE renouvelables mondiaux), **RTE** *Futurs énergétiques 2050* (nucléaire nouveau), **ADEME** (renouvelables France).

**Données embarquées de la carte `/hydrogene`** (ADR-0029, servies avec la page, attribution incluse) :

- **European Hydrogen Observatory** © Clean Hydrogen JU — sites d'électrolyseurs UE (instantané semestriel, filtre *Water electrolysis*). <https://observatory.clean-hydrogen.europa.eu/>
- **IGN — Admin Express** (Licence Ouverte 2.0) — contours régionaux, simplifiés.
- **Natural Earth** (domaine public) — contexte pays voisins.

**Modèle de référence & écosystème** :

- Modèle britannique : <https://carbonintensity.org.uk/> et <https://api.carbonintensity.org.uk/>
- Concurrent fermé : <https://www.electricitymaps.com/>
