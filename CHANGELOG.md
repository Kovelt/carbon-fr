# Changelog

Tous les changements notables de ce projet sont consignés dans ce fichier.

Le format s'inspire de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/),
et le projet suit le [versionnage sémantique](https://semver.org/lang/fr/). En
phase `0.x`, des ruptures d'API peuvent survenir en *minor* (cf. GOUVERNANCE §6).

## [Non publié]

### Ajouté

- **Socle hexagonal** : crate `core` (domaine, cas d'usage, ports, sans IO),
  adapters `odre` (ODRÉ/éCO2mix), `postgres` (PostgreSQL natif) et `http`
  (axum), et binaire `carbonfr-server` (composition root + poller unique).
- **API `/v1`** (couverture nationale) :
  - `GET /v1/intensity/now` — dernière intensité carbone (gCO₂eq/kWh) ;
  - `GET /v1/mix` — mix de production par filière (MW) ;
  - `GET /v1/intensity/date?from=&to=` — série historique sur un intervalle ;
  - `GET /v1/intensity/stats?from=&to=[&interval=hour|day]` — résumé
    (moyenne/min/max) et série agrégée depuis les rollups ;
  - `GET /health` — sonde de disponibilité.
- **Backfill historique** national par export de masse ODRÉ
  (`carbonfr-server backfill`), upsert conditionnel au millésime.
- **Rollups** : vues matérialisées horaires et journalières, rafraîchies par le
  poller et le backfill.
- **Méthodologie `acv-ademe@1`** (cycle de vie ADEME, basée production, ADR-0008)
  coexistant avec `rte-direct` : dérivée et stockée à l'ingestion, sélectionnable
  via `?methodology=` sur les endpoints `/v1`.
- **Couverture régionale** (12 régions métropolitaines) : le poller ingère le
  mix régional (éCO2mix régional, `thermique` agrégé) et en dérive l'intensité
  `acv-ademe`. `rte-direct` reste national (taux_co2 publié par RTE).
- **OpenAPI 3.1** dérivée du code (`utoipa`) sous `GET /v1/openapi.json` +
  **Swagger UI** sous `GET /docs`.
- **Collection Bruno** versionnée (`bruno/`) couvrant tous les endpoints
  (cas nominaux national/régional × `rte-direct`/`acv-ademe`, et erreurs 400/404).
- **Prévision d'intensité** (phase 3, ADR-0009) : modèle `climatology@1`
  (climatologie horaire-de-semaine glissante + correction de persistance
  décroissante), fonction de domaine pure + adapter `ClimatologyForecaster`
  (alimenté par l'historique stocké). Exposée sous
  `GET /v1/intensity/forecast?from=&horizon_hours=` (série prévue) et
  `GET /v1/intensity/greenest-window?from=&horizon_hours=&window_minutes=`
  (créneau le plus bas-carbone). Prévisions **non persistées** (calculées à la
  lecture) ; l'identité du modèle est exposée dans chaque réponse.
- **Contrat de prévision `ForecastPoint`** (ADR-0011) : type domaine dédié avec
  **intervalle d'incertitude** (`expected`/`lower`/`upper`), `ModelVersion` et
  **sans `vintage`** — remplace le `Vec<Measurement>` du port `ForecastModel`.
  `GET /v1/intensity/forecast` expose l'intervalle ; `greenest-window` gagne un
  sélecteur `?estimator=central|prudent`.
- **Intervalles par quantiles de résidus par horizon** (ADR-0011 §5) : type
  `HorizonBands` calibré par backtest *walk-forward* (`backtest-bands`) ; les
  bornes **s'élargissent avec l'horizon**. Le serveur auto-calibre au démarrage
  (`CARBONFR_FORECAST_CALIBRATE_WEEKS`), avec repli sur la dispersion par créneau.
- **Framework de prévision ML GBDT** (ADR-0012, tranche 2a) : crate
  `carbonfr-adapter-gbdt` (`gbdt` pur Rust) — *feature engineering* partagé
  train/inférence (anti-fuite), `train_model`, `GbdtForecaster` (artefact
  versionné chargé par chemin), sous-commande `carbonfr-server train`
  (entraîne → sauve → compare `gbdt@1` vs `climatology@1` au backtest).
  *Mesuré* : sans features météo, le GBDT **ne bat pas** la climatologie calibrée
  (attendu — la météo est le levier) ; `climatology@1` **reste servi**.
- **Backfill météo historique + features météo/climatologie** (ADR-0012,
  tranche 2b) : archive des prévisions Open-Meteo (anti-fuite `run_at`), features
  vent/irradiance *as-of* + climatologie de créneau (apprentissage résiduel),
  calcul identique train/inférence. *Mesuré* : `gbdt@1` ne bat **toujours pas**
  `climatology@1` (~2× pire), même entraîné sur l'année entière → baseline
  calibrée difficile ; `@1` reste servi. Correctif : dédup `(region, at)` dans
  l'upsert de charge.
- **Store de prévision météo** (ADR-0012, tranche 1 du modèle ML) : port
  `WeatherForecastSource` + adapter `carbonfr-adapter-meteo` (Open-Meteo, vent à
  100 m + irradiance, agrégés sur 7 points de métropole), store
  `WeatherRepository` (table `weather_forecast`) **daté `(run_at, valid_at)`**
  pour l'anti-fuite, ingéré par le poller. Entrée du futur `GbdtForecaster`.
- **Store de charge** (consommation réalisée + prévue RTE) : table `consumption`,
  ports `ConsumptionRepository`/`ConsumptionSource`, ingestion par le poller
  (conso récente + prévisions J-1/J) et backfill de la réalisée. Entrée
  réutilisable pour le futur modèle ML (ADR-0012). *Note* : l'ajustement
  **linéaire** de la prévision par la charge (ADR-0011 §4) a été essayé puis
  **écarté** — mesuré moins bon que la climatologie seule (cf. ADR-0011).
- **Backtest** du modèle de prévision (`carbonfr-server backtest`, ADR-0009) :
  évaluation *walk-forward* sur l'historique, MAE/RMSE global et par horizon
  (h+1/h+6/h+24), comparés à une référence de persistance — pour mesurer la
  précision plutôt que la supposer. Mode `backtest-sweep` (balayage N × τ).
- **Calibration de `climatology@1`** (addendum ADR-0009) : défauts révisés
  `N = 10 semaines`, `τ = 2 semaines`, calés par backtest sur la donnée réelle
  2024 — le modèle bat désormais la persistance à tous les horizons (l'ancien
  `τ = 6 h` la sous-performait). Formule et contrat d'API inchangés.
- **Méthodologie `acv-ademe@2` consumption-based — domaine pur + vérifiabilité**
  (ADR-0010, tranche A) : trait de domaine `MethodologyCalculator`
  (`RteDirect` / `AcvAdemeProduction` / `AcvAdemeConsumption`), value object
  `CrossBorderFlows` (flux signés par voisin + intensité du voisin, enum
  `Neighbor`), calcul pur *consumption-based* (imports valorisés à l'intensité
  du voisin − exports + **pertes T&D**) — **sans IO**. `acv-ademe@2` est une
  version **distincte** de `@1` (production), qui reste publié (gouvernance
  ADR-0005). Deux endpoints de **vérifiabilité**, sans dépendance externe :
  `GET /v1/methodologies` (catalogue + versions) et `GET /v1/factors` (table des
  facteurs par filière + facteur de pertes T&D). *Le calcul de `@2` sera **servi**
  une fois la source d'import ENTSO-E branchée (tranche B) ; il apparaît `planned`
  dans `/v1/methodologies`.* Défaut de l'API inchangé : `rte-direct`.
- **Adapter ENTSO-E — contexte d'import transfrontalier** (ADR-0010, tranche B
  1/2) : port `CrossBorderSource` + value object horodaté `CrossBorderSnapshot`
  (domaine) et crate `carbonfr-adapter-entsoe`. Pour chaque frontière de la
  France métropolitaine : **flux physique net signé** (`documentType=A11`, import
  − export) et **intensité carbone du voisin** dérivée de sa génération par type
  (`documentType=A75`/`processType=A16`) via les **mêmes facteurs ADEME** que le
  domaine (mapping `PsrType` B01–B25 → filières, zones EIC). Token
  `CARBONFR_ENTSOE_TOKEN` ; jamais appelé par requête utilisateur. Parsing XML
  testé sur fixtures ; *chemins XML/codes calés sur le guide RESTful API ENTSO-E,
  **à valider contre l'API live** (`tests/live.rs`, `--ignored`).*
- **`acv-ademe@2` servie : store + ingestion + lecture** (ADR-0010, tranche B
  2/2) : port + store Postgres `CrossBorderRepository` (table `cross_border_flow`,
  migration `0007`, testé sur Postgres réel) ; le poller ingère le contexte
  d'import à chaque cycle **si `CARBONFR_ENTSOE_TOKEN` est défini** (source
  optionnelle, non bloquante) ; cas d'usage `GetConsumptionIntensity` (calcul
  **à la lecture**, sans stockage de ligne `@2`) exposé via
  **`GET /v1/intensity/now?methodology=acv-ademe&version=2`** (national).
  `acv-ademe@2` passe `served` dans `/v1/methodologies`. Défaut de l'API inchangé
  (`rte-direct`) ; sans token, le calcul renvoie `404` faute de contexte d'import.
