# Contribuer à `carbon-fr`

Merci de l'intérêt porté au projet. `carbon-fr` est une API d'intensité carbone française **souveraine, open source et dev-first**. Ces quelques règles visent à garder le code sain, l'architecture nette et les décisions traçables.

En participant, tu acceptes de respecter notre [Code de conduite](CODE_OF_CONDUCT.md).

## Avant de coder

- **Ouvre d'abord une issue** pour discuter de l'idée (bug, fonctionnalité, refactor). On évite ainsi le travail perdu.
- Pour toute **décision structurante** (choix de techno, de découpage, de modèle de données, de méthodologie), on n'improvise pas en code : on rédige un **ADR** dans [`docs/adr/`](docs/adr/) (gabarit fourni). Le « pourquoi » se documente avant le « comment ».

## Architecture — la règle d'or

Le projet suit une **architecture hexagonale** (ports & adapters). Une seule règle, mais non négociable :

> **Les dépendances pointent vers l'intérieur. Le `core` ne dépend de rien.**

Concrètement :

- Le crate `core` ne contient **aucune IO** : pas de `reqwest`, pas de `sqlx`, pas d'`axum`, idéalement pas de `serde`. La (dé)sérialisation et la persistance sont des préoccupations d'**adapters**.
- Le domaine définit des **ports** (traits) ; les adapters les **implémentent**.
- Seul le binaire `server` (composition root) connaît les implémentations concrètes.

Si une contribution fait fuiter de l'infrastructure dans le domaine, elle sera refusée — non par rigidité, mais parce que c'est exactement ce que l'architecture protège. Détails dans [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) et l'ADR-0002.

## Conventions de code

