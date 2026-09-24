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
- `cargo test --workspace` doit passer. Le `core` se teste **sans IO**, avec des *fakes* en mémoire implémentant les ports.
- `cargo deny check` doit passer (licences permissives, avis RustSec, sources de confiance — voir [`deny.toml`](deny.toml)). Toute dépendance à licence inédite force une décision explicite dans `deny.toml`.
- Pas d'`unwrap()` / `expect()` hors tests et hors bootstrap du binaire.
- Erreurs : `thiserror` dans les bibliothèques ; `anyhow` toléré uniquement dans le binaire.
- Unité canonique : **gCO₂eq/kWh**. L'horodatage est porté explicitement par chaque mesure.
- **Méthodologie carbone** : c'est un attribut **versionné** porté par chaque mesure (voir ADR-0005). On ne modifie **jamais** silencieusement une méthode publiée ; toute nouvelle méthode = nouvelle version + nouvel ADR.

## Crates publiées (`carbonfr-core`, `carbonfr-eligibility`)

Ces deux crates — et elles seules, les autres membres du workspace restent
`publish = false` — sont préparées pour crates.io ([ADR-0030](docs/adr/0030-politique-publication-crates-io.md)).
Toute contribution qui les touche doit respecter ces règles supplémentaires,
vérifiées en CI :

- **MSRV `1.88.0`** (`rust-version` du workspace) : plancher imposé par une
  dépendance directe (`time`), pas par l'édition 2024. Ne la relever qu'en
  version **minor**, et seulement quand une dépendance directe l'exige
  réellement — jamais par anticipation.
- **`#[non_exhaustive]`**, décidé enum par enum (ADR-0030 §3) : posé sur les
  catalogues voués à grandir (erreurs, sources de coût, filières, motifs
  d'indétermination…) ; laissé exhaustif sur les ensembles finis par
  construction du domaine (méthodologie à N valeurs fixe, topologie figée…).
  Les structs à champs publics n'en reçoivent **jamais** : elles doivent
  rester constructibles par littéral, y compris pour le futur SDK Rust.
- **En 0.x, toute rupture d'API publique de `core`/`eligibility`** (retrait ou
  renommage public, ajout de variante sur un enum exhaustif consommé par
  `match`, ajout de champ public à une struct…) **relève la version *minor*
  du workspace dans la même PR** — jamais un patch. `cargo-semver-checks`
  compare l'API à la baseline du dernier tag de release `vX.Y.Z` et échoue
  sinon : l'outil « assume minor » tant que la version n'a pas bougé.

Trois jobs CI dédiés (`.github/workflows/ci.yml`), à garder verts avant toute
fusion touchant ces deux crates :

| Job (`name:`) | Vérifie |
|---|---|
| `MSRV (Rust 1.88)` | `cargo check` sur les deux crates, toolchain épinglée `1.88.0` (pas `stable`) |
| `semver (crates publiables)` | `cargo-semver-checks` contre le dernier tag `v*` (ADR-0030 §4) |
| `rustdoc + package (crates publiables)` | `cargo doc -D warnings` (liens morts) puis empaquetage croisé des deux crates (`cargo package`, aucun upload) |

Ces trois checks sont **requis** par le *ruleset* de `main` depuis le
2026-09-24 (procédure : [`docs/brief-claude-code-ruleset-main.md`](docs/brief-claude-code-ruleset-main.md)).

## Processus de contribution

1. Fork + branche dédiée (`feat/…`, `fix/…`, `docs/…`).
2. Commits clairs et articulés (un commit = une intention). Les [Conventional Commits](https://www.conventionalcommits.org/) sont appréciés mais non obligatoires. Les commits **signés** (GPG/SSH) sont **recommandés**, sans être exigés.
3. Ouvre une Pull Request en décrivant le **quoi** et le **pourquoi**, en liant l'issue / l'ADR concerné (un gabarit de PR est proposé automatiquement).
4. La CI doit être **verte sur les huit contrôles** : `fmt + clippy`, `cargo-deny (licences + advisories)`, `tests (avec PostgreSQL)`, `build release`, `SDK TypeScript` et les trois contrôles des crates publiées (§ ci-dessus : MSRV, semver, rustdoc + package).

## Revue & fusion — `main` est protégée

La branche `main` est verrouillée par un *ruleset* GitHub (voir [ADR-0027](docs/adr/0027-politique-contribution-verrouillage-branche.md)). Concrètement :

- **aucun push direct** sur `main` : tout passe par une Pull Request ;
- **CI verte obligatoire** (les huit contrôles ci-dessus) et **branche à jour** avec `main` (un rebase peut être nécessaire avant fusion) ;
- **conversations résolues** avant fusion ;
- **historique linéaire** : fusion en **squash** ou **rebase** (pas de *merge commit*) ;
- force-push et suppression de `main` interdits ; la règle s'applique **sans exception**, mainteneur compris.

En phase solo, aucune approbation humaine n'est exigée (un mainteneur ne peut pas approuver sa propre PR) : la relecture se fait via le diff. **Dès la première contribution externe**, une approbation du mainteneur (Code Owner, via [`CODEOWNERS`](.github/CODEOWNERS)) deviendra obligatoire — c'est la Phase B d'ADR-0027.

## Langue

La documentation et les ADR sont en **français**. Les issues et PR peuvent être en français ou en anglais.

## Licence des contributions

`carbon-fr` est distribué sous double licence **`MIT OR Apache-2.0`**.

> Sauf mention contraire explicite de votre part, toute contribution que vous soumettez intentionnellement pour inclusion dans le projet sera **doublement licenciée sous `MIT OR Apache-2.0`**, sans aucune condition supplémentaire — conformément à la section 5 de la licence Apache 2.0.

## Outillage (optionnel)

Le dépôt contient un [`CLAUDE.md`](CLAUDE.md) qui décrit le contexte et les conventions pour les contributeurs utilisant Claude Code. Il n'est pas requis, mais il encode les mêmes règles que ce document.
