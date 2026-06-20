# Architecture — `carbon-fr`

Ce document décrit la conception du système : la vision, les contraintes, le découpage hexagonal, le modèle de données et la roadmap. Les décisions ponctuelles et leurs alternatives sont tracées dans les [ADR](adr/).

## 1. Vision & contraintes

`carbon-fr` rend la donnée carbone du réseau électrique français **directement consommable par des développeurs et des machines**, là où la source officielle reste brute et plafonnée.

Contraintes structurantes :

- **Souveraineté** : aucune dépendance propriétaire obligatoire, auto-hébergeable, licences OSI.
- **Dev-first** : API lisible, index humainement interprétable, SDK, OpenAPI.
- **Résilience au quota** : la source impose 50 000 appels/utilisateur/mois ; l'architecture ne doit pas exposer ce plafond aux clients.
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
| `/exchanges`, `/weather`, `/factors`, `/methodologies` | Donnée externe ou catalogue, exposés tels quels | Faible |
| `/renewable`, `/price`, `/cost-reference` | Donnée dérivée/composée (modèle physique, décomposition TRV, fourchettes LCOE) | Moyenne |
| `/intensity/forecast`, `/greenest-window`, `/schedule*`, `/intensity/below`, `/intensity/stream` | **Valeur créée** : il faut modéliser (prévision) puis l'outiller (carbon-aware, live) | Élevée |

La prévision est donc une **responsabilité du service**, pas un simple proxy. Le modèle est branché derrière un port (`ForecastModel`) : on a démarré avec un modèle statistique (`climatology@1`) et un modèle ML (`gbdt@1`) a été exploré derrière le même port sans toucher au reste — il n'est pas servi car il ne bat pas la climatologie au backtest. À titre de référence, le modèle britannique repose sur du machine learning et de la modélisation réseau, avec une prévision à 96 h et plus.

## 3. Quota, ingestion & cache

Le quota (50 000 appels/utilisateur/mois) est un faux problème **à une condition** : un seul composant tape la source.

- Le national temps réel se rafraîchit toutes les 15 min → ~96 récupérations/jour.
- Le régional temps réel se rafraîchit toutes les heures, et **un seul appel renvoie les 12 régions** → ~24 récupérations/jour (pas ×12).
- Un **poller unique** (singleton) alimente la base ; l'API sert ensuite **tous** les clients depuis la base. En haute disponibilité, l'écriture reste assurée par un seul poller (élection de leader) pour éviter les appels en double.

Budget indicatif : ~96/jour (national) + ~24/jour (régional) ≈ **~120 appels/jour, soit ~3 700/mois — moins de 8 % du quota**. Le plafond ne concerne que les intégrateurs qui taperaient RTE en direct, précisément ce que `carbon-fr` leur évite. C'est aussi ce qui justifie l'existence du service par rapport à « tape l'open data toi-même ».

> La base joue le rôle de **read-model** (le « cache ») : pas de couche de cache séparée au départ. Une couche mémoire chaude pour `/intensity/now` pourra être ajoutée plus tard si le besoin se présente.