- **`acv-ademe@2` sur l'historique et les stats** (ADR-0010 §6) : la méthode
  consommation est servie **à la lecture** au-delà de `/now`, via
  `GET /v1/intensity/date` et `GET /v1/intensity/stats`
  (`?methodology=acv-ademe&version=2`, national). Port
  `CrossBorderRepository::flows_range`, fonction pure `derive_consumption_series`
  (jointure mix × contexte d'import le plus proche), agrégats `summarize`/
  `bucketize` calculés dans le domaine (la série `@2` n'est pas matérialisée).
  `@2` n'existe que là où le contexte d'import a été ingéré.
- **Prévision `acv-ademe@2` (consumption-based)** (ADR-0013, tranche A) : on
  prévoit les **entrées** (mix par filière + contexte d'import : flux et intensité
  de chaque voisin) par climatologie horaire-de-semaine + correction de
  persistance (formule `climatology@1`, par canal), puis on applique le **même**
  calculateur pur `AcvAdeme` (ADR-0010) — la prévision hérite de la version de
  méthode, reste **auditable** et **converge vers le nowcast** quand l'horizon → 0
  (invariant testé). Fonction domaine `acv_ademe_forecast`, adapter
  `AcvAdemeForecaster<R, C>`, **routage par méthode** au composition root, servi
  via `GET /v1/intensity/forecast?methodology=acv-ademe&version=2` (national).
  *Modèle `acv-clim@1`* ; baseline que le futur `MixForecaster` GBDT + ENTSO-E
  day-ahead devront battre (garde de promotion).
- **Primitives de scheduling carbon-aware** (ADR-0014, tranche A) : fonctions
  **pures** du domaine (zéro nouveau port) sur la prévision, réutilisant le
  sélecteur `central`/`prudent` — créneau contigu le plus bas-carbone **avant une
  échéance**, **lowest-k** créneaux (job divisible), créneaux **sous un seuil**, et
  **annotation d'économie** vs « maintenant » (delta + %, et gCO₂eq absolus si
  l'énergie du job est fournie). Cas d'usage `CarbonAwareScheduler` + endpoints
  `GET /v1/schedule`, `GET /v1/schedule/slots`, `GET /v1/intensity/below`. Posture
  **anonyme/sans état** préservée ; ce sont des conseils sur prévision, **pas du
  pilotage**.
- **Flux live SSE** (ADR-0014, tranche B) : `GET /v1/intensity/stream`
  (`text/event-stream`) pousse un événement `intensity` à chaque mise à jour
  nationale du read-model (cadence du poller), avec filtres optionnels `region`
  et `below=X` et heartbeat keep-alive. Type domaine léger `IntensityUpdate`,
  diffusion par **canal mémoire `tokio::broadcast`** (poller intégré ; migration
  `LISTEN`/`NOTIFY` documentée pour un futur `bin/poller`). **Sans état
  par-client**, anonyme, auto-hébergeable.
- **Compteur de consultation** : `GET /v1/stats` + `POST /v1/stats/visit`
  (port `VisitCounter`). IP **jamais stockée** — empreinte SHA-256 salée
  (`CARBONFR_VISIT_SALT`), déduplication unique par IP/jour ; IP lue via
  `X-Forwarded-For`/`X-Real-IP`.
- **Documentation & gouvernance** : ADR 0001–0009 acceptés (+ addendum ADR-0003),
  ADR 0010–0015 **proposés** (vision forward : `acv-ademe` consumption-based,
  contrat `ForecastPoint`, modèle ML, prévision `acv-ademe`, usage/streaming,
  tier hébergé),
  `ARCHITECTURE.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `GOUVERNANCE.md`,
  et intégration continue GitHub Actions (fmt, clippy, tests + PostgreSQL).
- **Chaîne d'approvisionnement** : politique `cargo-deny` (`deny.toml`) vérifiée
  en CI — licences permissives en liste blanche (compatibles MIT/Apache-2.0),
  avis de sécurité RustSec, et sources de confiance.

### Notes

- `acv-ademe@1` est **basée production** : pour une région importatrice,
  l'intensité reflète la production locale, pas la consommation (imports =
  version consommation, `acv-ademe@2`).
- La prévision (`/forecast`, `/greenest-window`) relève de la phase 3.
