# ADR-0032 — Génération des types du SDK TypeScript et publication de la doc rustdoc des crates internes

- **Statut** : Accepté (la fusion de la PR vaut acceptation par Morgan) — 2 points à confirmer par Morgan, cf. encadrés
- **Date** : 2026-09-25
- **Décideurs** : Morgan (Kovelt / carbon-fr)
- **ADR liés** : ADR-0002 (hexagonal — les adapters et `bin/server` sont couplés à l'IO, jamais candidats à une distribution en bibliothèque), ADR-0019 (axe de version propre du SDK TS, `sdk-v*`), ADR-0027 (gouvernance solo — Phase A active, Phase B déclenchée par la première contribution externe), ADR-0030 (périmètre crates.io = `core`+`eligibility` seulement, §1 et §9 docs.rs), ADR-0031 (SDK Rust — décision 2, écriture manuelle + test de parité contre le snapshot OpenAPI : précédent direct de la décision 1 ci-dessous)

## Contexte

Itération I7 du plan (`docs/plan-iterations.md`, §I7) laisse deux questions ouvertes, décisionnelles, sans ligne de code hors `docs/` :

1. **Types du SDK TypeScript** : `sdk/typescript/src/types.ts` (589 lignes) et `client.ts` (493 lignes) sont écrits et maintenus **à la main**, en parallèle du SDK Rust (`carbonfr-sdk`) qui, lui, a déjà tranché pour l'écriture manuelle + un test de parité mécanique contre le snapshot OpenAPI (ADR-0031 décision 2, `crates/sdk/tests/parity.rs`). Le SDK TS n'a **aucun** garde-fou équivalent en continu : la seule vérification de dérive à ce jour a été un **audit manuel** fait une fois, pour la sortie de la 0.2.0 — « audit de parité exhaustif contre l'OpenAPI (26 opérations, 51 schémas) : paramètres de requête manquants (`version` sur `mix`, `schedule`, `scheduleSlots`, `below`, `greenestWindow`…) » (`CHANGELOG.md:214-219`). Cet audit a donc **trouvé une dérive réelle**, pas hypothétique — mais seulement à la relecture manuelle, pas en CI.
2. **Doc rustdoc des crates non publiées** : le workspace compte **9 membres non publiés** (8 adapters + `bin/server`), tous `publish = false` (vérifié : `crates/adapter-{odre,postgres,http,forecast,meteo,entsoe,webhook,gbdt}/Cargo.toml:8`, `bin/server/Cargo.toml:8`). Le job CI de documentation (`rustdoc-package`, `.github/workflows/ci.yml:221-237`) ne construit et ne vérifie **que** les 3 crates publiées ou publiables (`-p carbonfr-core -p carbonfr-eligibility -p carbonfr-sdk`, `ci.yml:237`) ; les 9 autres n'ont aujourd'hui **aucun** build de doc vérifié en CI et **aucun** hébergement (pas de workflow `pages` dans `.github/workflows/`, vérifié par listing du dossier le 2026-09-25).

Les deux questions partagent une même famille de réponse (outiller vs écarter, avec un coût d'entretien à mettre en face d'un bénéfice réel) — d'où un seul ADR, comme le nom du fichier l'indique.

## Décision

### 1. Génération des types TS depuis l'OpenAPI 3.1 : **écartée pour le v1** ; adopter à la place un test de parité mécanique, léger, symétrique de celui du SDK Rust

**Outil sérieux évalué : `openapi-typescript`** (le seul générateur TS avec une adoption large qui documente explicitement un support OpenAPI 3.1 — `openapi-ts.dev/introduction`, vérifié 2026-09-25 : « Supports OpenAPI 3.0 and 3.1 (including advanced features like discriminators) »). État vérifié le 2026-09-25 (`registry.npmjs.org/openapi-typescript`) :

| Point | Constat |
|---|---|
| Version courante | `7.13.0` (`dist-tags.latest`) |
| Dernière publication | `2026-02-11T16:02:25Z` (champ `time`) — **~7,5 mois** sans nouvelle version au jour de cette revue, contre un rythme de plusieurs versions par trimestre sur 2025 (`7.9.0` en août, `7.10.0` en octobre, `7.12.0`/`7.13.0` en février) : ralentissement net, mais pas un signal d'abandon — **~25,8 M téléchargements sur 30 jours** (2026-08-25 → 2026-09-23, `api.npmjs.org/downloads/point/last-month/openapi-typescript`, consulté 2026-09-25) : paquet massivement utilisé |
| `peerDependencies` | `{"typescript": "^5.x"}` |

**Limite de fidélité connue, et confirmée présente dans notre propre contrat** : OpenAPI 3.1 adopte l'idiome JSON Schema 2020-12 `"type": ["T", "null"]` pour exprimer la nullabilité (au lieu du `nullable: true` de 3.0). Un problème ouvert sur le dépôt du générateur (`openapi-ts/openapi-typescript` issue #898, « Support nullable as type arrays for OpenAPI 3.1 », retrouvé par recherche le 2026-09-25) documente que cette forme est aujourd'hui générée en `unknown` plutôt qu'en `T | null`. Ce n'est pas un cas théorique pour nous : `crates/adapter-http/tests/openapi.snapshot.json` utilise cet idiome exact **23 fois** dans `components/schemas` (script de comptage rejoué le 2026-09-25 ; ex. `components/schemas/CostAssumptionsBody/properties/discount_rate` → `["number", "null"]`, `.../CreateWebhookRequest/properties/region` → `["string", "null"]`), plus 3 `oneOf` nullables issus de la montée `utoipa` 6 (`CHANGELOG.md:207-209`, `best_eligible`/`eligibility`/`marginal_technology`). Un import brut dégraderait donc, sur au moins ces champs, des types aujourd'hui précis à la main (`number | null`, `sdk/typescript/src/types.ts:475-479`) vers `unknown` — une **régression de fidélité**, pas un gain, sauf à ajouter une étape de post-traitement du fichier généré (nouvelle surface de maintenance, pour un générateur déjà en décélération).

**Conflit de dépendance concret, reproduit dans ce dépôt le 2026-09-25** (aucun fichier modifié — `--dry-run`, `git status --porcelain sdk/typescript` vide avant/après) :

```
$ cd sdk/typescript && npm install openapi-typescript --dry-run
npm error ERESOLVE unable to resolve dependency tree
npm error Found: typescript@7.0.2
npm error   dev typescript@"^7.0.2" from the root project
npm error Could not resolve dependency:
npm error   peer typescript@"^5.x" from openapi-typescript@7.13.0
```

`sdk/typescript/package.json:47` épingle déjà `typescript: "^7.0.2"` (devDependency), au-delà du `peerDependencies` déclaré par `openapi-typescript` 7.13.0. L'installer aujourd'hui exigerait `--legacy-peer-deps` (résolution dégradée, non recommandée par npm lui-même) ou d'attendre une release compatible TypeScript 7 — aucune option propre.

**Alternative écartée pour la même famille de raisons qu'ADR-0031 décision 2** : les générateurs Java (OpenAPITools, ciblage `typescript-fetch`/`typescript-axios`) imposeraient une dépendance JRE au pipeline `sdk-typescript` (`ci.yml:120-138`, aujourd'hui Node seul) pour un bénéfice non supérieur à `openapi-typescript` sur le point qui compte ici (fidélité 3.1) — écarté sans plus d'examen.

