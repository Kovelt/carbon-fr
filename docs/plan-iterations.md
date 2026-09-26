# Plan de la suite — itérations I0 → I10 (à partir du 2026-09-23)

- **Statut** : document vivant — cocher les cases au fil des PR, dater chaque révision en tête.
- **Dernière mise à jour** : 2026-09-26 — **I0 à I7 livrées** (v0.9.5 en prod) ; **nouveau cycle I8 → I10 planifié** à partir d'un second état des lieux (8 volets, 57 constats vérifiés) : donnée régionale à combler, risques prod à borner, index qualitatif, export CSV, compression, SDK TS, documentation. **I8 démarrée** : #131 (`/v1/factors`) et #132 (`servers` OpenAPI) mergées ; quota ODRÉ réel (PROD-3) implémenté, en PR. Restent : actions hors code de Morgan (cf. « Actions hors code »), échéances datées et chantiers « en attente d'un déclencheur ».
- **Sources** : état des lieux multi-agents du 2026-09-23 (constats revérifiés contre le code), recherche en 4 volets (préparation crates.io, backlog consolidé des ADR/roadmaps, montées majeures des dépendances, échéances datées), 3 plans concurrents (« fiabilité d'abord », « adoption d'abord », « valeur métier d'abord ») départagés par un juge. Base retenue : **valeur métier d'abord**, avec les greffes des deux autres.
- **Sources du cycle I8 → I10 (2026-09-26)** : second état des lieux multi-agents en 8 volets (items ouverts des docs, qualité/tests, dépendances, exploitation en prod — lecture seule —, surface produit vs carbonintensity.org.uk / Electricity Maps, sécurité, DX, performance/données) ; 57 constats retenus après vérification par deux lentilles (exactitude, pertinence/conformité ADR), 1 réfuté ; 3 plans concurrents (mêmes angles) départagés par 3 juges (utilisateur de l'API, mainteneur solo, cohérence architecturale) : **valeur métier d'abord** retenu à 2 voix sur 3, avec greffes (risques prod avancés en I8, gouvernance crates.io) ; critique de complétude appliquée (échéances de décembre/février, SECURITY.md, mix historique, règle ADR-0030 §3 sur les DTO du SDK Rust). Les identifiants de constats (`PROD-1`, `QUAL-1`…) renvoient à cet état des lieux.
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
| **I7** | `sqlx` 0.9 et décisions de méthodologie | ~2–3 sem. | v0.9.3 (visée v0.10.0) | Dette de dépendances soldée, 4 décisions de fond actées |
| **I8** | Donnée régionale comblée, risques prod bornés, petites victoires API | ~3 sem. | v0.10.0 | Historique régional `acv-ademe` comblé + self-heal régional ; quota ODRÉ instrumenté ; `/v1/factors` cohérent ; SSE plafonné + timeout HTTP ; `?region=all`, `/v1/mix` enrichi, `/v1/regions` ; `servers` OpenAPI ; gouvernance à jour |
| **I9** | Ce qui différencie : index qualitatif, export, compression, live fiable | ~2–3 sem. | v0.11.0 | `index` servi (ADR-0033 d'abord) ; export CSV ; compression HTTP ; parcours local documenté ; SDK TS reconnecte au flux SSE |
| **I10** | Filet SDK TS, doc OpenAPI, rattrapage documentaire, échéances | ~2–3 sem. | v0.12.0 | Parité SDK TS ↔ OpenAPI en CI ; exemples OpenAPI ; README/ARCHITECTURE/CONTRIBUTING à jour ; revue LCOE, backfill consolidé, TRV 2027 |

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
- [x] Fermer les issues de veille conclues #76 et #81. — fermées (constaté le 2026-09-25).

**Sortie** : `GET /v1/openapi.json` annonce `0.8.0` en prod ; migrations 0013/0014 appliquées sans erreur ; CI verte ; 0 PR ouverte ; #76/#81 fermées.
**Risques** : la migration 0013 purge d'éventuels abonnements orphelins (clé supprimée à la main) — dump préalable obligatoire.

## I1 — Confiance dans la donnée et l'exploitation (~2 semaines → v0.8.1)

**Objectif** : corriger la donnée publiée déjà périmée et rendre l'exploitation vérifiable, pas seulement « en place ».

- [x] **TRV 2026-H2** (millésime `2026-H2` + sélection par horodatage, addendum ADR-0023) (en premier : `/v1/price` sert une grille périmée depuis le 1/8/2026). Le code le signale lui-même (`crates/core/src/domain/price.rs`, caveat « +3,04 % au 1/8/2026 — à re-millésimer »). Sourcer la délibération CRE du TURPE 7 revalorisé et une éventuelle réindexation de l'accise ; créer un **nouveau millésime** `2026-H2` sans modifier `trv_2026()` (versions portées par la donnée) ; addendum ADR-0023 ; CHANGELOG.
- [x] **Restauration testée** (2026-09-23 : archive o2switch → PG 17.11 jetable, restauration 5 s sans erreur, comptages conformes ; procédure dans `deploy/README.md` §4) : récupérer l'archive nocturne sur o2switch, la déchiffrer, restaurer le dump carbon-fr dans un PostgreSQL 17 jetable, comparer les comptes de lignes (`measurement`, `api_key`, `webhook_subscription`). Documenter la procédure dans `deploy/README.md` (**sans aucun secret** : chemins, commandes, RPO = 24 h, durée mesurée). Les sauvegardes étaient cassées du 2026-06-21 au 2026-09-23 : un test de restauration réel est le seul moyen de s'assurer qu'elles marchent vraiment.
- [x] **CI en PostgreSQL 17** (`postgres:16-alpine` → `postgres:17-alpine`), comme la prod (17.11).
- [x] **Alerte de fraîcheur du poller** — ⚠️ état au 2026-09-23 : les règles existent déjà sur le VPS (versionnées dans `deploy/prometheus/alerts.yml`) et Uptime Kuma sonde `/health` et la fraîcheur, **mais aucun canal de notification n'est configuré** (ni Alertmanager, ni notification Kuma) : reste à relier une notification (action Morgan). Formule de l'ADR-0022 : `time() - carbonfr_poller_last_success_timestamp_seconds > 2 × intervalle de poll`) dans le Prometheus du VPS ; la documenter dans `deploy/README.md` ; la tester en arrêtant volontairement le poller sur une instance de test. — **Fait le 2026-09-25** : canal e-mail Uptime Kuma configuré par Morgan (par défaut, rattaché aux sondes `/health` et fraîcheur, test reçu) ; règle Prometheus `CarbonfrDataStale` (âge de la dernière mesure > 2 h) ajoutée. Les alertes Prometheus restent sans routage (pas d'Alertmanager) : la sonde de fraîcheur Kuma couvre le cas « donnée figée ».
- [x] **Rejeu live ENTSO-E** (2026-09-23 : A11/A75 et A44 verts, signes des flux cohérents) (`cargo test -p carbonfr-adapter-entsoe --test live -- --ignored`, token requis) : jamais rejoué depuis le correctif du parseur A03 (0.7.0) ni les montées de `quick-xml`. Dater le rejeu dans `crates/adapter-entsoe/src/lib.rs`.
- [x] **Dates réglementaires périmées dans `ruleset.rs`** (mises à jour au 2026-09-23, sources contre-vérifiées) (`legal_basis` : échéance du 30/06/2026 dépassée, révision RFNBO glissée à l'automne). ⚠️ Texte **servi** par `/v1/eligibility/rulesets` : reformuler à partir des sources de la veille #90, avec une relecture de neutralité (ADR-0026).

**Sortie** : `/v1/price` sert le millésime 2026-H2, avec ses sources citées ; une restauration réelle est consignée dans `deploy/README.md` ; CI verte en PG17 ; alerte déclenchée en test ; rejeu ENTSO-E daté.

## I2 — Hygiène DX et dépendances légères (~1–2 semaines → v0.8.2 + `sdk-v0.2.0`)

**Objectif** : rattraper ce que voient les consommateurs (SDK, runtimes) et traiter les dépendances à faible risque.

- [x] **SDK TypeScript 0.2.0** : 6 fonctionnalités livrées depuis `sdk-v0.1.0` (RFC 9457, `/price`, `/cost-reference`, éligibilité, GATE de neutralité, `share-clim@1`) plus `status` des webhooks, mais npm est toujours en 0.1.0. Relire `client.ts`/`types.ts` contre l'OpenAPI, bump, tag `sdk-v0.2.0`, vérifier le trusted publishing npm en conditions réelles. — **publié le 2026-09-23** par trusted publishing (provenance signée, publié par « GitHub Actions ») ; piège rencontré : la permission « publish » du trusted publisher doit être cochée (sinon `E403 OIDC permission denied`), documenté dans `release-sdk.yml`.
- [x] **Node 20 → 24** (CI sur Node 22 = plancher `engines`, publication sur Node 24) (Node 20 est en fin de vie depuis le 2026-04-30) dans `ci.yml` et `release-sdk.yml` ; aligner `engines.node` du SDK sur ce que teste la CI.
- [x] **`utoipa` 5 → 6** (sorti le 2026-09-22 ; l'OpenAPI reste en 3.1 par défaut), PR isolée : régénérer le snapshot et **relire le diff ligne à ligne**. Le champ `servers` de l'OpenAPI fait l'objet d'une **PR séparée**, après (principe 5).
- [x] **Purge des webhooks désactivés** (laissée ouverte par l'addendum ADR-0016 : ils comptent dans le quota de 50 par clé) — après `CARBONFR_WEBHOOK_PURGE_DAYS` jours (défaut 30), tâche de fond toutes les 6 h.
- [x] **Tests hermétiques d'`adapter-webhook`** (serveur de test : succès, retry sur 5xx, timeout — aujourd'hui 2 tests seulement) — 8 tests, dont la redirection non suivie vérifiée par mutation.
- [x] **`deny.toml`** (origine des doublons documentée, `unmaintained` explicite) : documenter en `skip` les doublons de versions connus (`tower-http` 0.6, `webpki-roots` 0.26, `rand` 0.8…), avec la crate qui les tire. Le commentaire sur les crates non maintenues est **exact** (cargo-deny ≥ 0.16 les refuse par défaut) : le rendre explicite (`unmaintained = "all"`) plutôt que le corriger.
- [x] **Dependabot** (diagnostiqué le 2026-09-23, sans relance) : relancer la montée `sqlx` 0.9 (PR #25 fermée sans merge le 2026-06-17, donc ignorée depuis) et regarder dans les journaux Dependabot (GitHub → Insights) pourquoi `reqwest` 0.13 n'a jamais été proposé. **`reqwest` 0.13** : la feature `rustls-tls` n'existe plus (renommée `rustls`, qui tire `aws-lc-rs`) → la simple montée de version ne se résout pas et Dependabot l'abandonne en silence ; montée manuelle en **I5**. **`sqlx` 0.9** : toutes nos features existent (MSRV 1.94 < 1.98) mais l'API change ; ignorée depuis la fermeture de #25 → montée manuelle en **I7**, sans rouvrir une PR Dependabot qui resterait rouge d'ici là.

**Sortie** : `npm view @carbon-fr/sdk version` = 0.2.0 ; Node 24 en CI ; `utoipa` 6.x dans `Cargo.lock` ; `cargo deny check` sans avertissement non documenté.

## I3 — Décisions : crates.io (ADR-0030) et SDK Rust (ADR-0031) (~1–2 semaines, docs seules)

**Objectif** : trancher **avant** toute préparation technique. Itération 100 % décisionnelle : aucune ligne de code hors `docs/`. L'ADR-0030 d'abord ; l'ADR-0031 ensuite, car il demande une petite recherche (écosystème SSE en Rust).

- [x] **ADR-0030 — politique de publication crates.io** ([rédigé](adr/0030-politique-publication-crates-io.md) : MSRV réelle **1.88** — fixée par `time`, pas par l'édition 2024 —, `#[non_exhaustive]` décidé enum par enum, politique de `yank`), qui amende l'ADR-0019 (il dit aujourd'hui l'inverse : « crates non publiées ») et renoue avec l'ADR-0001 (le `core` « conçu pour être publiable »). Décisions à prendre : voir [Préparation crates.io](#préparation-cratesio) ci-dessous.
- [x] **Addendum ADR-0019** : 5ᵉ axe de versionnement (versions crates.io), renvoyant à l'ADR-0030.
- [x] **ADR-0031 — conception du SDK Rust** ([rédigé](adr/0031-conception-sdk-rust.md) : écriture manuelle + test de parité, `ring` explicite, SSE sur `eventsource-stream`, MSRV 1.88) : nom (`carbonfr-sdk`, libre sur crates.io), client HTTP (`reqwest` 0.13 avec provider TLS explicite, cf. I5, ou client plus minimal), écriture manuelle ou génération depuis l'OpenAPI, périmètre v1 = **parité avec le SDK TS**. ⚠️ Avant de choisir, vérifier s'il existe une crate **SSE** minimale et maintenue ; sinon, chiffrer le parsing SSE fait maison (le SDK TS s'appuie sur `fetch` natif, Rust n'a pas d'équivalent standard).

**Sortie** : ADR-0030 et ADR-0031 mergés (statut *Accepté*), index des ADR à jour, aucune contradiction ouverte avec l'ADR-0019.

## I4 — Préparation technique crates.io : `carbonfr-core` + `carbonfr-eligibility` (~2 semaines → v0.9.0)

**Objectif** : rendre les deux crates publiables avec une documentation propre et sans rupture SemVer silencieuse — **sans publier**.

- [x] Corriger les **2 liens rustdoc** de `core` vers des items privés (`forecast.rs` → `BAND_QUANTILE`, `price.rs` → `Filiere::merit_order`) et les **8 annotations** `[FAIT]`/`[ESTIMATION]` de `eligibility/src/ruleset.rs` que rustdoc prend pour des liens cassés. docs.rs publierait quand même (il ne refuse pas les avertissements), mais avec des liens morts.
- [x] **Métadonnées** des deux crates : `readme`, `documentation`, `homepage`, `keywords` (≤ 5), `categories` (taxonomie crates.io), `rust-version` ; les 9 autres membres gardent `publish = false`.
- [x] **Dépendance interne versionnée** : `carbonfr-core = { path = "crates/core", version = "…" }` dans `[workspace.dependencies]` (sinon `cargo package -p carbonfr-eligibility` échoue).
- [x] **`#[non_exhaustive]`** sur les enums publics qui grandiront (au minimum les erreurs : `SourceError`, `RepositoryError`, `ForecastError`, `ApplicationError`, `WebhookUrlError`) selon la règle fixée par l'ADR-0030 ; **décision documentée** pour les structs à champs publics (`GenerationMix`, `Measurement`…).
- [x] Documenter dans le README de chaque crate les types tiers exposés par l'API publique (`async-trait`, `time::OffsetDateTime`), et ce qui reste **hors** des crates publiées (migrations SQL, données de la carte `/hydrogene`).
- [x] Feature flags : aucun aujourd'hui. Acter dans l'ADR-0030 si une feature optionnelle (ex. `serde` sur les types du domaine) est prévue avant une 1.0, pour ne pas l'ajouter plus tard de façon cassante.
- [x] **CI** : job MSRV (valeur fixée par l'ADR-0030, plancher de fait ≈ 1.85 avec l'édition 2024), job `cargo-semver-checks` (référence posée), job `RUSTDOCFLAGS="-D warnings" cargo doc` sur les deux crates.

**Sortie** (atteinte : CI de `main` verte sur les 8 jobs le 2026-09-23, dont la 1re exécution du job MSRV 1.88 ; checks requis par le ruleset le 2026-09-24) : `cargo package -p carbonfr-core -p carbonfr-eligibility` passe (les deux ensemble : `eligibility` seule ne peut pas être vérifiée avant la publication de `core`) ; `cargo doc -D warnings` passe sur les deux ; jobs MSRV, semver-checks et rustdoc verts ; workspace complet toujours vert.

## I5 — Publication crates.io et `reqwest` 0.13 (~2 semaines → v0.9.1)

**Objectif** : première publication (irréversible pour un numéro de version donné) ; préparer le terrain HTTP du SDK Rust. **Dans cet ordre** : publier, laisser passer le suivi docs.rs, et seulement ensuite ouvrir la PR `reqwest`.

- [x] **Première publication, manuelle** (**faite le 2026-09-24 en 0.9.1** — la 0.9.0 aurait figé « Not yet published » dans ses README ; piège : crates.io exige un e-mail vérifié ; propriétaire `morgan-voltz`, équipe `Kovelt/maintainers` à ajouter) (le Trusted Publishing de crates.io ne peut être configuré que sur une crate **déjà publiée**) : revérifier que les noms sont toujours libres, `cargo publish --dry-run`, puis `cargo publish -p carbonfr-core` et `-p carbonfr-eligibility` (dans l'ordre des dépendances) avec un **jeton crates.io ponctuel**, révoqué juste après.
- [x] **Puis Trusted Publishing** (workflow `release-crates.yml` livré ; reste la déclaration sur crates.io, par crate) pour les versions suivantes : déclarer le dépôt (`Kovelt/carbon-fr`, workflow `release-crates.yml`) comme *trusted publisher* sur la page de chaque crate ; workflow `release-crates.yml` avec OIDC GitHub (`id-token: write`, comme `release-sdk.yml` pour npm), déclenché par le tag `vX.Y.Z`. — **fait** : déclaré sur les 3 crates ; validé par les publications automatiques de 0.9.2 à 0.9.5 (`release-crates.yml`) et de `carbonfr-sdk` 0.1.1 (`release-rust-sdk.yml`).
- [x] **Suivi docs.rs pendant 24–48 h** (les deux pages construites sans erreur le 2026-09-24 à 06:07 ; badges dans les README des crates et section Rust du README racine) (l'environnement de build peut différer du local ; remédiation = version patch). Badges crates.io/docs.rs dans le README.
- [x] **Ensuite seulement, `reqwest` 0.12 → 0.13** (fait le 2026-09-24 → v0.9.2 : `ring` seul provider, rejeux live ENTSO-E/ODRÉ/météo et démarrage réel verts ; `tower-http` 0.6/0.7 **persiste** — reqwest l'épingle lui-même —, la « sortie » ci-dessous est corrigée en conséquence) : feature `rustls-tls` renommée ; `query` devient une feature à activer (adapters ODRÉ, ENTSO-E, météo) ; **fixer explicitement le provider crypto `ring`** (cohérent avec `sqlx` en `tls-rustls-ring`) pour éviter un double provider `ring`/`aws-lc-rs` qui paniquerait au démarrage. ⚠️ En `rustls-no-provider`, `reqwest` 0.13 vérifie les certificats via `rustls-platform-verifier`, qui lit le **magasin système** : le paquet `ca-certificates` du `Dockerfile` redevient alors indispensable (commentaire à mettre à jour). Tester d'abord le resolver anti-SSRF des webhooks (`PublicOnlyResolver`, ADR-0016), puis **démarrer le binaire complet**, pas seulement `cargo check`.

**Sortie** : fiches crates.io et pages docs.rs des deux crates en ligne ; trusted publisher configuré (plus aucun jeton nécessaire pour les versions suivantes) ; `reqwest` 0.13 en prod, tests anti-SSRF verts (le doublon `tower-http` ne dépend pas de nous : reqwest épingle `tower-http` 0.6).

## I6 — SDK Rust `carbonfr-sdk` 0.1 (~2–3 semaines → `rust-sdk-v0.1.0`)

**Objectif** : l'équivalent Rust du SDK TS, publié sur crates.io — sur `reqwest` 0.13, déjà en place depuis I5 (sans quoi le SDK serait à retravailler tout de suite).

- [x] Relire intégralement `sdk/typescript/src/{client,types}.ts` pour borner la surface v1 (fait le 2026-09-25 : lecture intégrale des 493 + 589 lignes, croisée avec le snapshot OpenAPI 3.1.0 — 26/26 méthodes publiques de `CarbonFr` (client.ts 0.2.0) correspondent 1:1 aux 26 opérations `/v1` en périmètre, hors `health`/`health_ready`).
- [x] Nouveau membre du workspace `crates/sdk` (chemin fixé par l'ADR-0031) : client configurable (URL de base, clé API `Bearer`, timeout REST configurable/désactivable, `reqwest::Client` injectable), erreurs typées `CarbonFrError` `#[non_exhaustive]` (RFC 9457, `ProblemDetails`), **flux SSE** en `Stream` Rust nommé (`IntensityStream`, feature Cargo `stream`) — le point le plus délicat, commencé en premier comme prévu.
- [x] Le reste de `/v1` — intensité (`now`/`date`/`stats`), webhooks (créer/lister/supprimer), prix, coût, éligibilité, météo, échanges, renouvelable : **25 méthodes REST** (fait le 2026-09-25), chacune testée contre un serveur local avec des réponses réelles capturées sur la prod (23 lectures) ; test de parité avec le snapshot OpenAPI (26 opérations).
- [x] Tests hermétiques (serveur de test local `axum`, patron `adapter-webhook`, aucun appel réseau) : en place pour le socle et le flux SSE (`crates/sdk/src/{client,stream,region}.rs`, modules `tests`). Fixtures JSON/SSE des 26 opérations `/v1` déjà rapatriées (`crates/sdk/tests/fixtures/`), prêtes pour le test de parité (ADR-0031 décision 2) et les futurs tests des opérations REST restantes.
- [x] `examples/` exécutables (fait le 2026-09-25) : `intensity_now`, `mix_region`, `stream` (feature `stream`), `api_error` ; compilés en CI.
- [x] Publication `carbonfr-sdk` 0.1.0 (**faite le 2026-09-25**, manuelle, docs.rs vert ; tag `rust-sdk-v0.1.0`) — le README de la 0.1.0 a gardé une mention « first crates.io publication » (même piège que la 0.9.0 de `core`) → **0.1.1** corrective, à publier par `release-rust-sdk.yml` une fois le *trusted publisher* déclaré pour `carbonfr-sdk`. première publication **manuelle** (jeton ponctuel, comme en I5), puis *trusted publisher* déclaré pour cette crate (workflow `release-rust-sdk.yml`, en place et prêt, déclenché par le tag `rust-sdk-v*`) ; tag `rust-sdk-v0.1.0` (axe de versionnement propre, comme le SDK TS) ; README : section « SDK officiels » (à créer à ce moment-là — prématuré tant que la crate n'est pas publiée).

**Sortie** : `cargo add carbonfr-sdk` compile dans un projet vide ; parité TS atteinte sur intensité, webhooks et SSE ; docs.rs vert.

## I7 — `sqlx` 0.9 et décisions de méthodologie (~2–3 semaines → v0.10.0)

**Objectif** : solder la montée la plus risquée (toute la persistance), puis trancher les chantiers de fond laissés ouverts par les ADR.

- [x] **`sqlx` 0.8 → 0.9** (PR isolée ; fait le 2026-09-25 : `AssertSqlSafe` sur les 4 requêtes dynamiques, intégration PG17 23/23, démarrage réel vert) : d'après le CHANGELOG de `sqlx`, les requêtes construites dynamiquement doivent passer par `AssertSqlSafe` (≥ 4 appels `sqlx::query(&sql)` et 5 `QueryBuilder` dans `adapter-postgres`) ; suite d'intégration Postgres complète **en PG17** ; `cargo tree -d` : doublons `webpki-roots`/`hashbrown`/`rand` réduits.
- [x] **Partitionnement de `measurement`** (mesuré en prod le 2026-09-25 : 148 Mo, `/date` ≤ 8,5 ms, `/stats` 10 ans 45 ms → **non retenu**, seuils de déclenchement dans l'addendum ADR-0004) (ADR-0004 : « à reconsidérer maintenant que l'historique est ingéré ») : mesurer d'abord (taille, `EXPLAIN ANALYZE` des requêtes `/date` et `/stats` sur de larges intervalles), puis trancher dans un addendum ; si oui, migration testée sur une copie de prod avec fenêtre de maintenance.
- [x] **Facteurs ADEME** (addendum ADR-0008 : nom `acv-ademe@3` acté, déclencheurs et travaux listés, 3 points à confirmer) : l'ADR-0008 promettait la Base Empreinte V23.6 sous le nom `acv-ademe@2`, mais ce numéro a servi à la vue consommation (ADR-0010). Addendum ADR-0008 : calendrier d'un **`acv-ademe@3`** (nouvelle table de facteurs + revalidation des backtests). Effort estimé important : décider ici, implémenter plus tard.
- [x] **Cadence de revue LCOE** (addendum ADR-0024 : revue annuelle + à chaque nouvelle édition d'une source ; prochaine le **2026-12-15**) (ADR-0024 : aucune fréquence fixée ; millésimes 2021–2024) : addendum avec la date de la prochaine revue.
- [x] **Critère de déclenchement d'un `acv-ademe` régional** (addendum ADR-0010) (ADR-0010 : « dérivation sur dérivation » reportée) : addendum.
- [x] **Types du SDK TS générés depuis l'OpenAPI** (ADR-0032 : génération écartée pour la v1, test de parité TS recommandé ; doc des crates internes non publiée, déclencheur Phase B) : décider (outiller ou écarter explicitement, avec justification) ; même question pour publier la doc rustdoc des crates non publiées.

**Sortie** : `sqlx` 0.9 en prod ; 4 addenda mergés (ADR-0004, 0008, 0010, 0024) ; décision codegen actée. → atteinte avec la v0.9.3.

## I8 — Donnée régionale comblée, risques déjà en prod éteints, petites victoires API (~3 semaines → v0.10.0)

**Objectif** : combler en premier le trou de donnée régionale `acv-ademe` (principe 2, « la donnée publiée d'abord ») ; dans la même fenêtre de déploiement, borner deux ressources aujourd'hui non bornées en prod (connexions SSE, absence de timeout HTTP entrant) ; livrer les extensions d'API additives les moins coûteuses ; remettre à niveau la documentation de gouvernance et de crates.io.

- [ ] **Instrumenter le quota ODRÉ réel** (PROD-3) : les en-têtes `x-ratelimit-dataset-remaining`/`-limit` sont reçus à chaque appel (mesuré le 2026-09-26 depuis le VPS : régional 22 422/50 000 restants à J26, national 45 392/50 000) mais jamais lus. Les capter dans `carbonfr-adapter-odre` (0 appel supplémentaire), exposer en jauges Prometheus `carbonfr_odre_quota_remaining{dataset=…}`, règle d'alerte (< 10 % restant avant la fin du mois) dans `deploy/prometheus/alerts.yml` ; addendum ADR-0022. **Prérequis de l'item suivant** : pas question d'ajouter un appel/jour sur le jeu régional sans visibilité sur son quota. — *implémenté le 2026-09-26 (branche `feat/odre-quota-metrics`, PR à ouvrir)*
- [ ] **Combler l'historique régional `acv-ademe`** (PROD-1) : en prod, 26 989 lignes régionales pour 2026-02-03 → 2026-09-26 là où ~270 000 sont attendues (≈ 90 % manquants : jusqu'à la v0.9.5 le poller ne prenait que le dernier point régional par cycle) ; `/v1/intensity/date?region=bretagne&methodology=acv-ademe` sert ~24 points/jour. L'export de masse `eco2mix-regional-cons-def` existe (vérifié le 2026-09-26 : 2 839 104 enregistrements, pas 30 min, `eolien` typé chaîne comme `pompage`). Étendre le port `Eco2mixArchive` (`export_regional`), l'implémenter dans l'adapter ODRÉ, étendre `BackfillHistory` et la sous-commande `backfill` aux 12 régions ; tester sur une base jetable (une semaine) comme le national en I7, puis lancer les ~8 mois en prod avec dump préalable (`deploy/README.md` §5) ; documenter la résolution 30 min de l'archive régionale (vs 15 min en temps réel) ; CHANGELOG. **SemVer : minor** (nouvelle méthode sur un port public de `carbonfr-core` → rupture pour un implémenteur externe du trait, minor en 0.x selon l'ADR-0030). Aucun nouvel endpoint : pas de changement OpenAPI/Bruno/SDK.
- [ ] **Étendre l'auto-réparation quotidienne au régional** (PERF-3) : seconde tranche régionale dans `spawn_self_heal` (`bin/server/src/main.rs`), réutilisant `export_regional` (+1 appel ODRÉ/jour sur le jeu régional) — **à activer seulement une fois PROD-3 en prod et vérifié**. Addendum ADR-0003 (l'addendum du 2026-09-25 dit « pas d'export d'archive régional à ce jour »).
- [x] **`/v1/factors` : aligner le défaut de version** (QUAL-1) : `version` absent valait `2` alors que partout ailleurs l'absence vaut `1` (`/v1/intensity/now?methodology=acv-ademe` sert la v1) — la table auditée ne correspondait pas au calcul servi ; 0 test sur la route. `unwrap_or(1)`, doc du paramètre corrigée (propagée dans l'OpenAPI et le SDK Rust), 6 tests ajoutés, CHANGELOG « Corrigé ». **SemVer : patch** (adapter HTTP seul). *PR #131, mergée le 2026-09-26.*
- [ ] **Plafonner les connexions SSE concurrentes + timeout HTTP entrant** (SEC-1, PERF-2, SEC-4) : `/v1/intensity/stream` n'a ni plafond d'abonnés ni durée de vie (`broadcast::channel(64)` n'est qu'un tampon de retard) ; aucun `TimeoutLayer` ni timeout de lecture d'en-têtes (`axum::serve` par défaut). Sémaphore global sur la route (même motif que les livraisons webhook, `Semaphore::new(50)` dans `main.rs`), 503 au-delà d'un seuil généreux (quelques centaines) avec métrique de suivi ; `tower_http::timeout::TimeoutLayer` + timeout de lecture d'en-têtes hyper, défense en profondeur pour le self-hosting sans reverse proxy. Clôt la question ouverte « nombre max de connexions » de l'ADR-0014 (addendum). **SemVer : patch**. Docs : CHANGELOG, réponse 503 documentée dans l'OpenAPI.
- [ ] **Petites victoires API additives, une PR groupée** (PROD-API-2, PROD-API-4, PROD-API-5) :
  - PROD-API-2 : `?region=all` sur `/v1/intensity/now` (ou `/v1/intensity/now/all`) — 13 mesures en une requête SQL via `IntensityRepository` ; puis **remplacer les 12 `fetch` parallèles de `crates/adapter-http/assets/hydrogene/index.html`** par cet appel (le produit devient son propre premier client).
  - PROD-API-4 : `share` (0..1) et `label` additifs sur `MixBody` de `/v1/mix`, en réutilisant le calcul de parts déjà fait pour le contexte de `/v1/price` (`MixShareBody`).
  - PROD-API-5 : `GET /v1/regions` (slug, libellé, code INSEE — `Region::METROPOLITAN` du `core` a déjà tout) + `enum` utoipa sur le paramètre `region` des opérations concernées.
  **SemVer** : workspace **minor** (v0.10.0) si PROD-API-2 ajoute une méthode de port dans `core` (probable) ; côté SDK Rust, ajouter un champ public à un DTO (`MixBody` dans `crates/sdk/src/dto.rs`, struct entièrement publique) est un **minor de `carbonfr-sdk`** par décision de l'ADR-0030 §3 (champs publics assumés, jamais de `#[non_exhaustive]` posé après coup) → tag `rust-sdk-v0.2.0` ; SDK TS additif → `sdk-v0.3.0`. Docs : CHANGELOG, README (2 endpoints), snapshot OpenAPI, Bruno (3 requêtes), SDK TS (`types.ts`/`client.ts`), SDK Rust (`dto.rs`, `methods.rs`, `options.rs`).
- [x] **Champ `servers` de l'OpenAPI** (DOCS-3, DX-4) : promis en I2 (« PR séparée, après »), jamais livré (`/v1/openapi.json` sans `servers`). Instance hébergée + instance locale dans `carbonfr_openapi.rs`, snapshot régénéré, CHANGELOG. **SemVer : aucun**. *PR #132, mergée le 2026-09-26.*
- [ ] **Rattrapage GOUVERNANCE.md / CONTRIBUTING.md / SECURITY.md** (DOCS-1, DOCS-2) : GOUVERNANCE.md (dernière modification 2026-06-21) affirme encore « crates non publiées sur crates.io », « quatre axes de version » et « CI : 5 jobs » — l'addendum ADR-0019 (5ᵉ axe), l'ADR-0030 et les 8 jobs requis + 1 planifié du ruleset ne sont pas répercutés ; CONTRIBUTING.md dit « `carbonfr-sdk` pas encore publiée / pas couverte par `semver` » alors que le job couvre la crate depuis #123 ; SECURITY.md (2026-06-17) ne liste que le SDK TS dans son périmètre — y ajouter `carbonfr-sdk` (`crates/sdk/`, IO réseau réelle). **SemVer : aucun**.
- [ ] **Resynchroniser `deploy/prometheus/alerts.yml` sur le VPS** (PROD-4) : seul l'en-tête de commentaire diffère du dépôt depuis #128 (les 4 règles sont identiques) — à faire pendant le déploiement de la v0.10.0 (`cat` pour garder l'inode, `promtool check rules`, reload).

**Hors code (Morgan), en parallèle** : DOCS-4 (jeton crates.io et équipe `Kovelt/maintainers`, cf. « Actions hors code », **dans cet ordre** : équipe d'abord, révocation ensuite), DOCS-5 (licence ADEME), DOCS-6 (empreinte SSH).

**Sortie** : `SELECT count(*) FROM measurement WHERE region <> 'national' AND methodology_id = 'acv-ademe'` proche de l'attendu à 30 min sur la période comblée ; `spawn_self_heal` journalise 2 tranches (national + régional), activé après que `/metrics` expose `carbonfr_odre_quota_remaining` ; tests de `/v1/factors` verts, défaut = 1 ; au-delà du seuil, une connexion SSE supplémentaire reçoit 503 et une requête volontairement lente sur ses en-têtes est coupée ; `?region=all` renvoie 13 mesures en une requête et `/hydrogene` n'émet plus qu'un appel ; `/v1/mix` porte `share`/`label` ; `/v1/regions` liste 13 entrées ; le snapshot OpenAPI a `servers` et `region` en `enum` ; GOUVERNANCE/CONTRIBUTING/SECURITY ne contredisent plus l'état réel ; `alerts.yml` VPS = dépôt.
**Risques** : plus d'items que les itérations comparables, et plusieurs touchent `handlers.rs` (QUAL-1, SSE, les 3 items API) — une PR à la fois, rebaser souvent. Le backfill régional est ~12× plus volumineux par jour que le national : tranche courte d'abord, dump prod obligatoire avant le run complet. Le self-heal régional ajoute un appel/jour sur un quota déjà tendu : jamais avant PROD-3. Un plafond SSE trop bas coupe des clients légitimes : seuil généreux, métrique avant de resserrer. Révoquer le jeton crates.io sans second owner effectif laisserait le projet sans accès de publication en cas de perte de compte.

## I9 — Ce qui différencie carbon-fr : index qualitatif, export, compression, live fiable (~2–3 semaines → v0.11.0)

**Objectif** : mettre en avant les usages qui vendent carbon-fr — un index lisible pour le grand public et le no-code, un export en masse pour le reporting, un flux live qui tient la route — et lever la friction du premier contact (parcours local).

- [ ] **ADR-0033 puis implémentation : index qualitatif d'intensité** (PROD-API-1) : aucun équivalent de l'index *very low → very high* de carbonintensity.org.uk. ADR d'abord (principe 3) : seuils sourcés, fixes (façon NG ESO) ou percentiles sur l'historique national déjà en base — la page `/hydrogene` a déjà une classification interne (`CLASSES = [25, 45, 70, 100]`) comme point de départ. **Décision de Morgan requise avant le merge de l'ADR** (neutralité perçue des seuils). Puis calcul pur additif dans `core` (type `IntensityIndex` `#[non_exhaustive]`), champ additif sur `IntensityResponse`/`HistoryPoint`/`ForecastResponse`, SDK TS (`sdk-v0.4.0`) et Rust (`rust-sdk-v0.3.0`, minor obligatoire pour un champ public ajouté — ADR-0030 §3), Bruno, README, CHANGELOG. **SemVer : minor** (v0.11.0).
- [ ] **Export CSV de l'historique** (PROD-API-3) : `Accept: text/csv` ou `?format=csv` sur `/v1/intensity/date`, réponse écrite en flux (pas de matérialisation complète), plafond 366 j inchangé (addendum ADR-0004). **SemVer : patch**. Docs : CHANGELOG, README, snapshot OpenAPI (second content type), Bruno.
- [ ] **Compression HTTP gzip/br** (PERF-1) : mesuré en prod le 2026-09-26, `/v1/intensity/date` sur un an = 1 601 134 octets sans `content-encoding` (72 Ko en gzip, ÷22) ; ni Traefik ni l'API ne compressent. `tower_http::compression::CompressionLayer` (features `compression-gzip`/`compression-br`) posée sur le routeur, **en excluant `/v1/intensity/stream`** (SSE) pour ne pas bufferiser le flux live ; test explicite avec `Accept-Encoding` sur `/stream`. **SemVer : patch**. Docs : CHANGELOG.
- [ ] **Documenter le parcours local complet** (DX-1) : le README ne dit pas comment lancer l'API en local (Postgres via podman/docker, `DATABASE_URL` + `CARBONFR_VISIT_SALT` minimaux, `cargo run -p server`) — le chemin marche (vérifié le 2026-09-26), il manque au premier contact ; corriger le renvoi de `bruno/README.md` vers cette section aujourd'hui inexistante.
- [ ] **Reconnexion automatique du SDK TypeScript** (DX-2) : `stream()` de `sdk/typescript/src/client.ts` s'arrête silencieusement à la première coupure, alors que le SDK Rust reconnecte (backoff fixe puis exponentiel, désactivable, 3 tests dans `crates/sdk/src/stream.rs`). Porter le mécanisme et ses cas de test (coupure serveur, inactivité, désactivation) ; PR isolée (le plus gros morceau de l'itération). **SemVer SDK TS : minor** (`sdk-v0.3.0` ou suivant) ; README du SDK TS : paragraphe « Timeouts et reconnexion » symétrique du Rust.
- [ ] **Environnement Bruno « Production »** (DX-5) : `bruno/environments/Production.bru` (`baseUrl: https://carbon-fr-api.kovelt.fr`), mentionné dans `bruno/README.md`.
- [ ] 📅 **DST 2026-10-25** : vérifier après coup l'absence de doublon/trou dans `measurement` autour du changement d'heure (stockage UTC de bout en bout, risque a priori faible).
- [ ] 📅 **Veille RFNBO oct.–nov. 2026** : revérifier les signaux (issues de veille mensuelles) ; `rfnbo:2026-revision` reste `planned`, H3 seulement sur texte **adopté**.

**Sortie** : `/v1/intensity/now` porte un champ `index` cohérent, ADR-0033 mergé (*Accepté*) avant le premier commit qui l'implémente ; `curl -H 'Accept: text/csv' …/v1/intensity/date?…` renvoie du CSV valide sur une plage large sans pic mémoire ; `curl -D- -H 'Accept-Encoding: gzip' …/v1/intensity/date` → `content-encoding: gzip`, et `/stream` reste non compressé ; README « Lancer l'API en local » suivi de bout en bout ; un test TS simulant une coupure confirme la reconnexion ; `Production.bru` présent ; aucune anomalie sur `measurement` le 26 octobre ; veille RFNBO consignée sans rien activer.
**Risques** : l'ADR-0033 peut conclure à ne pas servir d'index (seuils trop disputés) — l'item se limite alors à l'ADR. `CompressionLayer` sans exclusion de `/stream` casserait le SSE. Le port de la reconnexion TS ne se copie pas depuis le Rust (`fetch`/`ReadableStream` ≠ `reqwest`) : le patron sert de référence.

## I10 — Filet SDK TS, documentation OpenAPI, rattrapage documentaire (~2–3 semaines, + échéances de décembre → v0.12.0)

**Objectif** : finir ce que le cycle a ouvert côté SDK et dev-first, améliorer la découvrabilité de l'OpenAPI sur les endpoints les plus consultés, rattraper la documentation qui décrit encore un projet arrêté à I7, et honorer les échéances de fin d'année.

- [ ] **Test de parité SDK TS ↔ OpenAPI** (DX-3) : table `operationId → méthode SDK` vérifiée contre `crates/adapter-http/tests/openapi.snapshot.json`, en `node:test` (zéro dépendance), dans le job CI `sdk-typescript` — recommandé par l'ADR-0032, une dérive réelle a déjà été trouvée une fois (sortie 0.2.0). Tolérer les méthodes utilitaires sans `operationId`. **SemVer : aucun**.
- [ ] **Exemples et en-têtes sur l'OpenAPI** (PROD-API-6) : 0 exemple de réponse sur 28 opérations ; `example` sur les réponses 200 des 4 endpoints les plus consultés (`/intensity/now`, `/mix`, `/forecast`, `/greenest-window`, en intégrant l'`index` d'I9), en-têtes `RateLimit-*`/`Cache-Control` déjà envoyés mais absents du spec. **SemVer : aucun**. Docs : snapshot OpenAPI, CHANGELOG.
- [ ] **PR groupée « rattrapage doc »** (DOCS-7, DX-7) : README §Feuille de route et `docs/ARCHITECTURE.md` §8 s'arrêtent avant crates.io/SDK Rust/fiabilité — ajouter la phase « Fiabilité & écosystème Rust » en y intégrant ce qu'I8/I9 ont livré ; CONTRIBUTING.md : paragraphe « Tests d'intégration Postgres » (`DATABASE_URL=postgres://localhost/carbonfr_test cargo test -p carbonfr-adapter-postgres --test pg`), symétrique du paragraphe ODRÉ live. Revérifier chaque chiffre cité en fin d'itération.
- [ ] 📅 **Revue LCOE annuelle — 2026-12-15** (addendum ADR-0024) : revue des sources de `/v1/cost-reference` ; nouveau millésime porté par la donnée si une source a changé, sinon addendum de reconduction daté.
- [ ] 📅 **Backfill consolidé juillet–septembre 2026 — fin décembre** : dès que RTE publie le jeu consolidé (~3 mois de retard), relancer `backfill` en source consolidée sur juil.–sept. (remplace les valeurs temps réel ingérées le 2026-09-25), dump préalable, `deploy/README.md` §5 — même patron que février–juin.
- [ ] 📅 **TRV / accise au 1er février 2027** : en janvier, vérifier la délibération CRE ; si un nouveau barème est publié, addendum ADR-0023 + millésime TRV `2027-H1` **avant** le 1er février (l'incident du millésime 2026-H2 manqué a coûté I1).

**Sortie** : le job `sdk-typescript` échoue si un `operationId` n'a pas de méthode SDK (vérifié en cassant un cas en local) ; Swagger UI affiche des valeurs pré-remplies sur les 4 endpoints, en-têtes documentés ; README/ARCHITECTURE/CONTRIBUTING ne contredisent plus l'état réel ; addendum LCOE daté mergé ; juil.–sept. 2026 en millésime consolidé ; `/v1/price` prêt pour le 1er février 2027.
**Risques** : glissement d'I8 (la plus chargée) sur les échéances de décembre — les échéances datées passent devant le reste de l'itération quand elles tombent (principe rappelé en tête de plan).

## Dette actée pour un futur cycle (hors I8–I10)

Constats confirmés par l'état des lieux du 2026-09-26, laissés volontairement hors des trois itérations — à reprendre dans un cycle dédié (I11+), pas au fil de l'eau sans décision.

| Constat | Pourquoi hors de ce plan |
|---|---|
| DEPS-1 (`Dockerfile` : `bookworm` en LTS communautaire depuis le 2026-06-11 → `trixie`) + SEC-6 (aucun scan de vulnérabilités ni SBOM sur l'image GHCR, base non épinglée par digest) | Réels et datés, mais sans effet sur les fonctionnalités de ce cycle. **À traiter ensemble**, en PR isolée (principe 5) : basculer l'image puis la scanner dans la foulée. |
| SEC-2 (secret de signature webhook stocké en clair en base), SEC-3 (`is_public_ip` ne couvre pas la forme IPv6 `::a.b.c.d`), SEC-5 (pas de CSP sur `/docs`) | Durcissements secondaires : SEC-2 suppose une fuite de base, SEC-3 est quasi inexploitable sur un Linux moderne, SEC-5 attend une CVE XSS sur `swagger-ui-dist`. SEC-3 + SEC-5 groupables en une petite PR. |
| QUAL-2 (`CalibrateRenewable::execute` jamais testé), QUAL-3 (validation `from`/`to` dupliquée dans 5 handlers), QUAL-4 (boucles de fond `spawn_webhook_purge`/`spawn_self_heal` non testées) | Dette de tests/refactor sur du code qu'aucun item d'I8–I10 ne modifie ; patrons de test déjà en place (`spawn_webhook_*`). |
| DEPS-2 (`train`/`adapter-gbdt` embarqués sans feature-gate dans le binaire de prod), DEPS-3 (`sha2` 0.10 → 0.11) | Dette de dépendances mineure, sans risque immédiat. |
| PROD-2 (`weather_forecast` porte un historique 2016–2026 de 87 479 lignes non documenté, lié au backtest `gbdt@1`) | Aucun risque actif ; à trancher à la prochaine revue de l'ADR-0012. |
| Mix historique (`/v1/mix` sans `from`/`to`, `HistoryPoint` sans `mix` alors que les 10 colonnes sont stockées depuis 2012) | Candidat I11+ : `mix: Option<…>` additif sur `HistoryPoint` ou `/v1/mix/date`, effort S/M, valeur moyenne, aucun déclencheur externe. |
| PROD-API-7 (région ↔ code postal), PROD-API-8 (historique public prévision vs réalisé) | Valeur faible (PROD-API-7) ; effort XL avec nouveau port de persistance et ADR dédié (PROD-API-8). |
| DX-6 (exemples OpenAPI sur les 289 propriétés), DX-8 (granularité des `code` RFC 9457) | DX-6 couvert en esprit par PROD-API-6 (I10, ciblé) ; DX-8 est un choix assumé par l'ADR-0021. |
| PERF-4 (retry/backoff du poller sur panne ODRÉ soutenue), PERF-5 (`EXPLAIN` des tables denses hors `measurement`) | Quota jamais épuisé en pratique ; tables encore petites (~3 mois d'ENTSO-E). |

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
| **2026-10-25** | Changement d'heure (fin de l'heure d'été) | Vérifier après coup l'absence de doublon/trou dans `measurement` → **I9** |
| **Oct.–nov. 2026** | Révision RFNBO annoncée « à l'automne » | Revérifier les signaux (veille #90) ; H3 seulement si le texte est **adopté** |
| **Fin 2026** | Proposition RED IV annoncée | Veille (quota RFNBO 42 %, ouverture au bas-carbone) |
| **Fin décembre 2026** | Publication par RTE du jeu consolidé de juillet–septembre 2026 (~3 mois de retard) | Backfill en source consolidée sur juil.–sept. (remplace le temps réel ingéré le 2026-09-25), dump préalable, `deploy/README.md` §5 → **I10** |
| **1er février 2027** (habituellement annuel) | Nouveau barème TRVE / accise (délibération CRE de janvier) | Nouveau millésime TRV, addendum ADR-0023 → **I10** (vérification en janvier) |
| **1er août** (habituellement annuel) | Revalorisation du TURPE | Millésime TRV de mi-année |
| **2026-12-15**, puis annuellement | Revue des sources LCOE de `/v1/cost-reference` (addendum ADR-0024) | Nouveau millésime porté par la donnée si une source a changé ; GATE de neutralité si la présentation change |
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
| Partitionnement de `measurement` | L'un des seuils de l'addendum ADR-0004 (taille, volume, latence p95, rythme de croissance) franchi |
| `acv-ademe@3` (facteurs Base Empreinte à jour) | Déclencheurs de l'addendum ADR-0008 (nouvelle édition exploitable, licence confirmée) |
| `acv-ademe@2` régional (vue consommation) | Critère de l'addendum ADR-0010 (flux inter-régionaux publiés et exploitables) |
| Test de parité du SDK TS contre l'OpenAPI | Recommandé par l'ADR-0032, à faire dans une PR de code |
| Délivrance de clés en libre-service (e-mail, lien magique, rotation, `/v1/keys`) | Besoin réel de clés hors opérateur |
| Site statique o2switch | Décision produit de Morgan (ou clore l'item de l'ADR-0007) |
| Servir `gbdt@1` / `share-meteo@2` | Nouveau backtest qui franchit la GATE (ADR-0012 / ADR-0028) |
| Phase B de gouvernance (revue Code Owners obligatoire) | Première contribution externe (ADR-0027) |

## Actions hors code (Morgan)

- [ ] Comparer l'empreinte SSH du VPS vue depuis le fixe avec celle enregistrée sur le portable (valeurs dans la mémoire locale d'exploitation, jamais dans ce dépôt public).
- [ ] **Révoquer le jeton crates.io ponctuel du 2026-09-24** (crates.io → *Account settings* → *API Tokens*) et ajouter l'équipe comme owner des 3 crates (`cargo owner --add github:Kovelt:maintainers carbonfr-core`, idem `carbonfr-eligibility`, `carbonfr-sdk`) — **dans cet ordre : vérifier/créer l'équipe GitHub `Kovelt/maintainers` et l'ajouter d'abord, révoquer ensuite** (un essai antérieur de `cargo owner --add` a échoué ; sans second owner effectif, la révocation laisserait le projet sans accès de publication en cas de perte de compte). La case « Avant I5 » ci-dessus reste cochée pour la publication, mais ce reliquat (DOCS-4) est ouvert.
- [x] Journaux Dependabot : pourquoi `reqwest` 0.13 n'a jamais été proposé — feature `rustls-tls` supprimée en 0.13, montée non résoluble automatiquement (cf. I2).
- [x] Activer « Automatically delete head branches » (Settings → General → Pull Requests) — actif depuis le 2026-09-25 (branches des PR #124 à #128 supprimées automatiquement au merge).
- [ ] Envoyer la demande de licence à cdo@ademe.fr (débloque H6 v2).
- [x] Avant I5 : créer ou vérifier le compte crates.io (fait, e-mail vérifié ; jeton ponctuel utilisé le 2026-09-24 puis retiré de `.env` — **à révoquer** sur crates.io) ; générer un jeton ponctuel pour la première publication (le révoquer juste après), puis déclarer `Kovelt/carbon-fr` comme *trusted publisher* sur `carbonfr-core` et `carbonfr-eligibility`.
- [x] En I6 : même chose pour `carbonfr-sdk` (déclaration distincte, possible seulement après sa première publication). — fait le 2026-09-25 (`release-rust-sdk.yml`, 0.1.1 publiée sans jeton).
- [x] I1 : fournir le token ENTSO-E pour le rejeu live, ou le lancer soi-même — rejoué le 2026-09-23 avec le token de prod, sans l'afficher.
- [x] I1 : dans Uptime Kuma (`status.<domaine>`), créer un canal de notification (e-mail SMTP, déjà configuré pour les sauvegardes) et l'attacher aux sondes « API /health » et « fraîcheur données » ; optionnellement une 3ᵉ sonde sur les alertes Prometheus actives. — **fait le 2026-09-25** (cf. I1).
