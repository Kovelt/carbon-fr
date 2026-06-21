# Gouvernance & workflow du projet

Comment `carbon-fr` est géré au quotidien : protection de la branche principale, cycle de contribution, intégration continue, versionnage et fichiers de gouvernance. Ce document s'adresse au mainteneur (toi) autant qu'aux futurs contributeurs ; les conventions de code, elles, vivent dans [CONTRIBUTING.md](CONTRIBUTING.md).

## 1. Principe directeur

`main` est la **source de vérité, toujours dans un état publiable**.

Règle d'or : **on ne pousse jamais directement sur `main`.** Tout changement passe par une branche, une Pull Request (PR) et une intégration continue (CI) verte. Cela vaut même en solo — non pour se freiner, mais parce que ça :

- protège des erreurs (un test cassé n'atteint jamais `main`) ;
- installe des réflexes propres ;
- rend le dépôt **prêt pour des contributeurs** sans rien avoir à changer.

## 2. Protéger `main` (GitHub)

> **Décision actée : [ADR-0027](docs/adr/0027-politique-contribution-verrouillage-branche.md).** La protection de `main` n'est pas qu'une recommandation — elle est **appliquée** (Phase A, depuis le 2026-06-21) via un **ruleset** GitHub (`protect-main`), dont l'état déclaratif vit dans [`.github/ruleset-main-phaseA.json`](.github/ruleset-main-phaseA.json). On retient les **rulesets** (versionnables en JSON, `bypass_actors` explicite), **pas** les *branch protection rules* legacy.

État appliqué sur `main` (Phase A — solo) :

| Règle | Effet |
| --- | --- |
| **Pull request obligatoire** | Aucun push direct sur `main`. |
| **Status checks requis (strict)** | Les **5** jobs de la CI doivent être verts **et** la branche à jour avant merge. |
| **Historique linéaire** (squash/rebase, pas de *merge commit*) | Un commit par PR sur `main` : historique lisible. |
| **Conversations résolues** | Aucun fil de revue non résolu au merge. |
| **Force-push & suppression bloqués** | `main` ne peut être ni réécrit ni supprimé. |
| **`bypass_actors` vide** | Zéro exception — la règle s'applique au mainteneur admin lui-même. |
| *Recommandé, non exigé* : commits signés | Intégrité/traçabilité, sans friction GPG/SSH imposée. |

Les **5 status checks requis** (le `context` = le `name:` du job, **pas** son id YAML ; app GitHub Actions `integration_id` 15368) :
`fmt + clippy`, `cargo-deny (licences + advisories)`, `tests (avec PostgreSQL)`, `build release (artefact déployable)`, `SDK TypeScript (typecheck + build)`. Le `cargo-deny` est la **porte de pureté de licence**, érigée en invariant.

> ⚠️ **Piège du status check** : un check ne peut être requis qu'après avoir **tourné au moins une fois**, et son `context` doit correspondre **exactement** au `name:` du job (sinon la règle attend un check qui n'arrive jamais → merge bloqué). On pose donc la CI d'abord, puis le ruleset.

## 3. La nuance « solo » (importante)

Le réglage **« Require approvals »** est un piège quand on est seul : GitHub exige une revue approuvée par une personne ayant les droits, **mais on ne peut pas approuver sa propre PR**. L'activer en solo, c'est se bloquer soi-même.

Donc :