**Backfill historique** : le rapatriement de l'historique (2012→) ne passe **pas** par l'API paginée — cela brûlerait le quota — mais par l'**export en masse** du jeu de données ODRÉ (un téléchargement), réalisé une fois puis maintenu à jour par le poller.

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
- `IntensityRepository` — lire/écrire les mesures (chaud + historique ; upsert conditionnel au millésime, rollups).
- `ForecastModel` — produire une prévision (rend des `ForecastPoint` avec intervalle).
- `ConsumptionRepository` / `ConsumptionSource` — charge réalisée/prévue (entrée du futur ML).
- `WeatherRepository` / `WeatherForecastSource` — météo (vent 100 m + irradiance, Open-Meteo).
- `CrossBorderSource` / `CrossBorderRepository` — flux transfrontaliers + intensité du voisin (ENTSO-E, pour `acv-ademe@2` et `/exchanges`).
- `SpotPriceSource` / `SpotPriceRepository` — prix spot day-ahead (ENTSO-E A44, pour `/price`).
- `ApiKeyRepository` — résolution des clés API (port **de bord** : consommé par le middleware, jamais par un cas d'usage).
- `SubscriptionRepository` / `Notifier` — abonnements webhooks + livraison signée.
- `VisitCounter` — compteur de visiteurs (IP jamais stockée).
- `Clock` — fournir l'instant courant (testabilité).

**Ports entrants** (cas d'usage exposés, génériques sur leurs ports) : `GetCurrentIntensity`, `GetIntensityHistory`, `GetIntensityStats`, `IngestLatest` (le poller), `BackfillHistory`, `FindGreenestWindow`, `CarbonAwareScheduler` (planification carbon-aware), `GetConsumptionIntensity` (`acv-ademe@2` à la lecture), `GetCrossBorderExchanges` (`/exchanges`), `GetElectricityPrice` (`/price`), `GetWeather`, `CalibrateRenewable`, `AnalyzeRenewableSignal`, plus les backtests (`BacktestForecast`, `BacktestConsumptionForecast`, `BacktestRenewable`).

**Pourquoi ce pattern ici précisément** :

1. *Risque source* (RTE change, quota, panne) → ajouter un adapter de secours (ENTSO-E, voire Electricity Maps) derrière le même `Eco2mixSource`, sans toucher au domaine.
2. *Roadmap incrémentale* → la phase 3 (prévision) n'est qu'un nouvel adapter derrière `ForecastModel`. La séparation « données repackagées » / « valeur créée » est matérialisée dans le code.
3. *Testabilité* → le `core` se teste avec des fakes en mémoire, sans base ni réseau.

## 5. Modèle de données

Unité d'observation : une **mesure** = `{ horodatage, région, intensité (gCO₂eq/kWh), méthodologie (identifiant + version), millésime, mix de production (optionnel) }`, au pas quart d'heure. Les champs **méthodologie** (voir §6 et ADR-0005) et **millésime** (voir ci-dessous et ADR-0006) sont portés dès le départ.

**Clé d'unicité** : `(région, horodatage, méthodologie)`. La méthodologie fait partie de la clé car deux méthodes produisent deux valeurs distinctes pour le même instant (§6).

Périmètre : `National` + 12 régions métropolitaines (couverture éCO2mix régional). Historique : les jeux consolidés/définitifs RTE remontent à 2012 (national) / 2013 (régional) — base du modèle de prévision en phase 3.

### Cycle de vie de la donnée (révisions)

RTE **révise** ses données : le temps réel du mois M est remplacé par des données « consolidées » (M+1), puis « définitives » (A+1). La donnée n'est donc **pas purement append-only**. Chaque mesure porte un **millésime** (`tr` | `consolidated` | `definitive`) ; l'ingestion fait un **upsert** sur la clé d'unicité et n'écrase une valeur que par un millésime de qualité supérieure (`definitive` > `consolidated` > `tr`). L'API sert toujours la meilleure version disponible et expose le millésime. Détails et alternatives : **ADR-0006**.

**Stockage : PostgreSQL natif, sans extension** (voir ADR-0004).

- Table `measurement` simple, index sur `(region, horodatage)` et sur `(region, methodology, at)`. Le partitionnement **déclaratif par plage temporelle** (mensuel) et l'index `BRIN` sur l'horodatage restent **reportés** (à reconsidérer maintenant que l'historique complet est ingéré) : les insertions arrivent ordonnées dans le temps ; les révisions sont des `UPDATE` ciblés (upsert), pas une remise en cause de l'ordre physique.
- **Rollups** (horaire/journalier) pour les statistiques et le modèle : initialement des **vues matérialisées** (migration `0002`), désormais de **vraies tables incrémentales** upsertées par seau (migration `0010`, lecture inchangée) et rafraîchies par le poller. Le rafraîchissement doit être déclenché après toute révision touchant la période agrégée.
- Choix **réversible** : le port `IntensityRepository` permet d'ajouter un adapter TimescaleDB plus tard si le volume ou l'ingestion l'exigent.

## 6. Méthodologie carbone

Un chiffre d'intensité n'est comparable que si sa méthode est explicite. Distinctions à exposer dans l'API :

- émissions **directes** (combustion) vs **cycle de vie** (ACV) ;
- imports/exports inclus ou non ;
- périmètre géographique.

**Décision actée (ADR-0005)** :

- **Base MVP** : reprendre l'estimation RTE telle quelle (émissions de la production en France), directement comparable à éCO2mix.
- **La méthodologie est un attribut de premier ordre, versionné et sélectionnable**, dès le MVP : chaque mesure (domaine) et chaque réponse (API) porte un identifiant (`methodology`, ex. `rte-direct` + version), même s'il n'existe qu'une valeur au lancement.
- **Enrichissement engagé** : une méthode additionnelle `acv-ademe` (cycle de vie via Base Carbone ADEME + imports interconnexions, façon modèle UK) **coexistera** avec `rte-direct`, sans rupture de contrat. Sa spécification fera l'objet d'un ADR dédié.

Conséquence : le type `Measurement` du domaine porte un champ `methodology` dès la phase 1, de sorte que l'ajout d'une méthode n'altère jamais le sens d'une donnée déjà publiée.

## 7. Découpage en crates

Les crates publiables sont préfixées `carbonfr-*` même si les dossiers restent courts (`crates/core`, `crates/adapter-odre`, …).

| Crate | Rôle | Dépendances notables |
| --- | --- | --- |
| `core` | domaine + cas d'usage + ports | aucune IO (`sha2` toléré = calcul pur HMAC) |
| `adapter-odre` | `Eco2mixSource` + `Eco2mixArchive` (eCO2mix RTE) via HTTP | reqwest, serde |
| `adapter-postgres` | repositories via SQL (Intensity, Consumption, Weather, CrossBorder, ApiKey, Subscription, SpotPrice, Visit) + migrations | sqlx |
| `adapter-http` | API HTTP `/v1` (DTO, OpenAPI/utoipa, SSE, middleware auth/quota) | axum, serde, utoipa |
| `adapter-forecast` | `ForecastModel` : climatologie (`climatology@1`) + prévision `acv-ademe@2` (`acv-clim`) | — |
| `adapter-meteo` | `WeatherForecastSource` via Open-Meteo (vent 100 m + irradiance) | reqwest |
| `adapter-entsoe` | `CrossBorderSource` (flux A11 + intensité voisine A75) + `SpotPriceSource` (spot A44) | reqwest, quick-xml |
| `adapter-webhook` | `Notifier` (livraison HMAC signée, anti-SSRF) | reqwest |
| `adapter-gbdt` | `ForecastModel` ML (`gbdt@1`, gardé par backtest — non servi) | gbdt |
| `server` (bin) | composition root + poller unique + registre `/metrics` + sous-commandes | toutes les précédentes |

## 8. Roadmap

Les cinq phases sont **livrées**. État réel (version de workspace `0.3.2`, contrat d'API `/v1`) :

1. **Socle** ✅ — `core` + poller unique (`IngestLatest`) + `/intensity/now` + `/mix` + `/health`, national.
2. **Historique + régional** ✅ — backfill par export de masse (2012→), `/intensity/date`, `/intensity/stats`, 12 régions (servies en `acv-ademe`), rollups (passés de vues matérialisées à tables incrémentales).
3. **Prévision** ✅ — `ClimatologyForecaster` (`climatology@1`) derrière `ForecastModel` → `/intensity/forecast` + `/greenest-window`, intervalles `lower`/`expected`/`upper` (contrat `ForecastPoint`, ADR-0011), calage par backtest (N=10 sem., τ=14 j). Modèle ML `GbdtForecaster` (`gbdt@1`) exploré derrière le même port mais **non servi** (ne bat pas la climatologie au backtest).
4. **Enrichissement & usage** ✅ — `acv-ademe@2` consumption-based + `adapter-entsoe` + `/v1/methodologies` & `/v1/factors` (ADR-0010) ; store météo (`adapter-meteo`) ; primitives carbon-aware (`/schedule`, `/schedule/slots`, `/intensity/below`) + flux live **SSE** (`/intensity/stream`, ADR-0014) ; clés API en middleware de bord **opt-in**, anonyme par défaut (ADR-0015) ; webhooks signés (`POST`/`GET`/`DELETE /v1/webhooks`, ADR-0016).
5. **Enrichissement, déploiement & SDK** ✅ — échanges transfrontaliers (`/exchanges`, ADR-0017), météo (`/weather`) & dérivation renouvelable (`/renewable`, ADR-0018), prix de l'électricité (`/price`, ADR-0023), couche comparative LCOE (`/cost-reference`, ADR-0024) ; **SDK TypeScript** `@carbon-fr/sdk` ; observabilité `/metrics` (ADR-0022) ; déploiement live (voir §9).

**Méthodologie enrichie** (transverse aux phases) : `acv-ademe` (cycle de vie Base Carbone ADEME + imports interconnexions) coexiste avec `rte-direct`, livrée en production `@1` (national + 12 régions) et consommation `@2` (national, ADR-0008/0010).

## 9. Déploiement

L'API est **live** sur un VPS géré par Kovelt (détails et alternatives : **ADR-0007**) :

- **API** : service `carbonfr-server` (poller **intégré**) + PostgreSQL dédié, déployé en conteneur sur le **VPS Kovelt**, derrière **Traefik** (terminaison TLS Let's Encrypt, en-têtes de sécurité, `X-Forwarded-For` de confiance). Le service tire une **image taguée depuis GHCR** (`ghcr.io/kovelt/carbon-fr:X.Y.Z`, épinglée — pas de build sur place) ; backfill réalisé, backups quotidiens.
- **Image & release** : `Dockerfile` multi-stage (build `rust:1-bookworm` via rustls sans OpenSSL, runtime `debian:bookworm-slim` non-root). Le workflow `release.yml` se déclenche sur tag `v*` (garde-fou : tag == version de workspace) et publie l'image GHCR taguée `X.Y.Z`/`X.Y`/`latest` (publique).
- **Forme du poller** : **intégré** au `server` (un seul binaire ; le SSE passe par un canal mémoire `tokio::broadcast`). Un `bin/poller` séparé sur `LISTEN`/`NOTIFY` reste documenté comme évolution si plusieurs instances API sont nécessaires.
- **Variantes self-host fournies** dans `deploy/` (exemples, pas la prod) : un `Caddyfile` (terminaison TLS + reverse proxy, sonde `/health/ready`) et une unité `carbonfr.service` durcie (systemd, `Restart=on-failure`). À coupler avec `CARBONFR_TRUST_PROXY=1` et un `CARBONFR_VISIT_SALT` non-défaut (le serveur refuse de démarrer sans, derrière proxy).
- **DNS** : sous-domaine Kovelt (`carbon-fr-api.kovelt.fr`).
- **Contrat d'URL** : l'API est versionnée dans le chemin (`/v1/…`) dès le départ, pour migrer de domaine ou faire évoluer l'API sans casser les intégrations.

> Configuration par variables d'environnement (`DATABASE_URL` requis ; `CARBONFR_BIND`, `CARBONFR_POLL_SECS`, `CARBONFR_TRUST_PROXY`, `CARBONFR_VISIT_SALT`, `CARBONFR_RATELIMIT_ENABLED`, `CARBONFR_ENTSOE_TOKEN` (active `acv-ademe@2` + `/price`), `CARBONFR_*_CALIBRATE_WEEKS`, …) — voir `.env.example`. Sous-commandes : `backfill`, `backtest`/`-sweep`/`-bands`/`-acv`/`-renewable`, `analyze-renewable-signal`, `train`, `mint-key`.

## 10. Sources de données & références

Tout est **re-traité et cité, jamais approprié** (cf. ADR-0003). Chaque source est isolée derrière un adapter.

**Données ingérées par le poller** (alimentent le read-model) :

- **RTE — éCO2mix**, via **ODRÉ** (licence ouverte) — intensité carbone + mix de production :
  - émissions de CO₂ (national, `rte-direct`) : <https://www.rte-france.com/eco2mix/les-emissions-de-co2-par-kwh-produit-en-france>
  - national temps réel : <https://odre.opendatasoft.com/explore/dataset/eco2mix-national-tr/>
  - régional temps réel : <https://odre.opendatasoft.com/explore/dataset/eco2mix-regional-tr/>
  - national consolidé/définitif (export de masse pour le backfill) : <https://odre.opendatasoft.com/explore/dataset/eco2mix-national-cons-def/>
- **ENTSO-E — Transparency Platform** (si `CARBONFR_ENTSOE_TOKEN`) — flux physiques transfrontaliers (`A11`), génération par type des pays voisins (`A75`) et prix de gros day-ahead (`A44`) ; alimente `acv-ademe@2`, `/exchanges` et `/price` (ADR-0010/0017/0023). <https://transparency.entsoe.eu/>
- **Open-Meteo** (CC-BY 4.0, attribué) — prévision **et** archive météo (vent 100 m + irradiance) ; alimente `/weather`, `/renewable` et les features ML (ADR-0012/0018). <https://open-meteo.com/>

**Référentiels versionnés** (constantes de domaine, pas des flux live) :

- **ADEME — Base Carbone / Base Empreinte** — facteurs d'émission cycle de vie par filière (méthodologie `acv-ademe`, ADR-0008). <https://base-empreinte.ademe.fr/>
- **CRE** (délibérations TURPE 7, TRV, accise) & **BOFiP** (accise, TVA) — décomposition du prix `/price` (ADR-0023). <https://www.cre.fr/> · <https://bofip.impots.gouv.fr/>
- **Couche comparative LCOE** `/cost-reference` (ADR-0024) : **Cour des comptes** & **CRE** (nucléaire existant), **IRENA** (LCOE renouvelables mondiaux), **RTE** *Futurs énergétiques 2050* (nucléaire nouveau), **ADEME** (renouvelables France).

**Modèle de référence & écosystème** :

- Modèle britannique : <https://carbonintensity.org.uk/> et <https://api.carbonintensity.org.uk/>
- Concurrent fermé : <https://www.electricitymaps.com/>
