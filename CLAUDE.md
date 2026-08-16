# CLAUDE.md

Contexte et conventions du projet `carbon-fr` pour les sessions Claude Code. Lis ce fichier en début de session avant toute modification.

## Le projet en une phrase

API d'intensité carbone de l'électricité française (gCO₂eq/kWh), souveraine, open source et dev-first — l'équivalent français de [carbonintensity.org.uk](https://carbonintensity.org.uk/), basée sur les données ouvertes RTE/éCO2mix via ODRÉ.

> **Nom** : `carbon-fr`. Les crates publiables sont préfixées `carbonfr-*` (ex. `carbonfr-core`, `carbonfr-adapter-odre`) même si les dossiers restent `crates/core`, `crates/adapter-odre`, etc.

## Où trouver quoi (sources faisant foi)

- **Décisions & pourquoi** : [`docs/adr/README.md`](docs/adr/README.md) = index complet n°/titre/statut. Chaque ADR est autoportant (statut en tête, décisions, mesures, verdicts de GATE, addenda). Lire au minimum 0002 (hexagonal), 0003 (source ODRÉ), 0005/0006 (méthodologie & millésime) avant de toucher au domaine.
- **Architecture, ports, crates** : [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) (§4 ports entrants/sortants, §7 crates). Membres du workspace = `Cargo.toml` racine (**autoritatif** — README/ARCHITECTURE peuvent dériver, ne pas s'y fier pour les états).
- **État d'avancement réel** : `CHANGELOG.md` (le plus fin), `README.md` §Feuille de route, [`docs/roadmap-hydrogene.md`](docs/roadmap-hydrogene.md) (phases H0-H7 + déclencheurs).
- **Sous-commandes du serveur & variables d'env** : docstring de `bin/server/src/main.rs` (tableau complet Variable | Défaut | Rôle) + `.env.example` ; liste des sous-commandes aussi dans `docs/ARCHITECTURE.md`.
- **Conventions de code** : `CONTRIBUTING.md` (pas d'`unwrap`/`expect` hors tests+bootstrap, `thiserror` en lib / `anyhow` en bin, unité canonique gCO₂eq/kWh, cas d'usage génériques sur leurs ports).
- **CI & gouvernance** : les 5 jobs de `.github/workflows/ci.yml` (fmt, clippy `-D warnings`, cargo-deny, tests, build release + SDK TS) doivent passer ; `main` protégée par ruleset, **zéro push direct, zéro bypass** (ADR-0027) — tout passe par PR.
- **Déploiement prod** : procédure générale dans `deploy/README.md` §2 (compose + Traefik, image GHCR épinglée). ⚠️ L'accès au VPS n'est **jamais** dans le dépôt (public) : il vit dans la **mémoire de session locale** (`prod-vps-kovelt-acces`) — la lire AVANT toute connexion, ne jamais deviner/sonder des identifiants SSH.

## Règle d'or de l'architecture

**Les dépendances pointent vers l'intérieur. Le domaine ne dépend de rien.** `core` et `eligibility` sont des libs PURES : aucune IO, pas de `reqwest`/`sqlx`/`axum`, idéalement pas de `serde`. Le domaine définit des **ports** (traits) ; les adapters les implémentent ; seul `bin/server` (composition root) connaît les implémentations et les assemble. Tests du `core` **sans IO**, avec des fakes en mémoire. Changement de source, de base ou de modèle = **un nouvel adapter**, sans toucher domaine ni API.

## À NE PAS faire

- Mettre `serde` / `sqlx` / `axum` dans `core`.
- Faire taper RTE directement à chaque requête utilisateur : un **poller unique** (singleton) alimente la base, l'API sert depuis la base (le quota de 50 000 appels/mois est consommé à moins de 8 % par construction).
- **Backfiller l'historique via l'API paginée** : utiliser l'**export en masse** d'ODRÉ (un téléchargement), sinon on brûle le quota.
- Traiter la donnée comme **append-only** : elle est révisée → upsert conditionnel au millésime (ADR-0006).
- **Exposer l'API sans préfixe de version** : tout endpoint public est sous `/v1` (l'URL est un contrat — ADR-0007).
- Étendre le périmètre méthodologique (cycle de vie, imports) sans ADR.
- Modifier une méthodologie ou un modèle publiés : versions **portées par la donnée** (`rte-direct`, `acv-ademe@2`, `climatology@1`…), **immuables** — nouvelle méthode = nouvelle version + nouvel ADR.
- Reproduire la donnée RTE comme si elle était nôtre : on re-traite, on cite la source.

## Contraintes actives & pièges

- **Non servi / écarté — ne pas « activer » sans re-jouer les gates** : `gbdt@1` (perd contre `climatology@1` au backtest, ADR-0012), `share-meteo@2` (expérience gatée, non servie, ADR-0028), prévision d'intensité météo-pilotée (écartée, ADR-0018), ruleset `rfnbo:2026-revision` (`planned`, texte UE non adopté).
- ⚠️ Backtests sur le jeu consolidé : il est au **pas 30 min** (`CARBONFR_BACKTEST_STEP_MINUTES`).
- ⚠️ `TD_LOSS_FACTOR_V1 = 0,072` reste à sourcer précisément (RTE/ADEME) avant publication pleine de `acv-ademe@2` ; chemins XML ENTSO-E calés sur le guide, **à valider live** (`cargo test -p carbonfr-adapter-entsoe --test live -- --ignored`, `CARBONFR_ENTSOE_TOKEN`).
- ⚠️ Éligibilité électrolyseur (ADR-0025/0026) : `bidding_zone` toujours `FR` (zone de dépôt nationale, jamais les 12 régions) ; prix indéterminé au-delà du day-ahead, jamais extrapolé ; **jamais d'éligibilité par site** (garde-fou testé).
- **Fraîcheur bornée** (audit 2026-08) : jointures « au plus proche ≤ » plafonnées — `MAX_FLOW_CONTEXT_AGE` (1 h, imports) et `MAX_SPOT_STALENESS` (6 h, spot) sont des constantes de domaine ; au-delà, créneau omis / 404. Ne pas réintroduire de report non borné.
- **Versionnement (ADR-0019), 4 axes découplés** : version applicative = SemVer unique de workspace (`Cargo.toml`, crates non publiées sur crates.io, distribution = image Docker GHCR `ghcr.io/kovelt/carbon-fr`) ≠ contrat d'API `/v1` ≠ versions méthodologies/modèles ≠ SDK (`@carbon-fr/sdk`, tags `sdk-v*`). Release = `git tag vX.Y.Z` aligné sur la version du workspace (garde-fou CI) ; en prod, épingler une version exacte.