- `cargo fmt --all` et `cargo clippy --all-targets -- -D warnings` doivent passer.
- `cargo test --workspace` doit passer. Le `core` se teste **sans IO**, avec des *fakes* en mémoire implémentant les ports. Les tests Postgres ne s'exécutent qu'avec `DATABASE_URL` : sans elle, ils s'auto-sautent en vert (voir [Tests d'intégration](#tests-dintégration)).
- `cargo deny --all-features --workspace check` doit passer — les mêmes options que la CI, à placer **avant** `check` (licences permissives, avis RustSec, sources de confiance — voir [`deny.toml`](deny.toml)). Toute dépendance normale ou de build à licence inédite force une décision explicite dans `deny.toml` (une crate tirée seulement par des `[dev-dependencies]` échappe au contrôle des licences : à vérifier à la main).
- Pas d'`unwrap()` / `expect()` hors tests et hors bootstrap du binaire.
- Erreurs : `thiserror` dans les bibliothèques ; `anyhow` toléré uniquement dans le binaire.
- Unité canonique : **gCO₂eq/kWh**. L'horodatage est porté explicitement par chaque mesure.
- **Méthodologie carbone** : c'est un attribut **versionné** porté par chaque mesure (voir ADR-0005). On ne modifie **jamais** silencieusement une méthode publiée ; toute nouvelle méthode = nouvelle version + nouvel ADR.

## Tests d'intégration

`cargo test --workspace` est **hermétique par défaut** (ni réseau ni base) : trois familles de tests ne tournent que sur demande — les deux décrites ci-dessous, et les tests du flux SSE de `carbonfr-sdk` (`crates/sdk/src/stream.rs`), compilés seulement avec la feature Cargo `stream` : `cargo test --workspace` ne l'active pas (aucun membre du workspace ne dépend du SDK), mais la CI les joue dans une étape dédiée du job `tests (avec PostgreSQL)` (`cargo test -p carbonfr-sdk --all-features`, serveur `axum` local, aucun réseau). Lance cette même commande dès que tu touches au flux SSE du SDK, ainsi que `cargo clippy -p carbonfr-sdk --all-targets --features stream` : la CI ne passe pas clippy avec cette feature.

**PostgreSQL.** Les tests de [`crates/adapter-postgres/tests/pg.rs`](crates/adapter-postgres/tests/pg.rs) s'exécutent contre un vrai serveur et sont pilotés par la variable `DATABASE_URL`. ⚠️ Sans elle, ils **s'auto-sautent mais comptent comme réussis** (message `SKIP : DATABASE_URL non défini`, visible avec `-- --nocapture`) : un `cargo test --workspace` vert en local ne dit donc rien des requêtes SQL ni des migrations (seuls tournent les tests unitaires hermétiques de `crates/adapter-postgres/src/mapping.rs`). La CI, elle, la définit (job `tests (avec PostgreSQL)`, service `postgres:17-alpine`, même version majeure que la prod) et les exécute pour de bon. **Lance-les en local dès que tu touches `crates/adapter-postgres/`** (requêtes, migrations) ou un port de persistance du `core` :

```bash
# PostgreSQL jetable, configuré comme le service de la CI (la base carbonfr_test est créée au démarrage)
podman run -d --name carbonfr-pg-test \
  -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=carbonfr_test \
  -p 127.0.0.1:5432:5432 docker.io/library/postgres:17-alpine

DATABASE_URL=postgres://postgres:postgres@localhost:5432/carbonfr_test \
  cargo test -p carbonfr-adapter-postgres --test pg

podman rm -f -v carbonfr-pg-test     # supprime le conteneur et son volume (la base jetable)
```

- La **base doit exister** (les tests ne la créent pas) et le rôle doit pouvoir y créer des tables ; les **migrations sont appliquées par les tests eux-mêmes** (`migrate()` de sqlx n'exécute que celles qui manquent, journal `_sqlx_migrations` sous verrou consultatif : relancer les tests ne change pas le schéma ; aucune extension requise — ADR-0004). Sans identifiants dans l'URL, `sqlx` lit `PGUSER`/`PGPASSWORD` ; sans `PGUSER`, il se connecte sous le rôle `anonymous` (et non sous ton nom d'utilisateur système) — mets toujours l'utilisateur dans l'URL.
- **Base jetable uniquement** (jamais celle de dev ni de prod) : les tests tournent en parallèle et s'isolent par des données dédiées (méthodologies `test-pg-*`, horodatages des années 1980, empreintes de clé et identifiants d'abonnement de test), mais certains touchent toute la base (l'un vide la table `visit`, un autre purge tous les abonnements désactivés depuis plus de 30 jours, deux autres réagrègent tous les rollups), et chaque test purge ses mesures au démarrage, pas en fin de run.
- `DATABASE_URL` définie mais base injoignable : les tests **échouent** (ils ne sautent pas). Au premier démarrage, l'image `postgres` (alpine comprise) lance un serveur temporaire joignable par le seul socket Unix du conteneur, le temps de créer `carbonfr_test`, puis redémarre pour de bon en TCP : une sonde passée par ce socket peut répondre « prêt » trop tôt. Attends que la base réponde **en TCP** par le port publié, comme les tests (`pg_isready -h 127.0.0.1 -p 5432 -U postgres` depuis l'hôte, ou un `SELECT 1` par `DATABASE_URL`).

**API réelles (ODRÉ, ENTSO-E, Open-Meteo).** Les tests « live » (`tests/live.rs` d'`adapter-odre`, `adapter-entsoe` et `adapter-meteo`) frappent les vraies API : ils sont `#[ignore]` (réseau ; quota ODRÉ de 50 000 appels/mois par jeu de données — ADR-0003) et la CI ne les lance jamais. À la demande seulement, par exemple `cargo test -p carbonfr-adapter-odre --test live -- --ignored` ; ENTSO-E exige en plus un jeton dans `CARBONFR_ENTSOE_TOKEN`. Le mainteneur rejoue le test ENTSO-E après toute montée de `reqwest` ou de `quick-xml` et tout changement du parseur, et date le rejeu dans `crates/adapter-entsoe/src/lib.rs`.

## Crates publiées / publiables (`carbonfr-core`, `carbonfr-eligibility`, `carbonfr-sdk`)

Ces trois crates — et elles seules, les autres membres du workspace restent
`publish = false` — sont préparées pour crates.io ([ADR-0030](docs/adr/0030-politique-publication-crates-io.md),
[ADR-0031](docs/adr/0031-conception-sdk-rust.md)). `carbonfr-core` et
`carbonfr-eligibility` y sont publiées depuis le 2026-09-24, à chaque tag
`vX.Y.Z` (version du workspace, `release-crates.yml`) ; `carbonfr-sdk` y est
publiée depuis le 2026-09-25 (0.1.0, puis 0.1.1 ; 0.2.0 le 2026-09-27 ; 0.3.0 le 2026-10-20) sur
son **axe de version propre** — `version` dans `crates/sdk/Cargo.toml`, tag
`rust-sdk-vX.Y.Z`, `release-rust-sdk.yml` (addendum ADR-0019, ADR-0031
décision 12). Toute contribution qui les touche doit respecter ces règles
supplémentaires, vérifiées en CI :

- **MSRV `1.88.0`** (`rust-version` du workspace) : plancher imposé par une
  dépendance directe (`time`), pas par l'édition 2024. Ne la relever qu'en
  version **minor**, et seulement quand une dépendance directe l'exige
  réellement — jamais par anticipation.
- **`#[non_exhaustive]`**, décidé enum par enum (ADR-0030 §3) : posé sur les
  catalogues voués à grandir (erreurs, sources de coût, filières, motifs
  d'indétermination…) ; laissé exhaustif sur les ensembles finis par
  construction du domaine (méthodologie à N valeurs fixe, topologie figée…).
  Les structs à champs publics n'en reçoivent **jamais** : elles doivent
  rester constructibles par littéral, y compris dans le SDK Rust
  (`carbonfr-sdk`) — y ajouter un champ public est donc un bump *minor* de la
  crate, jamais un patch (imposé par `cargo-semver-checks` ; ex. `0.2.0`).
- **En 0.x, toute rupture d'API publique de `core`/`eligibility`** (retrait ou
  renommage public, ajout de variante sur un enum exhaustif consommé par
  `match`, ajout de champ public à une struct…) **relève la version *minor*
  du workspace dans la même PR** — jamais un patch. `cargo-semver-checks`
  compare l'API à la baseline du dernier tag de release `vX.Y.Z` et échoue
  sinon : l'outil « assume minor » tant que la version n'a pas bougé. Même
  règle pour `carbonfr-sdk`, contre le dernier tag `rust-sdk-v*` et sa propre
  version. Un ajout **compatible** (méthode de trait à corps par défaut,
  nouvelle fonction, nouveau type) ne demande aucun bump.

Trois jobs CI dédiés (`.github/workflows/ci.yml`), à garder verts avant toute
fusion touchant ces trois crates :

| Job (`name:`) | Vérifie |
|---|---|
| `MSRV (Rust 1.88)` | `cargo check` sur les trois crates (`--all-features` pour `carbonfr-sdk`, sans effet sur core/eligibility qui n'ont aucune feature), toolchain épinglée `1.88.0` (pas `stable`) |
| `semver (crates publiables)` | `cargo-semver-checks` : `carbonfr-core`/`carbonfr-eligibility` contre le dernier tag `v*`, `carbonfr-sdk` contre le dernier tag `rust-sdk-v*` (toutes features), ADR-0030 §4 |
| `rustdoc + package (crates publiables)` | `cargo doc -D warnings` (liens morts) puis empaquetage (`cargo package`, aucun upload) sur les trois crates, `carbonfr-sdk` avec `--all-features` en doc |

Ces trois checks sont **requis** par le *ruleset* de `main` depuis le
2026-09-24 (procédure : [`docs/brief-claude-code-ruleset-main.md`](docs/brief-claude-code-ruleset-main.md)).
Le job `tests (avec PostgreSQL)` joue aussi les tests de la feature `stream`
de `carbonfr-sdk` (`cargo test -p carbonfr-sdk --all-features`) et compile
ses `examples/` (`cargo build -p carbonfr-sdk --examples --all-features`,
depuis I6) — pas de job requis supplémentaire pour ça.

## Processus de contribution

1. Fork + branche dédiée (`feat/…`, `fix/…`, `docs/…`).
2. Commits clairs et articulés (un commit = une intention). Les [Conventional Commits](https://www.conventionalcommits.org/) sont appréciés mais non obligatoires. Les commits **signés** (GPG/SSH) sont **recommandés**, sans être exigés.
3. Ouvre une Pull Request en décrivant le **quoi** et le **pourquoi**, en liant l'issue / l'ADR concerné (un gabarit de PR est proposé automatiquement).
4. La CI doit être **verte sur les huit contrôles** : `fmt + clippy`, `cargo-deny (licences + advisories)`, `tests (avec PostgreSQL)`, `build release (artefact déployable)`, `SDK TypeScript (typecheck + build)` et les trois contrôles des crates publiées (§ ci-dessus : MSRV, semver, rustdoc + package).

## Revue & fusion — `main` est protégée

La branche `main` est verrouillée par un *ruleset* GitHub (voir [ADR-0027](docs/adr/0027-politique-contribution-verrouillage-branche.md)). Concrètement :

- **aucun push direct** sur `main` : tout passe par une Pull Request ;
- **CI verte obligatoire** (les huit contrôles ci-dessus) et **branche à jour** avec `main` (un rebase peut être nécessaire avant fusion) ;
- **conversations résolues** avant fusion ;
- **historique linéaire** : fusion en **squash** ou **rebase** (pas de *merge commit*) ;
- force-push et suppression de `main` interdits ; la règle s'applique **sans exception**, mainteneur compris.

En phase solo, aucune approbation humaine n'est exigée (un mainteneur ne peut pas approuver sa propre PR) : la relecture se fait via le diff. **Dès la première contribution externe**, une approbation du mainteneur (Code Owner, via [`CODEOWNERS`](.github/CODEOWNERS)) deviendra obligatoire — c'est la Phase B d'ADR-0027.

## Langue

La documentation et les ADR sont en **français** (exception : le README et les métadonnées `Cargo.toml` des crates publiées sur crates.io sont en anglais, pour l'audience de crates.io/docs.rs). Les issues et PR peuvent être en français ou en anglais.

## Licence des contributions

`carbon-fr` est distribué sous double licence **`MIT OR Apache-2.0`**.

> Sauf mention contraire explicite de votre part, toute contribution que vous soumettez intentionnellement pour inclusion dans le projet sera **doublement licenciée sous `MIT OR Apache-2.0`**, sans aucune condition supplémentaire — conformément à la section 5 de la licence Apache 2.0.

## Outillage (optionnel)

Le dépôt contient un [`CLAUDE.md`](CLAUDE.md) qui décrit le contexte et les conventions pour les contributeurs utilisant Claude Code. Il n'est pas requis, mais il encode les mêmes règles que ce document.
