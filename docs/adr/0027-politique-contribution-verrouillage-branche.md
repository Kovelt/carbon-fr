# ADR-0027 — Politique de contribution et verrouillage de branche

- **Statut** : Accepté
- **Date** : 2026-06-20
- **Décideur** : Morgan (mainteneur unique)
- **Portée** : Dépôt `carbon-fr`, branche `main`
- **S'appuie sur** : ADR-0002 (architecture hexagonale — l'invariant à protéger), ADR-0005 (méthodologie versionnée — jamais modifiée silencieusement), ADR-0019 (versionnement)
- **Mise en œuvre** : `CONTRIBUTING.md`, `.github/CODEOWNERS`, templates PR/issue, et le ruleset de branche (`.github/ruleset-main-phaseA.json`). Le *comment* est détaillé dans [`docs/brief-claude-code-ruleset-main.md`](../brief-claude-code-ruleset-main.md). Dépend de la CI (`.github/workflows/ci.yml`, en place) pour les *status checks* requis.

> **Note de numérotation.** Cet ADR a d'abord été rédigé sous le numéro provisoire
> `0015`, déjà attribué à [ADR-0015 (tier hébergé / clés API)](0015-tier-heberge-cles-api.md).
> Il a été renuméroté **0027** à l'intégration (prochain numéro libre) ; la date de
> décision (2026-06-20) est conservée.

---

## Contexte

`carbon-fr` est destiné à s'ouvrir aux contributions externes. Le projet porte
des invariants forts qu'une contribution bien intentionnée peut violer sans le
savoir :

- architecture hexagonale stricte (calcul/prédiction en types de domaine purs,
  I/O derrière des ports) ;
- pureté de licence (risque d'introduction d'une dépendance à licence
  incompatible) ;
- méthodologies versionnées (`rte-direct`, `acv-ademe`) et facteurs ACV ADEME
  qui ne doivent jamais bouger sans décision tracée ;
- gouvernance par ADR : aucune extension méthodologique ou structurelle ne ship
  sans ADR préalable.

La revue préalable de toute contribution est donc une exigence, pas un confort.

Une contrainte technique de GitHub structure la décision : **un mainteneur ne
peut pas approuver sa propre pull request.** Un verrouillage « strict » naïf
(approbation humaine exigée dès aujourd'hui) bloquerait donc le mainteneur
lui-même tant que le projet est solo.

Phase actuelle : mainteneur unique, pré-ouverture.

## Décision

Adopter un **verrouillage strict de `main` à activation phasée**. Principe
directeur : aucune contribution ne peut être fusionnée sans validation explicite
du mainteneur, et la règle s'applique **sans exception d'administrateur**
(`bypass_actors` vide).

Le verrouillage se décline en deux régimes, car la menace n'est pas la même
selon le moment :

### Phase A — solo (immédiate)

La seule menace est la précipitation du mainteneur. On verrouille le *mécanique*,
pas l'humain :

- pull request obligatoire (aucun push direct sur `main`) ;
- CI verte obligatoire (status checks requis = **tous** les jobs de `ci.yml`) ;
- branche à jour avant fusion (`strict_required_status_checks_policy`) ;
- conversations résolues avant fusion ;
- historique linéaire (squash/rebase imposé) ;
- force-push et suppression de branche interdits ;
- **pas d'approbation humaine requise** (techniquement impossible à satisfaire en
  solo) ; auto-relecture via le diff de la PR ;
- `bypass_actors` vide : la règle s'applique au mainteneur lui-même.

### Phase B — ouverte (déclenchée par la première contribution externe)

On ajoute la couche humaine :

- approbation requise : 1 ;
- revue obligatoire des Code Owners (`require_code_owner_review`) via
  `CODEOWNERS`, où le mainteneur est owner de l'ensemble du dépôt ;
- annulation des approbations obsolètes à chaque nouveau push
  (`dismiss_stale_reviews_on_push`).

**Conséquence mécanique recherchée :** un contributeur externe ne pouvant pas
approuver sa propre PR, et le mainteneur étant seul owner, **aucune ligne externe
ne peut être fusionnée sans le clic du mainteneur.** Le « toi seul merges » est
obtenu par la structure, non par un privilège d'administrateur.

### Options transverses

- **Historique linéaire : retenu** dès la phase A (propreté de l'historique,
  cohérent avec l'exigence de qualité du projet).
- **Commits signés : recommandé mais non exigé.** Exiger la signature
  (GPG/SSH) constituerait une barrière réelle pour un contributeur débutant.
  Choix : documenter la recommandation dans `CONTRIBUTING.md` sans en faire une
  règle bloquante. Réévaluable par ADR ultérieur.
- **DCO / `Signed-off-by` : hors périmètre de cet ADR.** L'attestation d'origine
  des contributions touche à la pureté de licence et fera l'objet d'une décision
  dédiée si elle est retenue.

## Conséquences

### Positives

- Les invariants (archi, licence, méthodo) sont protégés par une porte humaine
  systématique en phase ouverte.
- La gouvernance est crédible : « zéro exception » est réel, y compris pour le
  mainteneur, ce qui renforce la confiance des contributeurs.
- L'onboarding est cadré en amont, ce qui raccourcit les revues.
- Le mainteneur n'est jamais auto-bloqué (résolution de la tension solo/strict).

### Négatives / coûts

- **Friction pour les contributeurs** : rebase imposé, attente de CI verte.
  Atténuation : documentation claire dans `CONTRIBUTING.md`.
- **Goulot d'étranglement de revue** : toute la charge repose sur le mainteneur.
  Atténuation future : co-mainteneurs + `CODEOWNERS` granulaire (revue ciblée par
  chemin : domaine/méthodo réservés au mainteneur, périphérie déléguée).
- **Dépendance d'ordonnancement** : la règle de status checks ne peut être posée
  qu'une fois `ci.yml` en place avec des noms de jobs stables. La mise en œuvre du
  ruleset est donc séquencée *après* la CI.

### Transition Phase A → Phase B (checklist d'activation)

1. Porter `required_approving_review_count` de `0` à `1`.
2. Activer `require_code_owner_review`.
3. Vérifier la présence et la justesse de `CODEOWNERS`.
4. Vérifier que `bypass_actors` est toujours vide.

## Alternatives écartées

1. **Régime « modéré » permanent** (PR + CI sans approbation humaine, même en
   phase ouverte). *Rejeté :* ne garantit aucune revue humaine des contributions
   externes touchant la méthodologie ou l'architecture.
2. **Bypass administrateur permanent.** *Rejeté :* transforme « zéro exception »
   en « zéro exception sauf moi », ce qui ruine la crédibilité de la gouvernance.
3. **Approbation humaine exigée dès la phase solo.** *Rejeté :* techniquement
   impossible (auto-approbation interdite) ; auto-blocage du mainteneur.
4. **Accès en écriture accordé aux contributeurs de confiance.** *Rejeté à ce
   stade :* prématuré et incompatible avec l'exigence de revue préalable
   systématique. Réenvisageable une fois une communauté établie, via ADR dédié.
5. **Branch protection « legacy ».** *Rejeté :* on retient les **rulesets** GitHub
   (plus expressifs, versionnables en JSON, `bypass_actors` explicite). Ne pas
   reconvertir en règle de protection legacy.
