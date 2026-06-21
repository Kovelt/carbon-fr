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

## Processus de contribution

1. Fork + branche dédiée (`feat/…`, `fix/…`, `docs/…`).
2. Commits clairs et articulés (un commit = une intention). Les [Conventional Commits](https://www.conventionalcommits.org/) sont appréciés mais non obligatoires. Les commits **signés** (GPG/SSH) sont **recommandés**, sans être exigés.
3. Ouvre une Pull Request en décrivant le **quoi** et le **pourquoi**, en liant l'issue / l'ADR concerné (un gabarit de PR est proposé automatiquement).
4. La CI doit être **verte sur les cinq contrôles** : `fmt + clippy`, `cargo-deny (licences + advisories)`, `tests (avec PostgreSQL)`, `build release` et `SDK TypeScript`.

## Revue & fusion — `main` est protégée

La branche `main` est verrouillée par un *ruleset* GitHub (voir [ADR-0027](docs/adr/0027-politique-contribution-verrouillage-branche.md)). Concrètement :

- **aucun push direct** sur `main` : tout passe par une Pull Request ;
- **CI verte obligatoire** (les cinq contrôles ci-dessus) et **branche à jour** avec `main` (un rebase peut être nécessaire avant fusion) ;
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
