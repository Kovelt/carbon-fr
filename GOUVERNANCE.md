# Gouvernance & workflow du projet

Comment `carbon-fr` est géré au quotidien : protection de la branche principale, cycle de contribution, intégration continue, versionnage et fichiers de gouvernance. Ce document s'adresse au mainteneur (toi) autant qu'aux futurs contributeurs ; les conventions de code, elles, vivent dans [CONTRIBUTING.md](CONTRIBUTING.md).

## 1. Principe directeur

`main` est la **source de vérité, toujours dans un état publiable**.

Règle d'or : **on ne pousse jamais directement sur `main`.** Tout changement passe par une branche, une Pull Request (PR) et une intégration continue (CI) verte. Cela vaut même en solo — non pour se freiner, mais parce que ça :

- protège des erreurs (un test cassé n'atteint jamais `main`) ;
- installe des réflexes propres ;
- rend le dépôt **prêt pour des contributeurs** sans rien avoir à changer.

## 2. Protéger `main` (GitHub)

GitHub propose deux mécanismes : les **branch protection rules** classiques et les **rulesets**, plus récents et recommandés (ils se cumulent ; quand deux règles se chevauchent, la plus restrictive l'emporte). Les deux sont gratuits sur les dépôts publics.

Réglages recommandés sur `main` :

| Réglage | Pourquoi |
| --- | --- |
| **Require a pull request before merging** | Interdit le push direct sur `main`. |
| **Require status checks to pass** | La CI (fmt, clippy, test) doit être verte avant tout merge. |
| **Require linear history** + merge en *squash* | Un commit par PR sur `main` : historique lisible. |
| **Require conversation resolution** | Aucun fil de revue non résolu au merge. |
| **Block force-push & suppression** de `main` | `main` ne peut être ni réécrit ni supprimé. |
| *Optionnel* : **Require signed commits** | Intégrité/traçabilité (au prix d'un peu de friction GPG/SSH). |

> ⚠️ **Piège du status check** : une vérification n'apparaît dans la liste sélectionnable qu'après avoir tourné **au moins une fois** sur le dépôt. Donc : on pousse d'abord le workflow CI, on le laisse s'exécuter une fois, puis on revient cocher le check comme requis.

## 3. La nuance « solo » (importante)

Le réglage **« Require approvals »** est un piège quand on est seul : GitHub exige une revue approuvée par une personne ayant les droits, **mais on ne peut pas approuver sa propre PR**. L'activer en solo, c'est se bloquer soi-même.

Donc :

- **En solo** : exiger **PR + status checks**, mais **pas** d'approbation.
- **Dès qu'arrivent des contributeurs** : ajouter « require 1 approval » et un fichier `CODEOWNERS`.
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

C'est le même schéma que sur un **GitLab auto-hébergé** plus tard (branches protégées + merge requests + CI) — aucun verrouillage sur GitHub.

## 5. Intégration continue (CI)

Le workflow [`.github/workflows/ci.yml`](.github/workflows/ci.yml) s'exécute à chaque PR (et sur `push` vers `main`, plus un scan quotidien des advisories) et applique exactement les règles du CONTRIBUTING :

- **lint** : `cargo fmt --all --check` + `cargo clippy --all-targets -- -D warnings` ;
- **deny** : `cargo deny check` (licences permissives, avis RustSec, sources de confiance — `deny.toml`) ;
- **test** : `cargo test --workspace` avec un service PostgreSQL (les tests d'intégration appliquent eux-mêmes les migrations) ;
- **build-release** : `cargo build --release --locked` du binaire `carbonfr-server` (garantit que l'image de prod compile et que le lockfile est cohérent) ;
- **sdk-typescript** : typecheck + build du SDK `@carbon-fr/sdk` (`sdk/typescript/`).

Ces jobs deviennent les **status checks requis** de la protection de `main`. La CI applique ainsi *mécaniquement* ce que la doc décrit : plus de « on a oublié de lancer clippy ».

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
| `.github/workflows/ci.yml` | CI (fmt, clippy, test + PostgreSQL) | ✅ présent |
| `GOUVERNANCE.md` | Gouvernance & workflow (ce document) | ✅ présent |
| `SECURITY.md` | Signalement de faille (en privé) | ✅ présent |
| `.github/dependabot.yml` | MAJ dépendances + alertes sécurité (cargo, npm, actions) | ✅ présent |
| `.github/ISSUE_TEMPLATE/` + `PULL_REQUEST_TEMPLATE.md` | Gabarits issues/PR | ⬜ à ajouter |
| `CHANGELOG.md` | Journal des versions | ✅ présent |
| `CODEOWNERS` | Revue obligatoire par domaine | ⬜ quand contributeurs |

## 8. Lien avec les ADR

Toute décision **structurante** (techno, découpage, modèle de données, méthodologie, déploiement…) se trace dans un [ADR](docs/adr/) avant le code. Les choix de gouvernance ci-dessus restent quant à eux dans ce document, qui évolue librement.

## 9. Références

- GitHub — [À propos des rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets)
- GitHub — [Branches protégées](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches)
- [Semantic Versioning](https://semver.org/lang/fr/)
- [Keep a Changelog](https://keepachangelog.com/fr/)
- [Conventional Commits](https://www.conventionalcommits.org/)