## Commandes

```bash
cargo check --workspace && cargo test --workspace     # hermétique (sans réseau ni base)
cargo clippy --all-targets -- -D warnings && cargo fmt --all
cargo deny check                                      # licences + advisories RustSec (deny.toml)

DATABASE_URL=postgres://localhost/carbonfr cargo run -p server        # API ; sous-commandes (backfill, backtest*, train, mint-key…) : cf. docstring main.rs

# Intégration nécessitant des ressources externes :
DATABASE_URL=postgres://localhost/carbonfr_test cargo test -p carbonfr-adapter-postgres --test pg   # Postgres réel
cargo test -p carbonfr-adapter-odre --test live -- --ignored                                        # API ODRÉ réelle
```

## Repères d'implémentation

- **`rte-direct` = national-only** (taux_co2 publié seulement au national, addendum ADR-0003). Le **régional** est servi en **`acv-ademe`** : `latest`/`range` de l'adapter ODRÉ, pour une région, lisent le mix régional (`eco2mix-regional-tr`, refine `code_insee_region`) et dérivent l'intensité. ⚠️ `pompage` y est typé **chaîne** (`"0"`) → non décodé.
- **Millésime stocké en rang `SMALLINT`** (0/1/2) côté Postgres → upsert conditionnel = `WHERE EXCLUDED.vintage_rank >= measurement.vintage_rank`. Mix = 10 colonnes (pas de `serde` dans le `core`).
- **`upsert_many` = INSERT multi-lignes** (`QueryBuilder`, paquets de 1000) + **dédup par clé** (`dedup_by_key`, garde le meilleur millésime) — obligatoire pour le volume du backfill (~494k lignes).
- **Backfill** : port `Eco2mixArchive` (export de masse, dataset `eco2mix-national-cons-def`), cas d'usage `BackfillHistory` qui **découpe en tranches** (une tranche = un export, pas l'API paginée — ADR-0003). Jamais de backfill via `range()` (plafonné).
- **Rollups** : `measurement_rollup_{hourly,daily}`, seaux `date_trunc(..., 'UTC')`. **D'abord** des vues matérialisées (migration `0002`, index unique requis par `REFRESH … CONCURRENTLY`), **désormais de vraies tables incrémentales** upsertées par seau (migration `0010`, lecture inchangée ; index BRIN `measurement(at)` par la migration `0012`). Le **résumé** `/v1/intensity/stats` est exact (agrégat sur `measurement`) ; la **série** (`interval=`) vient des rollups. Rafraîchies par le poller (si `written > 0`) et en fin de backfill.
- **`acv-ademe`** : facteurs ACV versionnés en **constante de domaine** (`EmissionFactors::acv_ademe_v1`, ADR-0008), calcul pur `acv_ademe_intensity` + `derive_acv_ademe`. Dérivée et **stockée à l'ingestion** (poller + backfill) au même horodatage/millésime ; servie via `?methodology=acv-ademe`. **National + 12 régions** (le mix régional agrège le fossile en `thermique` → `GenerationMix.thermique: Option`, facteur gaz). **Basée production** : pour une région importatrice, reflète la production locale, pas la conso (imports = `acv-ademe@2`).
- **Partitionnement mensuel + BRIN** (ADR-0004) : toujours reporté (table simple, cf. commentaire de la migration `0001`). À reconsidérer maintenant que l'historique complet est ingérable.
- **OpenAPI code-first** (`utoipa`) : `ToSchema` sur les **DTO de l'adapter HTTP** uniquement (jamais le `core`), `#[utoipa::path]` sur les handlers (fonctionne malgré la généricité), `ApiDoc` dans `carbonfr_openapi.rs` → `/v1/openapi.json` + Swagger UI `/docs`. Collection **Bruno** dans `bruno/` (cf. [[dx-openapi-bruno]]).
- **Compteur de visiteurs** : port `VisitCounter` (`/v1/stats`, `POST /v1/stats/visit`). **IP jamais stockée** — empreinte SHA-256 salée (`CARBONFR_VISIT_SALT`, défaut `carbon-fr` à surcharger en prod), dédup `(ip_hash, jour)`. IP lue via `X-Forwarded-For`/`X-Real-IP` (derrière le proxy ; pas de `ConnectInfo` car `Option<ConnectInfo>` n'est pas un extracteur axum 0.8).
- **sqlx en requêtes runtime** (pas les macros `query!`) → `cargo check` reste hermétique, sans base.
- Tests : `core`/adapters hermétiques ; intégration Postgres pilotée par `DATABASE_URL` ; ODRÉ « live » en `--ignored`. ⚠️ postgres-alpine se relance pendant son init → attendre une vraie requête SQL stable avant de lancer les tests (pas seulement `pg_isready`).