**Décision retenue : ni générateur, ni statu quo silencieux.** Comme pour le SDK Rust, écrire un **test de parité mécanique**, symétrique de `crates/sdk/tests/parity.rs` : une table `operationId → méthode SDK` vérifiée à chaque run contre `crates/adapter-http/tests/openapi.snapshot.json` (28 opérations aujourd'hui, 26 en périmètre v1 hors `health`/`health_ready`, compte revérifié le 2026-09-25), exécutée dans le job CI `sdk-typescript` déjà existant (`ci.yml:120-138`). Cela automatise exactement ce que l'audit manuel de la 0.2.0 a fait une fois (`CHANGELOG.md:214-219`) — sans regénérer les types, donc sans toucher à leur fidélité actuelle ni à leur documentation domaine (ex. la note sur la non-comparabilité du `score` entre `framework`s, `types.ts:194-199`, qu'un générateur ne produirait pas sans un gabarit dédié). Coût attendu : quelques dizaines de lignes en JS/TS natif (`node:test` + `node:assert`, déjà dans le runtime Node 22/24 utilisé par le job — zéro dépendance runtime ajoutée, cohérent avec « Toujours zéro dépendance runtime » du SDK, `CHANGELOG.md:219`), lisant le même fichier snapshot que le test Rust.

> **Point à confirmer par Morgan** : quand planifier l'implémentation du test de parité TS (cet ADR est décisionnel, aucune ligne de code n'est écrite ici). *Recommandation* : le greffer dans la prochaine PR qui touche `sdk/typescript/src/client.ts` ou le contrat `/v1` (coût marginal minime une fois qu'on est déjà dans ces fichiers), plutôt que d'ouvrir un chantier isolé sans changement fonctionnel ; à défaut, l'inscrire explicitement dans une itération I8+.

### 2. Doc rustdoc des 9 crates non publiées (adapters + `bin/server`) : **pas de publication pour l'instant** (GitHub Pages ou équivalent écarté, avec déclencheur de réévaluation)

État mesuré le 2026-09-25 : `cargo doc --no-deps --offline` rejoué sur les 9 crates non publiées (adapters + `server`) produit déjà, sans même activer `-D warnings`, des avertissements de liens dans au moins une crate :

```
warning: public documentation for `carbonfr_adapter_webhook` links to private item `PublicOnlyResolver`
 --> crates/adapter-webhook/src/lib.rs:7:34
warning: unresolved link to `HttpNotifier::new_for_test`
   --> crates/adapter-webhook/src/lib.rs:113:14
warning: public documentation for `HttpNotifier` links to private item `HttpNotifier::build`
   --> crates/adapter-webhook/src/lib.rs:115:39
```

C'est la même famille de travail que celle déjà identifiée et budgétée pour `core`/`eligibility` avant leur publication (ADR-0030, état vérifié : « 2 liens vers des items privés + 8 liens `[FAIT]`/`[ESTIMATION]` pris pour des liens intra-doc », traités en I4) — mais **non faite** ici et **non budgétée** par le plan I7 (qui ne prévoit, pour I7, qu'une décision, pas une préparation technique).

Trois éléments pèsent contre une publication maintenant :

- **Aucun lectorat externe identifié aujourd'hui.** Gouvernance solo, Phase A active (ADR-0027) ; Phase B (« ouverte (déclenchée par la première contribution externe) », `0027-politique-contribution-verrouillage-branche.md:66`) n'a pas encore de déclencheur atteint. `docs/ARCHITECTURE.md` et les ADR couvrent déjà le « pourquoi » ; `cargo doc --open` en local couvre le « quoi » sans aucun coût d'hébergement pour Morgan lui-même.
- **Ces 9 crates ne portent aucune garantie SemVer propre** (`publish = false`, version héritée du workspace, aucune n'a de politique `#[non_exhaustive]` dédiée comme `core`/`eligibility`, ADR-0030 §3). ADR-0030 §1 est explicite : les publier romprait « l'esprit bibliothèque réutilisable ». Un site de doc publique, même en lecture seule, suggère implicitement une surface stable qu'on ne veut pas promettre pour ces crates-là — risque de signal trompeur si présenté sans bandeau.
- **Coût d'hébergement non nul pour un bénéfice non prouvé** : nouveau job CI (build doc des 9 crates), déploiement (branche `gh-pages` ou `actions/deploy-pages`), page à maintenir en plus des 3 crates déjà suivies par `rustdoc-package` (`ci.yml:221-237`).

**Décision : ne pas publier maintenant.** Redéclenchement à réévaluer sur l'un de ces deux signaux : **(a)** la première contribution externe réelle (Phase B, ADR-0027), qui rend la lecture du code par un tiers effectivement plus fréquente que « Morgan avec son IDE » ; **(b)** un besoin explicite de Morgan (ex. onboarding d'un co-mainteneur). Si déclenché : portée = les 9 crates avec un bandeau explicite « documentation interne, aucune garantie de stabilité » sur la page d'accueil générée, hébergement le plus simple possible (GitHub Pages via Actions, pas d'infra dédiée), et build **non bloquant** (`-D warnings` optionnel, pas obligatoire dès le premier jour comme pour les 3 crates publiées) tant que les liens cassés déjà repérés ci-dessus n'ont pas été nettoyés.

