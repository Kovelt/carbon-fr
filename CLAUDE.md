# CLAUDE.md

Contexte et conventions du projet `carbon-fr` pour les sessions Claude Code. Lis ce fichier en début de session avant toute modification.

## Le projet en une phrase

API d'intensité carbone de l'électricité française (gCO₂eq/kWh), souveraine, open source et dev-first — l'équivalent français de [carbonintensity.org.uk](https://carbonintensity.org.uk/), basée sur les données ouvertes RTE/éCO2mix via ODRÉ.

> **Nom** : `carbon-fr` (confirmé, libre sur crates.io). Les crates publiables sont préfixées `carbonfr-*` (ex. `carbonfr-core`, `carbonfr-adapter-odre`) même si les dossiers restent `crates/core`, `crates/adapter-odre`, etc.

## Stack

- **Langage** : Rust (edition 2024), runtime async **tokio**.
- **Architecture** : hexagonale (ports & adapters), workspace Cargo multi-crates.
- **Web** : axum (en place — `adapter-http`, API `/v1`).
- **Base** : PostgreSQL **natif, sans extension** (pas de TimescaleDB — choix de licence, voir ADR-0004).
- **Erreurs** : `thiserror` dans les bibliothèques, `anyhow` toléré uniquement dans le binaire.
- **Ports async** : `async-trait`.

## Structure du workspace

```
carbon-fr/
├── Cargo.toml                # workspace
├── crates/
│   ├── core/                 # lib PURE : domaine + cas d'usage + ports (zéro IO)
│   ├── adapter-odre/         # impl Eco2mixSource (HTTP → ODRÉ)          ✅
│   ├── adapter-postgres/     # impl IntensityRepository (sqlx/Postgres)  ✅
│   └── adapter-http/         # API axum (adapter entrant, /v1)           ✅
└── bin/
    └── server/               # composition root : câble adapters + poller ✅
```

## Règle d'or de l'architecture

**Les dépendances pointent vers l'intérieur. Le domaine ne dépend de rien.**

- `core` ne contient **aucune** IO : pas de `reqwest`, pas de `sqlx`, pas d'`axum`, idéalement pas de `serde`. La (dé)sérialisation et la persistance sont des préoccupations d'**adapters**.
- Le domaine définit des **ports** (traits). Les adapters les **implémentent**.
  - Ports sortants : `Eco2mixSource`, `IntensityRepository`, `ForecastModel`, `Clock`.
  - Ports entrants : les cas d'usage (`GetCurrentIntensity`, `IngestLatest`, `FindGreenestWindow`, …).
- Seul `bin/server` (la *composition root*) connaît les implémentations concrètes et les assemble.

Conséquence pratique : un changement de source de données, de base, ou de modèle de prévision = **un nouvel adapter**, sans toucher au domaine ni à l'API.

## Conventions de code

- Pas d'`unwrap()` / `expect()` hors tests et hors `main` de bootstrap.
- Les cas d'usage sont génériques sur leurs ports (`struct UseCase<R: IntensityRepository>`), dispatch statique, zéro coût.
- Les tests du `core` se font **sans IO**, avec des *fakes* en mémoire implémentant les ports (c'est le bénéfice de l'hexagonal — on le démontre dans les tests).
- `cargo fmt` + `cargo clippy --all-targets -- -D warnings` doivent passer avant tout commit.
- Unité canonique : **gCO₂eq/kWh**. Pas de stépsite quart d'heure implicite — l'horodatage est porté explicitement par chaque mesure.

## Domaine — repères

- **Régions** : `National` + 12 régions métropolitaines (couverture éCO2mix régional).
- **Filières** : nucléaire, gaz, charbon, fioul, hydraulique, éolien, solaire, bioénergies, pompage, échanges.
- **Méthodologie carbone** (ADR-0005, **Accepté**) : base MVP = estimation RTE (`rte-direct`, émissions de la production FR). La méthodologie est un **attribut versionné porté par chaque `Measurement`** (champ `methodology`) — pas une constante globale. Enrichissement engagé : une méthode `acv-ademe` (cycle de vie + imports) coexistera plus tard sans rupture. Toute nouvelle méthode = nouvelle version + nouvel ADR ; **jamais** de modification silencieuse d'une méthode publiée.
- **Millésime / révisions** (ADR-0006) : la donnée RTE est révisée (`tr` → `consolidated` → `definitive`). Chaque `Measurement` porte un champ `vintage`. Clé d'unicité = `(region, horodatage, methodology)`. L'ingestion fait un **upsert conditionnel** : un millésime n'écrase l'existant que s'il est de qualité ≥ (`definitive` > `consolidated` > `tr`). Donc `IntensityRepository` expose un upsert, pas un simple insert. On sert toujours la meilleure version et on expose le millésime.

