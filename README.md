<div align="center">

# carbon-fr

**L'API d'intensité carbone de l'électricité française — souveraine, open source et _dev-first_.**

L'équivalent français de [carbonintensity.org.uk](https://carbonintensity.org.uk/), bâti sur les données ouvertes RTE / éCO2mix via [ODRÉ](https://odre.opendatasoft.com/).

[![Release](https://img.shields.io/github/v/release/Kovelt/carbon-fr?label=release&color=brightgreen)](https://github.com/Kovelt/carbon-fr/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#licence)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange.svg)](https://www.rust-lang.org/)
[![Statut](https://img.shields.io/badge/statut-en%20production-brightgreen.svg)](#feuille-de-route)
[![Architecture](https://img.shields.io/badge/architecture-hexagonale-6f42c1.svg)](docs/ARCHITECTURE.md)

</div>

---

## Pourquoi `carbon-fr` ?

L'intensité carbone du réseau électrique français (en **gCO₂eq/kWh**) est une donnée publique précieuse — pour décaler une recharge de véhicule, planifier un batch de calcul, ou afficher l'empreinte d'un service. Mais la source officielle reste **brute, plafonnée en appels, et sans prévision d'intensité**.

`carbon-fr` la rend **directement consommable par des développeurs et des machines** :

- 🇫🇷 **Souverain & auto-hébergeable** — aucune dépendance propriétaire obligatoire, licences OSI.
- 🚀 **Dev-first** — API REST lisible et versionnée (`/v1`), **OpenAPI 3.1** + Swagger UI servis, collection Bruno, **SDK TypeScript** ([`sdk/typescript/`](sdk/typescript/)).
- 🛡️ **Résilient au quota** — un poller unique alimente la base ; l'API sert tous les clients depuis ce read-model, à **moins de 8 % du quota** RTE.
- 🔬 **Méthodologie versionnée** — chaque mesure porte sa méthode de calcul (`rte-direct` et `acv-ademe` — cycle de vie production **et** consommation), jamais de changement silencieux.
- 🔮 **Prévision comme valeur ajoutée** — l'intensité prévisionnelle n'existe pas à la source : `carbon-fr` la modélise derrière un port dédié (`climatology@1`, gardée par backtest).
- ⏱️ **Carbon-aware & live** — primitives de scheduling (créneau sous échéance, *lowest-k*, seuil, économie), flux **SSE**, et **webhooks** signés (sur clé API).

## Utiliser l'API

**Aucune installation** : l'instance hébergée répond tout de suite. L'intensité carbone courante du réseau national, en une requête :

```bash
curl https://carbon-fr-api.kovelt.fr/v1/intensity/now
```

```json
{
  "region": "national",
  "timestamp": "2026-06-17T06:30:00Z",
  "intensity": { "value": 20.0, "unit": "gCO2eq/kWh" },
  "methodology": "rte-direct",
  "methodology_version": 1,
  "vintage": "tr"
}
```

Ajoute `?region=<slug>` pour l'une des 12 régions, ou `?methodology=acv-ademe` pour le cycle de vie (cf. [Fonctionnalités](#fonctionnalités)).

🌐 **Dans le navigateur** — explore et essaie tous les endpoints depuis la **Swagger UI** : **[carbon-fr-api.kovelt.fr/docs](https://carbon-fr-api.kovelt.fr/docs)**.

📦 **En TypeScript / JavaScript** — le SDK officiel ([`@carbon-fr/sdk`](sdk/typescript/), zéro dépendance runtime) :

```bash
npm install @carbon-fr/sdk
```

```ts
import { CarbonFr } from "@carbon-fr/sdk";

const cf = new CarbonFr(); // instance hébergée par défaut
const now = await cf.intensityNow();
console.log(now.intensity.value, now.intensity.unit); // 20 gCO2eq/kWh
```

<!-- TODO: capture /docs -->

## Fonctionnalités

| Endpoint | Nature | Statut |
| --- | --- | --- |
| `GET /v1/intensity/now` | Intensité courante (national + 12 régions) | ✅ |
| `GET /v1/mix` | Mix de production par filière | ✅ |
| `GET /v1/exchanges` | Échanges transfrontaliers par frontière (flux signé + carbone du voisin, ENTSO-E) | ✅ |
| `GET /v1/exchanges/date?from=&to=` | Série historique des échanges transfrontaliers | ✅ |
| `GET /v1/weather` · `/weather/date` | Météo nationale (vent 100 m + irradiance, Open-Meteo CC-BY) | ✅ |
| `GET /v1/renewable` | Production renouvelable **estimée** depuis la météo + facteur de charge (ADR-0018) | ✅ |
| `GET /v1/intensity/date?from=&to=` | Historique sur un intervalle (révisé/consolidé/définitif) | ✅ |
| `GET /v1/intensity/stats?from=&to=[&interval=hour\|day]` | Résumé (moyenne/min/max) + série agrégée | ✅ |
| `GET /v1/intensity/forecast` | Prévision d'intensité (`climatology@1`) | ✅ |
| `GET /v1/intensity/greenest-window` | Créneau le plus bas-carbone | ✅ |
| `GET /v1/schedule` · `/schedule/slots` · `/intensity/below` | Scheduling carbon-aware (échéance, *lowest-k*, seuil + économie) | ✅ |
| `GET /v1/intensity/stream` | Flux **live** (Server-Sent Events) | ✅ |
| `GET /v1/methodologies` · `/factors` | Catalogue des méthodes + table des facteurs (vérifiabilité) | ✅ |
| `POST`/`GET`/`DELETE /v1/webhooks` | Abonnements webhook signés (clé API requise) | ✅ |

Tous les endpoints `/v1` acceptent `?region=<slug>` (national par défaut) et `?methodology=<id>` : **`rte-direct`** (estimation RTE, combustion directe — défaut) ou **`acv-ademe`** (cycle de vie ADEME, ADR-0008).

La spécification **OpenAPI 3.1** (dérivée du code via `utoipa`) est servie sous **`GET /v1/openapi.json`**, et une **Swagger UI** sous **`GET /docs`**. Une collection **[Bruno](https://www.usebruno.com/)** versionnée (dossier [`bruno/`](bruno/)) couvre tous les endpoints (cas nominaux + erreurs).

Un compteur de consultation sobre est exposé (**`GET /v1/stats`**, **`POST /v1/stats/visit`**) : l'IP n'est **jamais** stockée — seule une empreinte **SHA-256 salée** sert à dédupliquer (unique par IP/jour), RGPD-friendly.

> Couverture **National + 12 régions métropolitaines**. Le `taux_co2` publié par RTE (`rte-direct`) n'existe qu'au national ; l'intensité **régionale** est dérivée via `acv-ademe` (cycle de vie appliqué au mix régional, ADR-0008). `acv-ademe@1` est **basée production** ; `acv-ademe@2` **consumption-based** (imports valorisés à l'intensité du voisin via ENTSO-E + pertes T&D, ADR-0010) est servie au national via `?methodology=acv-ademe&version=2`.

## Architecture

`carbon-fr` suit une **architecture hexagonale** (ports & adapters) stricte : le domaine ne dépend de rien, les dépendances pointent vers l'intérieur. Changer de source de données, de base, ou de modèle de prévision = **un nouvel adapter**, sans toucher au cœur métier.

```
        Adapters entrants                       Adapters sortants
       (API axum, CLI…)                  (ODRÉ · PostgreSQL · ForecastModel)
              │                                          ▲
              ▼   appelle les cas d'usage                │  implémentent les ports
        ┌───────────────────────────────────────────────────────────┐
        │                      core  (lib pure, zéro IO)             │
        │   application/  cas d'usage      ports/  traits sortants   │
        │   domain/       intensité, régions, mesures, méthodologie  │
        └───────────────────────────────────────────────────────────┘
                              ▲  assemble tout
                       bin/server (composition root)
```

Le détail — vision, contraintes, modèle de données, quota — vit dans **[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)**. Le « pourquoi » des choix structurants est tracé dans les **[ADR](docs/adr/)**.

## Structure du workspace

```
carbon-fr/
├── Cargo.toml                  # workspace Cargo
├── crates/
│   ├── core/                   # ✅ domaine + cas d'usage + ports (lib PURE, zéro IO)
│   ├── adapter-odre/           # ✅ impl Eco2mixSource/Eco2mixArchive (ODRÉ)
│   ├── adapter-postgres/       # ✅ impl repositories (sqlx/Postgres)
│   ├── adapter-http/           # ✅ API axum + OpenAPI + auth + SSE (adapter entrant)
│   ├── adapter-forecast/       # ✅ impl ForecastModel (climatology@1, acv-ademe@2)
│   ├── adapter-meteo/          # ✅ impl WeatherForecastSource (Open-Meteo)
│   ├── adapter-entsoe/         # ✅ impl CrossBorderSource (ENTSO-E)
│   ├── adapter-webhook/        # ✅ impl Notifier (livraison signée, anti-SSRF)
│   └── adapter-gbdt/           # ✅ impl ForecastModel ML (GBDT)
├── bin/
│   └── server/                 # ✅ composition root : adapters + poller
├── bruno/                      # collection Bruno (requêtes .bru versionnées)
├── sdk/typescript/             # SDK client TypeScript (@carbon-fr/sdk)
├── deploy/                     # Caddyfile (reverse proxy TLS) + unité systemd
├── Dockerfile                  # image de prod multi-stage
├── .env.example                # variables d'environnement documentées
└── docs/
    ├── ARCHITECTURE.md
    └── adr/                    # Architecture Decision Records
```

## Développer / contribuer

Pour travailler sur `carbon-fr` lui-même (et non simplement consommer l'API). Prérequis : [Rust](https://www.rust-lang.org/tools/install) (edition 2024, `cargo` ≥ 1.85).

```bash
git clone git@github.com:Kovelt/carbon-fr.git
cd carbon-fr

cargo check --workspace        # compile tout le workspace
cargo test  --workspace        # lance les tests (le core se teste sans IO)
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Le crate `core` se teste **entièrement en mémoire**, avec des _fakes_ implémentant les ports — c'est le bénéfice direct de l'hexagonal (voir [`crates/core/tests/use_cases.rs`](crates/core/tests/use_cases.rs)).

## Déploiement

Image de production via le [`Dockerfile`](Dockerfile) multi-stage (binaire `--release`, runtime Debian slim, utilisateur non-root). En bare-metal, une unité systemd ([`deploy/carbonfr.service`](deploy/carbonfr.service)) avec `Restart=on-failure`. Dans les deux cas, placer l'API **derrière un reverse proxy TLS** ([`deploy/Caddyfile`](deploy/Caddyfile)) et activer `CARBONFR_TRUST_PROXY=1`.

```bash
docker build -t carbon-fr .
docker run -e DATABASE_URL=postgres://… -e CARBONFR_VISIT_SALT=… -p 8080:8080 carbon-fr
```

Configuration via variables d'environnement — voir [`.env.example`](.env.example). Sondes : `GET /health` (liveness) et `GET /health/ready` (vérifie la base). Les migrations sont appliquées au démarrage.

## Méthodologie & données

- **Unité canonique** : gCO₂eq/kWh.
- **Périmètre** : `National` + 12 régions métropolitaines (couverture éCO2mix régional).
- **Méthodologie MVP** : `rte-direct` — reprise de l'estimation RTE (émissions de la production française), directement comparable à éCO2mix. Versionnée et portée par chaque mesure (ADR-0005).
- **Révisions** : la donnée RTE est révisée (`tr` → `consolidated` → `definitive`). L'ingestion fait un **upsert conditionnel au millésime** : on sert toujours la meilleure version (ADR-0006).
- **Source citée, jamais appropriée** : `carbon-fr` re-traite et cite RTE/ODRÉ, il ne s'y substitue pas.

> Données amont : [RTE éCO2mix](https://www.rte-france.com/eco2mix), publiées en open data via [ODRÉ](https://odre.opendatasoft.com/) sous licence ouverte.

## Feuille de route

- [x] **Cadrage** — ADR, architecture, modèle de domaine.
- [x] **Phase 1 — Socle** : `core` · adapters ODRÉ / Postgres / HTTP · poller · `/intensity/now` + `/mix` (national).
- [x] **Phase 2 — Historique & régional** : backfill par export de masse · `/intensity/date` · rollups + `/intensity/stats` · régional via `acv-ademe` (12 régions) · OpenAPI 3.1 + Bruno.
- [x] **Phase 3 — Prévision** : `climatology@1` (backtest, calibration des intervalles) → `/forecast` + `/greenest-window`.
- [x] **Phase 4 — Enrichissement & usage** : `acv-ademe@2` consumption-based (ENTSO-E) · prévision `acv-ademe` · scheduling carbon-aware + SSE · clés API + quota · webhooks signés. *(ML GBDT exploré, gardé par backtest ; raffinements ouverts.)*
- [x] **Phase 5 — Enrichissement, déploiement & SDK** : échanges transfrontaliers (`/v1/exchanges`), météo (`/v1/weather`), dérivation renouvelable (`/v1/renewable`) ; **déployé** sur VPS FR/EU (Traefik + PostgreSQL) ; **SDK TypeScript** (`@carbon-fr/sdk`).
- [ ] **À venir** : SDK Rust ; site statique (o2switch) ; `UsageMeter` persistant.

## Contribuer

Les contributions sont les bienvenues — lire d'abord **[CONTRIBUTING.md](CONTRIBUTING.md)** et les conventions de code dans **[CLAUDE.md](CLAUDE.md)**. En résumé : `cargo fmt` + `cargo clippy -D warnings` doivent passer, le `core` reste sans IO, et toute décision structurante passe par un ADR.

## Licence

Distribué sous licence **MIT OU Apache 2.0**, au choix.

- [LICENSE-MIT](LICENSE-MIT)
- [LICENSE-APACHE](LICENSE-APACHE)

Sauf mention contraire explicite, toute contribution soumise pour inclusion dans ce dépôt — telle que définie par la licence Apache 2.0 — sera distribuée sous cette double licence, sans condition supplémentaire.

---

<div align="center">
Un projet <a href="https://kovelt.fr">Kovelt</a>.
</div>