> **Point à confirmer par Morgan** : publier ou non une doc rustdoc des crates internes, et si oui, à quel horizon. *Recommandation* : ne pas publier maintenant (aucun lecteur externe, coût d'hébergement pour un bénéfice nul aujourd'hui), et se caler sur la Phase B de gouvernance (ADR-0027) comme déclencheur plutôt que sur une date fixe.

## Conséquences

**Positives** :
- Le SDK TS gagne un garde-fou **continu** (CI, à chaque run) là où il n'avait qu'un audit **ponctuel** (une fois, à la sortie de la 0.2.0) — sans introduire de dépendance de génération dont la fidélité 3.1 est démontrée insuffisante sur notre propre schéma (23 occurrences de l'idiome nullable affecté) ni de conflit de peer-dependency déjà reproduit.
- Cohérence inter-SDK : la même question (générer depuis l'OpenAPI 3.1 vs écrire à la main + test de parité) reçoit la même réponse côté Rust (ADR-0031 décision 2) et côté TypeScript — ce n'est pas une coïncidence de style, mais deux analyses indépendantes qui convergent sur la même limite de fond (écosystème de génération pas encore mûr pour du JSON Schema 2020-12 fidèle).
- Pas de chantier d'hébergement de doc ouvert sans lecteur identifié ; le critère de réévaluation (Phase B, ADR-0027) est déjà un déclencheur défini ailleurs dans le dépôt, pas une nouvelle notion à suivre.

**Négatives / limites assumées** :
- Le SDK TS continue de dupliquer à la main ce qu'un générateur automatiserait en théorie — coût de maintenance humaine qui persiste (mitigé par le futur test de parité, pas éliminé).
- Le test de parité TS n'est **pas implémenté** par cet ADR (itération décisionnelle) : tant qu'il ne l'est pas, le SDK TS reste dans l'état actuel (audit manuel ponctuel) — fenêtre de risque non refermée avant l'implémentation effective (cf. point à confirmer, calendrier proposé).
- Les 9 crates internes restent sans doc publiée : un futur contributeur externe devra, en attendant Phase B, lire le code source et `docs/ARCHITECTURE.md` plutôt qu'un site de référence — assumé, cohérent avec l'absence actuelle de tout contributeur externe.

## Alternatives envisagées

- **Génération complète via `openapi-typescript`** — écartée : limite de fidélité 3.1 démontrée sur notre propre snapshot (23 occurrences), conflit `peerDependencies` reproduit dans ce dépôt, cadence de publication ralentie (~7,5 mois sans version au jour de la revue). Cf. décision 1.
- **Générateurs OpenAPITools (Java)** — écartés : dépendance JRE ajoutée au pipeline `sdk-typescript` (aujourd'hui Node seul) pour un bénéfice non supérieur sur le point qui compte (fidélité 3.1), même famille de raison qu'ADR-0031 §2 pour le SDK Rust.
- **Statu quo (aucun garde-fou de parité, TS comme aujourd'hui)** — écartée : l'audit manuel de la 0.2.0 a démontré qu'une dérive réelle se produit (`CHANGELOG.md:214-219`) ; ne rien automatiser après l'avoir constaté reviendrait à répéter le même risque à chaque release future, alors qu'un test mécanique reprend un patron déjà en place côté Rust.
- **Publier la doc rustdoc de toutes les crates dès maintenant** — écartée : coût d'hébergement certain pour un lectorat externe non encore existant (Phase A, ADR-0027) ; risquerait de laisser croire à une garantie de stabilité que ADR-0030 §1 refuse explicitement pour les adapters et `bin/server`.
- **Publier seulement un sous-ensemble des 9 crates (ex. la plus grosse, `adapter-http`)** — écartée : aucun critère objectif pour choisir laquelle, introduirait une asymétrie arbitraire ; mieux vaut trancher toutes les 9 ensemble au moment du déclencheur (Phase B) que d'ouvrir une exception non motivée aujourd'hui.
- **docs.rs pour les crates internes** — techniquement impossible : docs.rs ne construit que des crates publiées sur crates.io, et ADR-0030 §1 exclut explicitement ces 9 crates de toute publication crates.io ; seul un hébergement externe (type GitHub Pages) resterait pertinent si la publication était un jour décidée.
