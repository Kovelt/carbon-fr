# Brief Claude Code — Configuration du ruleset de branche `main`

> Source de vérité : **[ADR-0027](adr/0027-politique-contribution-verrouillage-branche.md)**
> (Politique de contribution et verrouillage de branche). En cas de doute sur le
> *pourquoi*, s'y référer. Ce brief ne décrit que le *comment* de la mise en œuvre
> du ruleset.
>
> **Renumérotation.** Ce brief visait initialement « ADR-0015 » ; cet identifiant
> était déjà pris (tier hébergé). La décision de contribution est désormais
> **ADR-0027**.

## Objectif

Poser sur `carbon-fr` un ruleset de branche verrouillant `main` selon la
**Phase A** d'ADR-0027 (régime solo). La Phase B (approbation humaine + Code
Owners) n'est **pas** activée par ce brief : elle le sera plus tard, à la
première contribution externe.

## État du dépôt (constaté le 2026-06-21)

- Dépôt : **`Kovelt/carbon-fr`** (public). Le token `gh` a `admin: true` dessus.
- `ci.yml` **existe et tourne** (sur `push: main` et sur `pull_request`) — le
  prérequis bloquant ci-dessous est donc **levé**.
- ⚠️ **Un ruleset existe déjà** : `protect-main` (id `17745480`, actif depuis le
  2026-06-16). Il n'est qu'un **Phase A partiel** : il ne requiert que 2 checks
  (`fmt + clippy`, `tests (avec PostgreSQL)`), avec `strict = false`,
  `dismiss_stale = false`, `thread_resolution = false`, et autorise encore les
  *merge commits*. **Conséquence : on le met à jour (PUT), on ne crée pas de
  doublon (POST)** — deux rulesets sur `main` se combineraient de façon confuse.

## Prérequis (levé)

Le ruleset référence des **status checks par leur nom de check-run**. GitHub exige
que ce nom corresponde exactement au **`name` du job** dans le workflow (et non à
son id YAML). Noms réels confirmés sur `main` (tous app `github-actions`,
`integration_id = 15368`) :

| Job YAML (`id`) | `name:` = nom du check-run requis |
|---|---|
| `lint` | `fmt + clippy` |
| `deny` | `cargo-deny (licences + advisories)` |
| `test` | `tests (avec PostgreSQL)` |
| `build-release` | `build release (artefact déployable)` |
| `sdk-typescript` | `SDK TypeScript (typecheck + build)` |

ADR-0027 exige « **tous** les jobs de `ci.yml` » → les **5** contexts sont requis
(la version initiale de ce brief n'en listait que 4, génériques `fmt/clippy/test/build`,
explicitement « à confirmer » — corrigé ici). Le `cargo-deny` est essentiel : c'est
la porte de **pureté de licence**, érigée en invariant par l'ADR.

## Paramètres

- `OWNER` : `Kovelt`.
- `REPO` : `carbon-fr`.
- Cible de branche : `~DEFAULT_BRANCH` (cible la branche par défaut sans coder
  « main » en dur).
- Authentification : `gh auth status` doit montrer un token avec *Administration
  (write)* sur le dépôt (le scope classique `repo` + droit admin suffit ; vérifié).

## État déclaratif cible — Phase A (source de vérité du comment)

| Règle | Valeur Phase A | Valeur Phase B (plus tard) |
|---|---|---|
| PR obligatoire | oui | oui |
| `required_approving_review_count` | `0` | `1` |
| `require_code_owner_review` | `false` | `true` |
| `dismiss_stale_reviews_on_push` | `true` | `true` |
| `required_review_thread_resolution` | `true` | `true` |
| `required_status_checks` (strict) | oui — **les 5 contexts** ci-dessus | idem |
| `allowed_merge_methods` | `["squash", "rebase"]` (pas de merge commit) | idem |
| `required_linear_history` | oui | oui |
| `non_fast_forward` (bloque force-push) | oui | oui |
| `deletion` (bloque suppression) | oui | oui |
| `bypass_actors` | **vide** | **vide** |

`bypass_actors` vide = aucune exception, y compris l'administrateur. C'est
l'expression du « zéro exception » d'ADR-0027. Ne pas y ajouter d'acteur.