- **En solo** (Phase A d'ADR-0027) : exiger **PR + status checks**, mais **pas** d'approbation.
- **Dès qu'arrivent des contributeurs** (Phase B) : activer « require 1 approval » + la revue **Code Owners** — le fichier [`.github/CODEOWNERS`](.github/CODEOWNERS) est **déjà en place** (inerte tant que `require_code_owner_review` est `false`).
- **Ne pas sur-configurer** : pas de « 2 reviewers » ni de règles d'organisation quand on est tout seul. Beaucoup de « règles OSS » servent à coordonner une foule ; on adopte le sous-ensemble utile maintenant.

## 4. La boucle de travail

```
git switch -c feat/intensity-now      # branche dédiée
# ... commits ...
git push -u origin feat/intensity-now
# ouvrir la PR (décrire le QUOI et le POURQUOI, lier l'issue / l'ADR)
# la CI tourne → vert
# merge (squash) → suppression de la branche
```

Conventions de branches : `feat/…`, `fix/…`, `docs/…`, `chore/…`. Conventions de commits : un commit = une intention ; [Conventional Commits](https://www.conventionalcommits.org/) appréciés mais non obligatoires (voir CONTRIBUTING).

C'est le même schéma que sur un **GitLab auto-hébergé** plus tard (branches protégées + merge requests + CI). Sur GitHub, ce verrouillage est **actif** via le ruleset d'ADR-0027 (cf. §2 et [CONTRIBUTING.md](CONTRIBUTING.md) § « `main` est protégée »).

## 5. Intégration continue (CI)

Le workflow [`.github/workflows/ci.yml`](.github/workflows/ci.yml) s'exécute à chaque PR (et sur `push` vers `main`, plus un scan quotidien des advisories) et applique exactement les règles du CONTRIBUTING :

- **lint** : `cargo fmt --all --check` + `cargo clippy --all-targets -- -D warnings` ;
- **deny** : `cargo deny check` (licences permissives, avis RustSec, sources de confiance — `deny.toml`) ;
- **test** : `cargo test --workspace` avec un service PostgreSQL (les tests d'intégration appliquent eux-mêmes les migrations) ;
- **build-release** : `cargo build --release --locked` du binaire `carbonfr-server` (garantit que l'image de prod compile et que le lockfile est cohérent) ;
- **sdk-typescript** : typecheck + build du SDK `@carbon-fr/sdk` (`sdk/typescript/`).

Ces cinq jobs **sont** les **status checks requis** de la protection de `main` (ADR-0027) ; leur `context` correspond exactement au `name:` du job. La CI applique ainsi *mécaniquement* ce que la doc décrit : plus de « on a oublié de lancer clippy ».

## 6. Versionnage & releases

- **SemVer**, **version unique de workspace** (`[workspace.package] version` dans le `Cargo.toml` racine, héritée par toutes les crates via `version.workspace = true` ; pas de version par crate — ADR-0019). Tag git `vX.Y.Z` par release, qui doit **refléter** cette version (garde-fou CI dans `release.yml`).
- **Phase `0.x`** : tant qu'on est avant la `1.0`, les ruptures internes sont tolérées en *minor* — ça laisse itérer sans drame, tout en restant honnête sur la stabilité.
- **CHANGELOG.md** au format [Keep a Changelog](https://keepachangelog.com/fr/) : une section par version, regroupée en *Ajouté / Modifié / Corrigé / Supprimé*.
- **Release = image Docker, pas crates.io.** Les crates ne sont **pas** publiées sur crates.io (le service se distribue en image) : pousser un tag `vX.Y.Z` déclenche [`.github/workflows/release.yml`](.github/workflows/release.yml), qui construit et publie l'image sur **GHCR** (`ghcr.io/kovelt/carbon-fr`, publique) taguée `X.Y.Z` / `X.Y` / `latest`, puis crée la **GitHub Release** associée (notes extraites du CHANGELOG). En prod : épingler une version exacte (rollback = redéployer le tag précédent). Le **SDK TypeScript** suit son propre tag `sdk-v*` ([`release-sdk.yml`](.github/workflows/release-sdk.yml)).
- **Quatre axes de version découplés** (ADR-0019), à ne jamais confondre : version applicative (code, ce tag), contrat d'API (`/v1`, ADR-0007), méthodologies & modèles portés par la donnée (`rte-direct`, `acv-ademe@1`/`@2`, `climatology@1`…), et SDK (`sdk-v*`). Aucun ne pilote les autres.
- **Dépréciation** (ADR-0020) : on ne retire **jamais** un élément public (version d'API, endpoint, champ, méthodologie) sans préavis. Une dépréciation s'annonce via les en-têtes HTTP `Deprecation` (RFC 9745) + `Sunset` (RFC 8594), une section *Déprécié* du CHANGELOG et `deprecated: true` dans l'OpenAPI ; retrait au plus tôt après la fenêtre (≥ 6 mois post-1.0, ≥ 30 jours en pré-1.0).

## 7. Les fichiers de gouvernance

| Fichier | Rôle | Statut |
| --- | --- | --- |
| `README.md` | Présentation, démarrage, liens | ✅ présent |
| `LICENSE-MIT` / `LICENSE-APACHE` | Double licence `MIT OR Apache-2.0` | ✅ présent |
| `CONTRIBUTING.md` | Conventions de code & processus | ✅ présent |
| `CODE_OF_CONDUCT.md` | Code de conduite (Contributor Covenant) | ✅ présent |
| `docs/ARCHITECTURE.md` + `docs/adr/` | Conception & décisions tracées | ✅ présent |
| `CLAUDE.md` | Contexte/conventions pour Claude Code | ✅ présent |
| `.github/workflows/ci.yml` | CI (5 jobs : lint, deny, test, build-release, sdk) | ✅ présent |
| `GOUVERNANCE.md` | Gouvernance & workflow (ce document) | ✅ présent |
| `SECURITY.md` | Signalement de faille (en privé) | ✅ présent |
| `.github/dependabot.yml` | MAJ dépendances + alertes sécurité (cargo, npm, actions) | ✅ présent |
| `.github/ISSUE_TEMPLATE/` + `PULL_REQUEST_TEMPLATE.md` | Gabarits issues/PR | ✅ présent |
| `CHANGELOG.md` | Journal des versions | ✅ présent |
| `.github/CODEOWNERS` | Revue obligatoire par domaine (Phase B) | ✅ présent (inerte en Phase A) |
| `.github/ruleset-main-phaseA.json` | État déclaratif du ruleset `main` (ADR-0027) | ✅ présent (appliqué) |

## 8. Lien avec les ADR

Toute décision **structurante** (techno, découpage, modèle de données, méthodologie, déploiement…) se trace dans un [ADR](docs/adr/) avant le code. La **politique de contribution et de verrouillage de `main`** est elle-même tracée par [ADR-0027](docs/adr/0027-politique-contribution-verrouillage-branche.md), qui en est la **source de vérité** ; ce document en décrit l'application au quotidien et reste aligné dessus (il n'« évolue librement » que sur les détails non couverts par un ADR).

## 9. Références

- GitHub — [À propos des rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets)
- GitHub — [Branches protégées](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches)
- [Semantic Versioning](https://semver.org/lang/fr/)
- [Keep a Changelog](https://keepachangelog.com/fr/)
- [Conventional Commits](https://www.conventionalcommits.org/)
