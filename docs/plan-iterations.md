# Plan de la suite — itérations I0 → I7 (à partir du 2026-09-23)

- **Statut** : document vivant — cocher les cases au fil des PR, dater chaque révision en tête.
- **Dernière mise à jour** : 2026-09-23 (I0 terminée hors fermeture des issues #76/#81 ; I1 livrée en v0.8.1, reste la notification Uptime Kuma).
- **Sources** : état des lieux multi-agents du 2026-09-23 (constats revérifiés contre le code), recherche en 4 volets (préparation crates.io, backlog consolidé des ADR/roadmaps, montées majeures des dépendances, échéances datées), 3 plans concurrents (« fiabilité d'abord », « adoption d'abord », « valeur métier d'abord ») départagés par un juge. Base retenue : **valeur métier d'abord**, avec les greffes des deux autres.
- **Horizon** : 13 à 17 semaines selon les itérations, soit vers mi-janvier 2027 au rythme d'un mainteneur seul assisté de Claude Code. Les durées sont des ordres de grandeur, pas des engagements.
- **Liens** : feuille de route produit dans le [README](../README.md#feuille-de-route), [roadmap hydrogène](roadmap-hydrogene.md), [index des ADR](adr/README.md), [CHANGELOG](../CHANGELOG.md).

## Principes

1. **La file de PR d'abord.** Rien de nouveau tant que le code déjà écrit n'est pas mergé, publié et déployé.
2. **La donnée publiée d'abord.** Une valeur servie qui est fausse ou périmée (TRV 2026-H2) passe avant toute nouvelle surface.
3. **Décider avant d'exécuter.** Toute décision structurante est un ADR (ou un addendum) mergé **avant** la première ligne de code qu'elle gouverne. crates.io suit strictement : **ADR → préparation → publication**.
4. **Garde-fous inchangés.** Aucun item ne densifie le poll ODRÉ (~80 % du quota consommé), ne contourne une GATE de backtest (`gbdt@1`, `share-meteo@2`) ni n'anticipe un droit non adopté (hydrogène H3).
5. **Montées majeures isolées et par risque croissant.** Une PR par montée (`utoipa` → `reqwest` → `sqlx`), jamais mêlée à une PR fonctionnelle.
6. **Un déclencheur externe reste un déclencheur.** Ce qui dépend d'un texte UE, d'une licence tierce ou d'un premier client va dans [En attente](#en-attente-dun-déclencheur), avec sa condition explicite.

## Vue d'ensemble

| # | Itération | Durée | Release visée | Sortie clé |
|---|---|---|---|---|
| **I0** | Vider la file de PR, livrer et déployer | ~1 sem. | v0.7.2 → v0.8.0 | Prod en 0.8.0, 0 PR ouverte |
| **I1** | Confiance dans la donnée et l'exploitation | ~2 sem. | v0.8.1 | TRV 2026-H2 servi, restauration **testée**, CI en PG17, alerte de fraîcheur |
| **I2** | Hygiène DX et dépendances légères | ~1–2 sem. | v0.8.2 + `sdk-v0.2.0` | SDK TS republié, Node 24, `utoipa` 6, purge webhooks |
| **I3** | Décisions : crates.io (ADR-0030) et SDK Rust (ADR-0031) | ~1–2 sem. | — (docs seules) | Politique de publication actée |
| **I4** | Préparation technique crates.io (`core` + `eligibility`) | ~2 sem. | v0.9.0 | `cargo package` et docs.rs verts, MSRV + semver-checks en CI |
| **I5** | Publication crates.io + `reqwest` 0.13 | ~2 sem. | v0.9.1 + crates publiées | `carbonfr-core` / `carbonfr-eligibility` sur crates.io et docs.rs |
| **I6** | SDK Rust `carbonfr-sdk` 0.1 | ~2–3 sem. | `rust-sdk-v0.1.0` | SDK publié, parité avec le SDK TS |
| **I7** | `sqlx` 0.9 et décisions de méthodologie | ~2–3 sem. | v0.10.0 | Dette de dépendances soldée, 4 décisions de fond actées |

Les échéances datées (TRV, veille réglementaire, snapshots) sont listées à part, [plus bas](#échéances-datées-et-obligations-récurrentes) : elles s'insèrent dans l'itération en cours quand elles tombent.

---

## I0 — Vider la file de PR, livrer et déployer (~1 semaine)

**Objectif** : repartir d'une base propre ; mettre le correctif de sécurité `rustls` en prod ; livrer la révocation de clés et la désactivation des webhooks.

- [x] Merger `docs/derive-2026-09` (#98, dérive doc ↔ code), puis **vérifier** que les dérives listées par l'état des lieux sont bien toutes closes (dont le tableau README : `/v1/stats`, refus de `acv-ademe@2` par `/v1/mix`).
- [x] Merger `ci/durcissement-2026-09` (#97) (permissions minimales, actions épinglées par SHA, issue d'alerte si le scan planifié échoue).
- [x] **Release v0.7.2** (#99, déployée le 2026-09-23) (`rustls` 0.23.45, RUSTSEC-2026-0285) : PR `chore(release)`, tag, image GHCR, **déploiement** selon la procédure de la mémoire locale `prod-vps-kovelt-acces`.
- [x] Rebaser puis merger `feat/revoke-key` (#100) (migration 0013 : FK `webhook_subscription → api_key` `ON DELETE CASCADE`), puis `feat/webhook-auto-disable` (#101, migration 0014).
- [x] Vérifier que `status`/`disabled_at` de `GET /v1/webhooks` sont bien **additifs** (diff du snapshot OpenAPI v0.7.2 → main : ajouts seulement) pour les clients du SDK TS 0.1.0 déjà publié (champs ignorés, rien de retiré).
- [x] **Release v0.8.0** (#102, déployée le 2026-09-23 ; migrations 0013/0014 appliquées) (migrations 0013 + 0014) : dump de la base juste avant le déploiement (comme avant la 0.2.1), déploiement, contrôle des migrations au démarrage.
- [ ] Fermer les issues de veille conclues #76 et #81.

**Sortie** : `GET /v1/openapi.json` annonce `0.8.0` en prod ; migrations 0013/0014 appliquées sans erreur ; CI verte ; 0 PR ouverte ; #76/#81 fermées.
**Risques** : la migration 0013 purge d'éventuels abonnements orphelins (clé supprimée à la main) — dump préalable obligatoire.

## I1 — Confiance dans la donnée et l'exploitation (~2 semaines → v0.8.1)

**Objectif** : corriger la donnée publiée déjà périmée et rendre l'exploitation vérifiable, pas seulement « en place ».

- [x] **TRV 2026-H2** (millésime `2026-H2` + sélection par horodatage, addendum ADR-0023) (en premier : `/v1/price` sert une grille périmée depuis le 1/8/2026). Le code le signale lui-même (`crates/core/src/domain/price.rs`, caveat « +3,04 % au 1/8/2026 — à re-millésimer »). Sourcer la délibération CRE du TURPE 7 revalorisé et une éventuelle réindexation de l'accise ; créer un **nouveau millésime** `2026-H2` sans modifier `trv_2026()` (versions portées par la donnée) ; addendum ADR-0023 ; CHANGELOG.
- [x] **Restauration testée** (2026-09-23 : archive o2switch → PG 17.11 jetable, restauration 5 s sans erreur, comptages conformes ; procédure dans `deploy/README.md` §4) : récupérer l'archive nocturne sur o2switch, la déchiffrer, restaurer le dump carbon-fr dans un PostgreSQL 17 jetable, comparer les comptes de lignes (`measurement`, `api_key`, `webhook_subscription`). Documenter la procédure dans `deploy/README.md` (**sans aucun secret** : chemins, commandes, RPO = 24 h, durée mesurée). Les sauvegardes étaient cassées du 2026-06-21 au 2026-09-23 : un test de restauration réel est le seul moyen de s'assurer qu'elles marchent vraiment.
- [x] **CI en PostgreSQL 17** (`postgres:16-alpine` → `postgres:17-alpine`), comme la prod (17.11).
- [ ] **Alerte de fraîcheur du poller** — ⚠️ état au 2026-09-23 : les règles existent déjà sur le VPS (versionnées dans `deploy/prometheus/alerts.yml`) et Uptime Kuma sonde `/health` et la fraîcheur, **mais aucun canal de notification n'est configuré** (ni Alertmanager, ni notification Kuma) : reste à relier une notification (action Morgan). Formule de l'ADR-0022 : `time() - carbonfr_poller_last_success_timestamp_seconds > 2 × intervalle de poll`) dans le Prometheus du VPS ; la documenter dans `deploy/README.md` ; la tester en arrêtant volontairement le poller sur une instance de test.
- [x] **Rejeu live ENTSO-E** (2026-09-23 : A11/A75 et A44 verts, signes des flux cohérents) (`cargo test -p carbonfr-adapter-entsoe --test live -- --ignored`, token requis) : jamais rejoué depuis le correctif du parseur A03 (0.7.0) ni les montées de `quick-xml`. Dater le rejeu dans `crates/adapter-entsoe/src/lib.rs`.
- [x] **Dates réglementaires périmées dans `ruleset.rs`** (mises à jour au 2026-09-23, sources contre-vérifiées) (`legal_basis` : échéance du 30/06/2026 dépassée, révision RFNBO glissée à l'automne). ⚠️ Texte **servi** par `/v1/eligibility/rulesets` : reformuler à partir des sources de la veille #90, avec une relecture de neutralité (ADR-0026).

**Sortie** : `/v1/price` sert le millésime 2026-H2, avec ses sources citées ; une restauration réelle est consignée dans `deploy/README.md` ; CI verte en PG17 ; alerte déclenchée en test ; rejeu ENTSO-E daté.

## I2 — Hygiène DX et dépendances légères (~1–2 semaines → v0.8.2 + `sdk-v0.2.0`)

**Objectif** : rattraper ce que voient les consommateurs (SDK, runtimes) et traiter les dépendances à faible risque.

- [ ] **SDK TypeScript 0.2.0** : 6 fonctionnalités livrées depuis `sdk-v0.1.0` (RFC 9457, `/price`, `/cost-reference`, éligibilité, GATE de neutralité, `share-clim@1`) plus `status` des webhooks, mais npm est toujours en 0.1.0. Relire `client.ts`/`types.ts` contre l'OpenAPI, bump, tag `sdk-v0.2.0`, vérifier le trusted publishing npm en conditions réelles.
- [ ] **Node 20 → 24** (Node 20 est en fin de vie depuis le 2026-04-30) dans `ci.yml` et `release-sdk.yml` ; aligner `engines.node` du SDK sur ce que teste la CI.
- [ ] **`utoipa` 5 → 6** (sorti le 2026-09-22 ; l'OpenAPI reste en 3.1 par défaut), PR isolée : régénérer le snapshot et **relire le diff ligne à ligne**. Le champ `servers` de l'OpenAPI fait l'objet d'une **PR séparée**, après (principe 5).
- [ ] **Purge des webhooks désactivés** (laissée ouverte par l'addendum ADR-0016 : ils comptent dans le quota de 50 par clé) et **tests hermétiques d'`adapter-webhook`** (serveur de test : succès, retry sur 5xx, timeout — aujourd'hui 2 tests seulement).
- [ ] **`deny.toml`** : documenter en `skip` les doublons de versions connus (`tower-http` 0.6, `webpki-roots` 0.26, `rand` 0.8…), avec la crate qui les tire. Le commentaire sur les crates non maintenues est **exact** (cargo-deny ≥ 0.16 les refuse par défaut) : le rendre explicite (`unmaintained = "all"`) plutôt que le corriger.
- [ ] **Dependabot** : relancer la montée `sqlx` 0.9 (PR #25 fermée sans merge le 2026-06-17, donc ignorée depuis) et regarder dans les journaux Dependabot (GitHub → Insights) pourquoi `reqwest` 0.13 n'a jamais été proposé.

**Sortie** : `npm view @carbon-fr/sdk version` = 0.2.0 ; Node 24 en CI ; `utoipa` 6.x dans `Cargo.lock` ; `cargo deny check` sans avertissement non documenté.

## I3 — Décisions : crates.io (ADR-0030) et SDK Rust (ADR-0031) (~1–2 semaines, docs seules)

**Objectif** : trancher **avant** toute préparation technique. Itération 100 % décisionnelle : aucune ligne de code hors `docs/`. L'ADR-0030 d'abord ; l'ADR-0031 ensuite, car il demande une petite recherche (écosystème SSE en Rust).

- [ ] **ADR-0030 — politique de publication crates.io**, qui amende l'ADR-0019 (il dit aujourd'hui l'inverse : « crates non publiées ») et renoue avec l'ADR-0001 (le `core` « conçu pour être publiable »). Décisions à prendre : voir [Préparation crates.io](#préparation-cratesio) ci-dessous.
- [ ] **Addendum ADR-0019** : 5ᵉ axe de versionnement (versions crates.io), renvoyant à l'ADR-0030.
- [ ] **ADR-0031 — conception du SDK Rust** : nom (`carbonfr-sdk`, libre sur crates.io), client HTTP (`reqwest` 0.13 avec provider TLS explicite, cf. I5, ou client plus minimal), écriture manuelle ou génération depuis l'OpenAPI, périmètre v1 = **parité avec le SDK TS**. ⚠️ Avant de choisir, vérifier s'il existe une crate **SSE** minimale et maintenue ; sinon, chiffrer le parsing SSE fait maison (le SDK TS s'appuie sur `fetch` natif, Rust n'a pas d'équivalent standard).

**Sortie** : ADR-0030 et ADR-0031 mergés (statut *Accepté*), index des ADR à jour, aucune contradiction ouverte avec l'ADR-0019.

## I4 — Préparation technique crates.io : `carbonfr-core` + `carbonfr-eligibility` (~2 semaines → v0.9.0)

**Objectif** : rendre les deux crates publiables avec une documentation propre et sans rupture SemVer silencieuse — **sans publier**.

- [ ] Corriger les **2 liens rustdoc** de `core` vers des items privés (`forecast.rs` → `BAND_QUANTILE`, `price.rs` → `Filiere::merit_order`) et les **8 annotations** `[FAIT]`/`[ESTIMATION]` de `eligibility/src/ruleset.rs` que rustdoc prend pour des liens cassés. docs.rs publierait quand même (il ne refuse pas les avertissements), mais avec des liens morts.
- [ ] **Métadonnées** des deux crates : `readme`, `documentation`, `homepage`, `keywords` (≤ 5), `categories` (taxonomie crates.io), `rust-version` ; les 9 autres membres gardent `publish = false`.
- [ ] **Dépendance interne versionnée** : `carbonfr-core = { path = "crates/core", version = "…" }` dans `[workspace.dependencies]` (sinon `cargo package -p carbonfr-eligibility` échoue).
- [ ] **`#[non_exhaustive]`** sur les enums publics qui grandiront (au minimum les erreurs : `SourceError`, `RepositoryError`, `ForecastError`, `ApplicationError`, `WebhookUrlError`) selon la règle fixée par l'ADR-0030 ; **décision documentée** pour les structs à champs publics (`GenerationMix`, `Measurement`…).
- [ ] Documenter dans le README de chaque crate les types tiers exposés par l'API publique (`async-trait`, `time::OffsetDateTime`), et ce qui reste **hors** des crates publiées (migrations SQL, données de la carte `/hydrogene`).
- [ ] Feature flags : aucun aujourd'hui. Acter dans l'ADR-0030 si une feature optionnelle (ex. `serde` sur les types du domaine) est prévue avant une 1.0, pour ne pas l'ajouter plus tard de façon cassante.
- [ ] **CI** : job MSRV (valeur fixée par l'ADR-0030, plancher de fait ≈ 1.85 avec l'édition 2024), job `cargo-semver-checks` (référence posée), job `RUSTDOCFLAGS="-D warnings" cargo doc` sur les deux crates.

**Sortie** : `cargo package -p carbonfr-core` et `-p carbonfr-eligibility` passent ; `cargo doc -D warnings` passe sur les deux ; jobs MSRV, semver-checks et rustdoc verts ; workspace complet toujours vert.

## I5 — Publication crates.io et `reqwest` 0.13 (~2 semaines → v0.9.1)

**Objectif** : première publication (irréversible pour un numéro de version donné) ; préparer le terrain HTTP du SDK Rust. **Dans cet ordre** : publier, laisser passer le suivi docs.rs, et seulement ensuite ouvrir la PR `reqwest`.

- [ ] **Première publication, manuelle** (le Trusted Publishing de crates.io ne peut être configuré que sur une crate **déjà publiée**) : revérifier que les noms sont toujours libres, `cargo publish --dry-run`, puis `cargo publish -p carbonfr-core` et `-p carbonfr-eligibility` (dans l'ordre des dépendances) avec un **jeton crates.io ponctuel**, révoqué juste après.
- [ ] **Puis Trusted Publishing** pour les versions suivantes : déclarer le dépôt (`Kovelt/carbon-fr`, workflow `release-crates.yml`) comme *trusted publisher* sur la page de chaque crate ; workflow `release-crates.yml` avec OIDC GitHub (`id-token: write`, comme `release-sdk.yml` pour npm), déclenché par le tag `vX.Y.Z`.
- [ ] **Suivi docs.rs pendant 24–48 h** (l'environnement de build peut différer du local ; remédiation = version patch). Badges crates.io/docs.rs dans le README.
- [ ] **Ensuite seulement, `reqwest` 0.12 → 0.13** : feature `rustls-tls` renommée ; `query` devient une feature à activer (adapters ODRÉ, ENTSO-E, météo) ; **fixer explicitement le provider crypto `ring`** (cohérent avec `sqlx` en `tls-rustls-ring`) pour éviter un double provider `ring`/`aws-lc-rs` qui paniquerait au démarrage. Tester d'abord le resolver anti-SSRF des webhooks (`PublicOnlyResolver`, ADR-0016), puis **démarrer le binaire complet**, pas seulement `cargo check`.

**Sortie** : fiches crates.io et pages docs.rs des deux crates en ligne ; trusted publisher configuré (plus aucun jeton nécessaire pour les versions suivantes) ; `reqwest` 0.13 en prod, doublon `tower-http` résorbé, tests anti-SSRF verts.

## I6 — SDK Rust `carbonfr-sdk` 0.1 (~2–3 semaines → `rust-sdk-v0.1.0`)

**Objectif** : l'équivalent Rust du SDK TS, publié sur crates.io — sur `reqwest` 0.13, déjà en place depuis I5 (sans quoi le SDK serait à retravailler tout de suite).

- [ ] Relire intégralement `sdk/typescript/src/{client,types}.ts` pour borner la surface v1 (non fait exhaustivement par la recherche).
- [ ] Nouveau membre du workspace (chemin fixé par l'ADR-0031) : client configurable (URL de base, clé API), erreurs typées (RFC 9457), intensité (`now`/`date`/`stats`), webhooks (créer/lister/supprimer), **flux SSE** en `Stream` Rust (le point le plus délicat : à commencer en premier).
- [ ] Tests hermétiques (serveur de test local, aucun appel réseau), `examples/` exécutables ; le reste de `/v1` (prix, coût, éligibilité, météo, échanges, renouvelable) couvert ou explicitement marqué « à venir » dans la doc.
- [ ] Publication `carbonfr-sdk` 0.1.0 : première publication **manuelle** (jeton ponctuel, comme en I5), puis *trusted publisher* déclaré pour cette crate ; tag `rust-sdk-v0.1.0` (axe de versionnement propre, comme le SDK TS) ; README : section « SDK officiels ».

**Sortie** : `cargo add carbonfr-sdk` compile dans un projet vide ; parité TS atteinte sur intensité, webhooks et SSE ; docs.rs vert.

## I7 — `sqlx` 0.9 et décisions de méthodologie (~2–3 semaines → v0.10.0)

**Objectif** : solder la montée la plus risquée (toute la persistance), puis trancher les chantiers de fond laissés ouverts par les ADR.

- [ ] **`sqlx` 0.8 → 0.9** (PR isolée) : d'après le CHANGELOG de `sqlx`, les requêtes construites dynamiquement doivent passer par `AssertSqlSafe` (≥ 4 appels `sqlx::query(&sql)` et 5 `QueryBuilder` dans `adapter-postgres`) ; suite d'intégration Postgres complète **en PG17** ; `cargo tree -d` : doublons `webpki-roots`/`hashbrown`/`rand` réduits.
- [ ] **Partitionnement de `measurement`** (ADR-0004 : « à reconsidérer maintenant que l'historique est ingéré ») : mesurer d'abord (taille, `EXPLAIN ANALYZE` des requêtes `/date` et `/stats` sur de larges intervalles), puis trancher dans un addendum ; si oui, migration testée sur une copie de prod avec fenêtre de maintenance.
- [ ] **Facteurs ADEME** : l'ADR-0008 promettait la Base Empreinte V23.6 sous le nom `acv-ademe@2`, mais ce numéro a servi à la vue consommation (ADR-0010). Addendum ADR-0008 : calendrier d'un **`acv-ademe@3`** (nouvelle table de facteurs + revalidation des backtests). Effort estimé important : décider ici, implémenter plus tard.
- [ ] **Cadence de revue LCOE** (ADR-0024 : aucune fréquence fixée ; millésimes 2021–2024) : addendum avec la date de la prochaine revue.
- [ ] **Critère de déclenchement d'un `acv-ademe` régional** (ADR-0010 : « dérivation sur dérivation » reportée) : addendum.
- [ ] **Types du SDK TS générés depuis l'OpenAPI** : décider (outiller ou écarter explicitement, avec justification) ; même question pour publier la doc rustdoc des crates non publiées.

**Sortie** : `sqlx` 0.9 en prod ; 4 addenda mergés (ADR-0004, 0008, 0010, 0024) ; décision codegen actée.

---

## Préparation crates.io

### État au 2026-09-23 (vérifié)

| Point | État | Preuve |
|---|---|---|
| Noms `carbonfr-core`, `carbonfr-eligibility`, `carbonfr-sdk` (et 12 variantes) | ✅ tous libres | API crates.io, 15 requêtes |
| `cargo package -p carbonfr-core` | ✅ passe (aucune dépendance interne) | 48 fichiers, 84,7 Kio compressés |
| `cargo package -p carbonfr-eligibility` | ❌ dépendance interne `carbonfr-core` sans `version` | `[workspace.dependencies]` |
| Doc rustdoc (`cargo doc -D warnings`) | ❌ `core` : 2 liens vers des items privés ; `eligibility` : 8 liens cassés (docs.rs publierait avec des liens morts) | `forecast.rs:84`, `price.rs:250`, `ruleset.rs` |
| Métadonnées (`readme`, `keywords`, `categories`, `rust-version`…) | ❌ absentes des 11 `Cargo.toml` ; `publish = false` partout | `Cargo.toml` |
| Hygiène SemVer | ⚠️ 0 enum `#[non_exhaustive]` sur 25 ; structs à champs publics | `core`, `eligibility` |
| MSRV | ❌ non déclarée ni testée (CI sur `stable` flottant) | `ci.yml` |
| Politique | ❌ l'ADR-0019 exclut aujourd'hui toute publication | ADR-0019 |

### Ce que l'ADR-0030 doit trancher (recommandations)

| Décision | Recommandation |
|---|---|
| Périmètre | **`carbonfr-core` + `carbonfr-eligibility`** (libs pures, sans IO) ; plus tard **`carbonfr-sdk`** (ADR-0031). **Jamais** `bin/server` ni les 8 adapters (couplés à l'infra ; `adapter-gbdt` = modèle non servi, ADR-0012). |
| Versions | Couplées à la version du workspace (conséquence directe de la dépendance interne versionnée) : un tag `vX.Y.Z` publie les deux crates à `X.Y.Z`. Le SDK Rust garde son axe propre (`rust-sdk-v*`), comme le SDK TS. |
| SemVer des libs | `#[non_exhaustive]` sur les enums d'erreur et les catalogues extensibles ; `cargo-semver-checks` bloquant en CI ; en 0.x, une rupture = version *minor*. |
| MSRV | Déclarée dans `[workspace.package]`, testée par un job CI dédié, relevée seulement dans une version *minor*. |
| Publication | Première version de chaque crate publiée **à la main** avec un jeton ponctuel révoqué aussitôt (le Trusted Publishing ne se configure que sur une crate existante), puis **Trusted Publishing** (OIDC GitHub, aucun jeton stocké) ; `--dry-run` systématique ; propriétaire = Morgan/Kovelt (gouvernance solo, ADR-0027). |
| Contenu | Migrations SQL et données `/hydrogene` hors des crates publiées ; types tiers exposés documentés ; feature flags (aucun aujourd'hui) : prévoir ou exclure explicitement une feature `serde`. |

### Séquence

**I3** : ADR-0030/0031 → **I4** : préparation, sans publier → **I5** : publication de `core` + `eligibility` → **I6** : publication de `carbonfr-sdk`. Aucune étape ne commence avant que la précédente soit mergée.

---

## Échéances datées et obligations récurrentes

| Quand | Quoi | Action |
|---|---|---|
| **Dépassée (1/8/2026)** | Revalorisation TURPE +3,04 % | Millésime TRV 2026-H2 → **I1** |
| **Dépassée (2026-04-30)** | Fin de vie de Node 20 (CI + publication du SDK) | Node 24 → **I2** |
| Chaque nuit | Sauvegarde du serveur (`backup.sh`, archive chiffrée vers o2switch) | Alerte e-mail en cas d'échec active depuis le 2026-09-23 ; tester une restauration → **I1**, puis une fois par trimestre |
| Le 1er de chaque mois | Veille hydrogène automatisée (issue) | Fermer l'issue du mois précédent : ajouter cette étape à la routine |
| **Oct.–nov. 2026** | Révision RFNBO annoncée « à l'automne » | Revérifier les signaux (veille #90) ; H3 seulement si le texte est **adopté** |
| **Fin 2026** | Proposition RED IV annoncée | Veille (quota RFNBO 42 %, ouverture au bas-carbone) |
| **1er février 2027** (habituellement annuel) | Nouveau barème TRVE / accise (délibération CRE de janvier) | Nouveau millésime TRV, addendum ADR-0023 |
| **1er août** (habituellement annuel) | Revalorisation du TURPE | Millésime TRV de mi-année |
| Tous les ~6 mois | Nouvel instantané European Hydrogen Observatory (dernier : Dec2025, toujours le plus récent au 2026-09-23) | Vérifier puis rafraîchir `/hydrogene` (ADR-0029) |
| Annuel (à fixer en I7) | Sources LCOE | Revue des millésimes (ADR-0024) |
| **01/07/2028** | Évaluation contraignante du nucléaire (art. 3 du 2025/2359) | Revoir le caveat nucléaire (`legal_basis`/`disclaimer`) |
| **2028-11-09** | Fin de vie de PostgreSQL 16 | Sans objet une fois la CI en PG17 (I1) |
| **2030-01-01** | Bascule horaire RFNBO (H7, déjà paramétrée) | Se recale via H3 si le texte révisé change la date |

## En attente d'un déclencheur

| Chantier | Déclencheur |
|---|---|
| **H3** — activer `rfnbo:2026-revision` | Texte RFNBO révisé **adopté** par le Collège (un projet ne suffit pas) |
| **H5** — branche EUA ; **H7** — bascule horaire | Flux de prix EUA utile à un autre usage (H5) ; 2030-01-01 ou texte révisé (H7) |
| **H6 v2** — sites ADEME `hyd01-sites` sur la carte | Réponse écrite de cdo@ademe.fr sur la licence (**demande à envoyer**) |
| `UsageMeter` persistant | Premier consommateur commercial qui sature le quota (addendum ADR-0015) |
| Délivrance de clés en libre-service (e-mail, lien magique, rotation, `/v1/keys`) | Besoin réel de clés hors opérateur |
| Site statique o2switch | Décision produit de Morgan (ou clore l'item de l'ADR-0007) |
| Servir `gbdt@1` / `share-meteo@2` | Nouveau backtest qui franchit la GATE (ADR-0012 / ADR-0028) |
| Phase B de gouvernance (revue Code Owners obligatoire) | Première contribution externe (ADR-0027) |

## Actions hors code (Morgan)

- [ ] Comparer l'empreinte SSH du VPS depuis le fixe : `ssh-keygen -lF 46.225.108.44` doit afficher `SHA256:4+SU2XlJ80JLKah4ySqu1AD2RxtaleMBAuXCEx+/rZ8`.
- [ ] Journaux Dependabot (GitHub → Insights → Dependency graph → Dependabot) : pourquoi `reqwest` 0.13 n'a jamais été proposé.
- [ ] Activer « Automatically delete head branches » (Settings → General → Pull Requests).
- [ ] Envoyer la demande de licence à cdo@ademe.fr (débloque H6 v2).
- [ ] Avant I5 : créer ou vérifier le compte crates.io ; générer un jeton ponctuel pour la première publication (le révoquer juste après), puis déclarer `Kovelt/carbon-fr` comme *trusted publisher* sur `carbonfr-core` et `carbonfr-eligibility`.
- [ ] En I6 : même chose pour `carbonfr-sdk` (déclaration distincte, possible seulement après sa première publication).
- [ ] I1 : fournir le token ENTSO-E pour le rejeu live, ou le lancer soi-même.
- [ ] I1 : dans Uptime Kuma (`status.<domaine>`), créer un canal de notification (e-mail SMTP, déjà configuré pour les sauvegardes) et l'attacher aux sondes « API /health » et « fraîcheur données » ; optionnellement une 3ᵉ sonde sur les alertes Prometheus actives.