## Mise en œuvre — payload Phase A

Le payload canonique vit dans **[`.github/ruleset-main-phaseA.json`](../.github/ruleset-main-phaseA.json)**
(même forme pour un `POST` création ou un `PUT` mise à jour) :

```json
{
  "name": "protect-main (phase A — solo)",
  "target": "branch",
  "enforcement": "active",
  "conditions": {
    "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] }
  },
  "bypass_actors": [],
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    { "type": "required_linear_history" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": true,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": true,
        "required_reviewers": [],
        "allowed_merge_methods": ["squash", "rebase"]
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": true,
        "do_not_enforce_on_create": false,
        "required_status_checks": [
          { "context": "fmt + clippy", "integration_id": 15368 },
          { "context": "cargo-deny (licences + advisories)", "integration_id": 15368 },
          { "context": "tests (avec PostgreSQL)", "integration_id": 15368 },
          { "context": "build release (artefact déployable)", "integration_id": 15368 },
          { "context": "SDK TypeScript (typecheck + build)", "integration_id": 15368 }
        ]
      }
    }
  ]
}
```

**Mettre à jour le ruleset existant** (cas réel — `protect-main`, id `17745480`) :

```bash
gh api \
  --method PUT \
  -H "Accept: application/vnd.github+json" \
  -H "X-GitHub-Api-Version: 2022-11-28" \
  /repos/Kovelt/carbon-fr/rulesets/17745480 \
  --input .github/ruleset-main-phaseA.json
```

**Créer** (uniquement si aucun ruleset `main` n'existe — fork, nouveau dépôt) :

```bash
gh api \
  --method POST \
  -H "Accept: application/vnd.github+json" \
  -H "X-GitHub-Api-Version: 2022-11-28" \
  /repos/Kovelt/carbon-fr/rulesets \
  --input .github/ruleset-main-phaseA.json
```

## Vérification

```bash
# Lister les rulesets et récupérer l'ID
gh api /repos/Kovelt/carbon-fr/rulesets

# Inspecter le ruleset
gh api /repos/Kovelt/carbon-fr/rulesets/17745480
```

Contrôles attendus :

- `enforcement` = `active` ;
- `bypass_actors` = `[]` ;
- les **cinq** contexts de status checks présents, orthographiés comme les
  check-runs réels (cf. tableau) avec `integration_id = 15368` ;
- `strict_required_status_checks_policy` = `true` ;
- `required_review_thread_resolution` = `true`, `dismiss_stale_reviews_on_push` = `true` ;
- un push direct sur `main` est refusé (tester avec une branche jetable) ;
- une PR ne peut fusionner que CI verte (5/5) + branche à jour + conversations résolues.

## Notes de vigilance

- **Le check-run se nomme comme le `name:` du job**, pas comme son id YAML. Si
  `ci.yml` renomme un job, le `context` correspondant doit être mis à jour ici
  **et** dans `.github/ruleset-main-phaseA.json`, sinon le check requis ne sera
  jamais satisfait (merge bloqué à jamais).
- `integration_id: 15368` = l'app GitHub Actions ; il rend le *matching* précis
  (évite qu'un check homonyme d'une autre app satisfasse la règle).
- Si l'API renvoie une erreur sur un nom de champ, vérifier la version courante
  de l'endpoint « rulesets » et ajuster le JSON sans changer l'**état déclaratif
  cible** ci-dessus, qui fait foi.
- Ne pas convertir cette protection en *branch protection rule* legacy : ADR-0027
  retient les **rulesets** (aucune protection legacy active — vérifié).
- Toute évolution des valeurs (passage Phase A → B) doit suivre la checklist
  d'activation d'ADR-0027, pas une modification ad hoc.

## État au 2026-06-21

Phase A **appliquée** : le ruleset `protect-main` (id `17745480`) a été **mis à
jour** (PUT) vers l'état déclaratif cible ci-dessus — 5 status checks requis,
`strict = true`, conversations résolues, dismiss stale, squash/rebase imposés,
`bypass_actors` vide. Phase B non activée.