## À NE PAS faire

- Mettre `serde` / `sqlx` / `axum` dans `core`.
- Faire taper RTE directement à chaque requête utilisateur : un **poller unique** (singleton) alimente la base, l'API sert depuis la base (le quota de 50 000 appels/mois est consommé à moins de 8 % par construction).
- **Backfiller l'historique via l'API paginée** : utiliser l'**export en masse** d'ODRÉ (un téléchargement), sinon on brûle le quota.
- Traiter la donnée comme **append-only** : elle est révisée → upsert conditionnel au millésime (voir repères Domaine + ADR-0006).
- **Exposer l'API sans préfixe de version** : tout endpoint public est sous `/v1` (l'URL est un contrat — ADR-0007).
- Étendre le périmètre méthodologique (cycle de vie, imports) sans ADR.
- Reproduire la donnée RTE comme si elle était nôtre : on re-traite, on cite la source.

## Commandes

```bash
cargo check --workspace
cargo test --workspace                       # hermétique (sans réseau ni base)
cargo clippy --all-targets -- -D warnings
cargo fmt --all

# Lancer l'API (migrations appliquées au démarrage du serveur) :
DATABASE_URL=postgres://localhost/carbonfr cargo run -p server

# Tests d'intégration nécessitant des ressources externes :
DATABASE_URL=postgres://localhost/carbonfr_test \
  cargo test -p carbonfr-adapter-postgres --test pg     # Postgres réel
cargo test -p carbonfr-adapter-odre --test live -- --ignored   # API ODRÉ réelle
```

## Décisions (ADR)

Le « pourquoi » des choix vit dans [`docs/adr/`](docs/adr/). Lire au minimum :
- ADR-0002 (architecture hexagonale + workspace) — cadre tout le reste.
- ADR-0003 (périmètre & source ODRÉ) — d'où vient la donnée, et ce qui n'y est pas.
- ADR-0004 (Postgres natif) — pourquoi pas de TimescaleDB.
- ADR-0005 (méthodologie carbone) — **Accepté** : `rte-direct` en MVP, méthodologie versionnée par mesure, enrichissement `acv-ademe` engagé.
- ADR-0006 (cycle de vie & révision) — **Accepté** : millésime `tr`/`consolidated`/`definitive`, upsert conditionnel sur `(region, horodatage, methodology)`.
- ADR-0007 (déploiement) — **Accepté** : API sur VPS FR/EU (PostgreSQL co-localisé), site statique sur o2switch, sous-domaine Kovelt, API versionnée `/v1`.

## État d'avancement

- [x] Cadrage + documentation (ADR, ARCHITECTURE).
- [x] Phase 1 — socle `core` + adapters ODRÉ/Postgres/HTTP + `bin/server` (poller unique + `/v1/intensity/now`, `/v1/mix`, `/health`). Validé de bout en bout (national).
- [ ] Phase 2 — historique (backfill par export de masse ODRÉ) + régional (intensité **dérivée par un modèle** : `taux_co2` absent du régional, cf. addendum ADR-0003).
- [ ] Phase 3 — prévision.

### Repères d'implémentation (phase 1)

- **Intensité régionale = national-only** à la source : `latest`/`range` de l'adapter ODRÉ renvoient `NoData` pour toute région ≠ `National` (addendum ADR-0003).
- **Millésime stocké en rang `SMALLINT`** (0/1/2) côté Postgres → upsert conditionnel = `WHERE EXCLUDED.vintage_rank >= measurement.vintage_rank`. Mix = 10 colonnes (pas de `serde` dans le `core`).
- **Partitionnement mensuel + BRIN** (ADR-0004) : reporté en phase 2 (table simple au socle, cf. commentaire de la migration `0001`).
- **sqlx en requêtes runtime** (pas les macros `query!`) → `cargo check` reste hermétique, sans base.
- Tests : `core`/adapters hermétiques ; intégration Postgres pilotée par `DATABASE_URL` ; ODRÉ « live » en `--ignored`.
- Serveur configurable par env : `DATABASE_URL`, `CARBONFR_BIND` (déf. `0.0.0.0:8080`), `CARBONFR_POLL_SECS` (déf. 900), `RUST_LOG`.
