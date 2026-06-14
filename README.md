<div align="center">

# carbon-fr

**L'API d'intensité carbone de l'électricité française — souveraine, open source et _dev-first_.**

L'équivalent français de [carbonintensity.org.uk](https://carbonintensity.org.uk/), bâti sur les données ouvertes RTE / éCO2mix via [ODRÉ](https://odre.opendatasoft.com/).

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#licence)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange.svg)](https://www.rust-lang.org/)
[![Statut](https://img.shields.io/badge/statut-phase%201%20(socle)-yellow.svg)](#feuille-de-route)
[![Architecture](https://img.shields.io/badge/architecture-hexagonale-6f42c1.svg)](docs/ARCHITECTURE.md)

</div>

---

## Pourquoi `carbon-fr` ?

L'intensité carbone du réseau électrique français (en **gCO₂eq/kWh**) est une donnée publique précieuse — pour décaler une recharge de véhicule, planifier un batch de calcul, ou afficher l'empreinte d'un service. Mais la source officielle reste **brute, plafonnée en appels, et sans prévision d'intensité**.

`carbon-fr` la rend **directement consommable par des développeurs et des machines** :

- 🇫🇷 **Souverain & auto-hébergeable** — aucune dépendance propriétaire obligatoire, licences OSI.
- 🚀 **Dev-first** — API REST lisible et versionnée (`/v1`), index humainement interprétable, OpenAPI et SDK prévus.
- 🛡️ **Résilient au quota** — un poller unique alimente la base ; l'API sert tous les clients depuis ce read-model, à **moins de 8 % du quota** RTE.
- 🔬 **Méthodologie versionnée** — chaque mesure porte sa méthode de calcul (`rte-direct` aujourd'hui, `acv-ademe` à venir), jamais de changement silencieux.
- 🔮 **Prévision comme valeur ajoutée** — l'intensité prévisionnelle n'existe pas à la source : `carbon-fr` la modélise derrière un port dédié.

## Fonctionnalités (cibles)

| Endpoint | Nature | Statut |
| --- | --- | --- |
| `GET /v1/intensity/now` | Intensité courante (national) | ✅ |
| `GET /v1/mix` | Mix de production par filière | ✅ |
| `GET /v1/intensity/date?from=&to=` | Historique sur un intervalle (révisé/consolidé/définitif) | ✅ |
| `GET /v1/intensity/stats?from=&to=[&interval=hour\|day]` | Résumé (moyenne/min/max) + série agrégée | ✅ |
| `GET /v1/intensity/forecast` | Prévision d'intensité | 📅 Phase 3 |
| `GET /v1/greenest-window` | Créneau le plus bas-carbone | 📅 Phase 3 |

> Couverture **nationale** pour l'instant. L'intensité **régionale** n'est pas publiée par la source : elle sera **dérivée par un modèle** (addendum ADR-0003), à venir.

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
│   ├── adapter-odre/           # 📅 impl Eco2mixSource (HTTP → ODRÉ)
│   ├── adapter-postgres/       # 📅 impl IntensityRepository (sqlx)
│   └── adapter-http/           # 📅 API axum (adapter entrant)
├── bin/
│   └── server/                 # 📅 composition root : câble les adapters
└── docs/
    ├── ARCHITECTURE.md
    └── adr/                    # Architecture Decision Records
```

## Démarrage rapide

Prérequis : [Rust](https://www.rust-lang.org/tools/install) (edition 2024, `cargo` ≥ 1.85).

```bash
git clone git@github.com:Kovelt/carbon-fr.git
cd carbon-fr

cargo check --workspace        # compile tout le workspace
cargo test  --workspace        # lance les tests (le core se teste sans IO)
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Le crate `core` se teste **entièrement en mémoire**, avec des _fakes_ implémentant les ports — c'est le bénéfice direct de l'hexagonal (voir [`crates/core/tests/use_cases.rs`](crates/core/tests/use_cases.rs)).

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
- [ ] **Phase 2 — Historique & régional** : backfill par export de masse ✅ · `/intensity/date` ✅ · rollups + `/intensity/stats` ✅ · régional (modèle) à venir.
- [ ] **Phase 3 — Prévision** : `ForecastModel` → `/forecast` + `/greenest-window`.
- [ ] **Phase 4 — DX** : SDK (Rust + TS), OpenAPI, conteneur Docker.

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
