# ADR-0031 — Conception du SDK Rust (`carbonfr-sdk`)

- **Statut** : Accepté
- **Date** : 2026-09-23
- **Décideurs** : Morgan (Kovelt / carbon-fr)
- **ADR liés** : ADR-0002 (hexagonal — `core` sans `serde`), ADR-0014 (SSE, `/v1/intensity/stream`), ADR-0015 (clés API `Bearer`),
  ADR-0016 (webhooks), ADR-0019 (versionnement, axe SDK découplé), ADR-0021 (erreurs RFC 9457), ADR-0030 (politique de publication
  crates.io, rédigé en parallèle — périmètre `core`+`eligibility` puis `carbonfr-sdk`, versions de `core`/`eligibility` couplées au
  workspace, SDK sur axe propre)

## Contexte

Le SDK TypeScript `@carbon-fr/sdk` 0.2.0 (publié le 2026-09-23, npm, Trusted Publishing) couvre 26 des 28 opérations de l'OpenAPI
3.1.0 servi (`health`/`health_ready` exclues, infra) — `sdk/typescript/src/client.ts:79-493`. `docs/plan-iterations.md` (I3, I6)
prévoit un équivalent Rust, `carbonfr-sdk`, publié sur crates.io après `carbonfr-core`/`carbonfr-eligibility` (I5). Aucune ligne de
code SDK Rust n'existe : ni crate, ni emplacement dans `[workspace] members` (`Cargo.toml` racine, 11 membres, aucun `sdk`).

Deux contraintes structurent la conception : (1) le workspace amène `reqwest` 0.13 en I5 (feature `rustls-tls` renommée `rustls`,
provider crypto `aws-lc-rs` par défaut, `query` devenue opt-in, `rust_version = "1.85.0"` — vérifié `GET
/api/v1/crates/reqwest/0.13.5`, 2026-09-23) ; (2) `deny.toml` interdit toute crate marquée « non maintenue » par RustSec
(`[advisories] unmaintained = "all"`, ligne 18) et n'autorise aucune licence copyleft (`[licenses] allow`, lignes 28-41, sans
MPL-2.0).

Point dur identifié : `GET /v1/intensity/stream` (`crates/adapter-http/src/handlers.rs:1232-1287`) n'émet qu'un type d'événement
`intensity` (`Event::default().event("intensity").data(json)`, sans `.id(...)` ni `.retry(...)`), sur un `tokio::broadcast::Sender`
éphémère (`crates/adapter-http/src/lib.rs:350-361`, `StreamState`) — aucune reprise `Last-Event-ID` n'est possible côté serveur
aujourd'hui. Le SDK TS ne se reconnecte d'ailleurs pas automatiquement (`client.ts:409-444`, boucle simple sur
`fetch`/`ReadableStream`).

## Décision

1. **Nom et emplacement** : crate `carbonfr-sdk`, chemin **`crates/sdk`** (nouveau membre du workspace, comme les 10 crates
   existantes — pas `sdk/rust`, qui casserait la convention « tout le Rust sous `crates/` » sans bénéfice fonctionnel). Nom vérifié
   libre le 2026-09-23 (`GET /api/v1/crates/carbonfr-sdk` → 404 `does not exist`, revérifié ce jour).

2. **Écriture manuelle**, pas de génération depuis l'OpenAPI. Le seul générateur Rust mature (progenitor 0.15.0, 5,2 M+
   téléchargements, publié le 2026-09-10) documente lui-même une cible **OpenAPI 3.0.x** ; notre document est en **3.1.0**
   (`openapi.snapshot.json` → `"openapi": "3.1.0"`) — risque de perte de fidélité sur des idiomes JSON Schema 2020-12. Le générateur
   Java officiel (OpenAPITools) ne documente pas non plus 3.1 pour sa cible `rust` et ajoute une dépendance JRE hors de l'esprit «
   binaire souverain léger » (ADR-0001). Les alternatives « 3.1-natives » sont trop jeunes/peu adoptées (créées 2024-10→2026-07, <45
   K téléchargements) pour un projet qui protège ses invariants par ADR. **Garde-fou compensatoire** : un test de parité, hermétique,
   vérifie que chaque `operationId` du snapshot OpenAPI retenu en v1 (décision 8) a une méthode SDK correspondante, sur le patron
   déjà utilisé par `adapter-webhook` (serveur de test `axum` en dev-dependency, `crates/adapter-webhook/Cargo.toml`).

3. **Client HTTP** : `reqwest` 0.13 (déjà la dépendance HTTP standard des 4 adapters existants), `reqwest::Client` construit par
   défaut mais **injectable** (builder). **Provider TLS : `ring`, passé explicitement**, comme le serveur en I5 — `reqwest` en
   `rustls-no-provider` et configuration rustls construite avec le provider `ring` fourni à la configuration du client, **sans**
   dépendre du provider par défaut du process. Trois raisons : (a) `crates/sdk` est un membre du **même workspace** que
   `bin/server` : Cargo unifie les features d'une dépendance partagée lors des builds `--workspace` (jobs lint et tests de la CI) —
   laisser la feature `rustls` par défaut (⇒ `aws-lc-rs`) sur `reqwest` réintroduirait dans ces builds le double provider
   `ring`/`aws-lc-rs` qu'I5 élimine ; (b) un provider passé **explicitement** à la configuration n'entre pas en conflit avec celui
   qu'une application cliente aurait installé par défaut dans son process — le risque de conflit ne vient que de la dépendance au
   provider par défaut quand deux sont compilés ; (c) `ring` évite aux utilisateurs le build C d'`aws-lc-sys`. Une feature
   `rustls-aws-lc-rs` opt-in reste ajoutable sans rupture si un utilisateur la demande. **Vérification exigée en I6** :
   `cargo tree -e features` sur `server` ne doit faire apparaître ni `aws-lc-rs` ni `aws-lc-sys`.

4. **Async uniquement en 0.1** (tokio). Le flux SSE est intrinsèquement asynchrone ; le SDK TS lui-même n'a qu'un mode
   (fetch/promises). `reqwest` 0.13 expose une feature `blocking` (vérifiée : `dep:futures-channel`, `dep:futures-util`,
   `tokio/sync`) qui reste **ajoutable sans rupture** plus tard si un besoin réel émerge — un retrait ultérieur serait, lui, coûteux
   (ADR-0020).

5. **Aucune dépendance à `carbonfr-core`** : les DTOs de requête/réponse sont des types propres au SDK (comme le fait déjà le SDK
   TS), et les petits enums de domaine (`Region`, `Methodology`, `Estimator`, `EligibilityFramework`, `Interval`…) y sont
   **dupliqués** avec leur (dé)sérialisation. Ce n'est pas une hypothèse : ADR-0030 §6 exclut toute feature `serde` sur `core`
   avant 1.0 (`core` n'a aucune dépendance `serde`, `crates/core/Cargo.toml` : `async-trait`, `thiserror`, `time`, `sha2`, `url`),
   et le SDK suit le **contrat HTTP** `/v1`, pas le modèle interne — une évolution du domaine qui ne change pas le contrat ne doit
   pas imposer une version du SDK. La cohérence des valeurs est garantie par le test de parité (décision 2). **Horodatages** :
   `time::OffsetDateTime` (RFC 3339, feature `serde` de `time`), comme le reste du projet.

6. **Erreurs typées** : `enum CarbonFrError` via `thiserror` (convention lib du projet, `CONTRIBUTING.md:32`),
   **`#[non_exhaustive]`** sur l'enum et sur le DTO interne `ProblemDetails`, au minimum 3 familles de variantes — transport
   (`reqwest::Error`), API (`status` + `code` RFC 9457 + `ProblemDetails` complet, ADR-0021), désérialisation. `code` est l'ancrage
   machine à matcher en priorité (ADR-0021, catalogue extensible). ⚠️ **Premier usage de `#[non_exhaustive]` dans tout le dépôt** (0
   occurrence aujourd'hui, `grep -rn non_exhaustive crates/ bin/` → vide) ; devance le travail prévu en I4 pour `core`/`eligibility`,
   ce n'est pas une contradiction avec l'audit actuel.

7. **SSE** : parseur `eventsource-stream` 0.2.3 (dépendances **normales** `futures-core ^0.3`, `nom ^7.1`, `pin-project-lite ^0.2.8`
   — `reqwest` n'apparaît qu'en **dev**-dépendance, `^0.11`, vérifié `GET /api/v1/crates/eventsource-stream/0.2.3/dependencies` ce
   jour — donc **aucun couplage** à une version majeure de `reqwest`), posé directement sur `reqwest::Response::bytes_stream()`.
   Publiée pour la dernière fois le 2022-02-17 (~4,5 ans) mais **aucun avis RustSec trouvé**
   (`rustsec.org/packages/eventsource-stream.html` → 404, `rustsec.org/advisories/` sans occurrence, vérifié ce jour) — risque
   `unmaintained = "all"` non nul mais non matérialisé aujourd'hui. Par-dessus : une boucle de reconnexion **maison, volontairement
   mince** — ré-essai après délai (fixe puis exponentiel borné, configurable), **sans reprise `Last-Event-ID`** : le serveur n'émet
   aucun `id` et sa source (`tokio::broadcast` éphémère, ADR-0014) ne permet de toute façon aucun rejeu — implémenter la reprise
   WHATWG complète n'apporterait aucun bénéfice mesurable tant que le serveur ne la supporte pas. `reqwest-eventsource`
   (reconnexion+`Last-Event-ID` déjà complets) est **écarté** : dépendance normale `reqwest ^0.12.0` figée, contredirait le nettoyage
   du doublon `tower-http`/`reqwest` visé par I5, sans release depuis 2024-03-29. Le tout derrière une **feature Cargo `stream`,
   opt-in** (off par défaut) pour ne pas imposer `tokio-stream`/`futures-util` aux usages REST-only ; elle active aussi `reqwest/stream`, qui
   garde `Response::bytes_stream()` (« Available on crate feature `stream` only », docs.rs `reqwest` 0.13.5).

8. **Périmètre v1 = parité stricte avec le SDK TS 0.2.0**, soit les mêmes 26 opérations (sur 28 — `health`/`health_ready` hors
   périmètre, infra) : intensité (`now`/`date`/`stats`/`below`), mix, prévision (`forecast`, `greenest-window`,
   `schedule`/`schedule/slots`), échanges (+historique), météo (+historique), renouvelable, méthodologies/facteurs, prix
   (+historique), `cost-reference`, éligibilité (`rulesets`), stats de visite (lecture + enregistrement), webhooks
   (créer/lister/supprimer), flux SSE. Toute opération non couverte à la publication 0.1.0 est listée **explicitement « à venir »**
   dans la doc rustdoc du crate, jamais silencieusement absente.

9. **Features Cargo** : par défaut minimal (`reqwest` + `json`, TLS `ring`, décision 3) ; `stream` = `eventsource-stream` +
   `reqwest/stream` + utilitaires de flux (SSE, décision 7) ; pas de `blocking` en 0.1 (décision 4).

10. **MSRV : 1.88.0, alignée sur ADR-0030 §5.** `reqwest` 0.13.5 seul demanderait 1.85.0 (édition 2024), mais les horodatages en
    `time::OffsetDateTime` (décision 5) tirent le plancher de `time` : **1.88.0** (`time` 0.3.49 comme 0.3.55, vérifié sur
    crates.io). Une seule MSRV pour les trois crates publiées, donc un seul job CI épinglé (celui qu'I4 pose), relevée seulement en
    version *minor*. Le SDK ne dépendra jamais de `sqlx` : la MSRV plus élevée qu'imposera `sqlx` 0.9 côté serveur (I7) ne le
    concerne pas.

11. **Tests hermétiques et exemples** : serveur de test HTTP local (`axum` en dev-dependency, patron déjà établi par
    `adapter-webhook`, 8 tests) — jamais de mock HTTP générique sans précédent dans le dépôt. Test de parité (décision 2). Dossier
    `examples/` exécutables (`cargo run --example`) : requête simple (`intensity_now`), requête paramétrée (`mix` avec région), flux
    SSE (derrière `stream`), gestion d'une erreur typée (`CarbonFrError::Api`) — plus découvrable et testable en CI qu'un README
    seul.

12. **Versionnement et publication** : axe SemVer **propre** au crate (**pas** `version.workspace = true`), conforme à ADR-0019 (« le
    SDK suit le contrat d'API, pas la version du serveur ») — à la différence de `carbonfr-core`/`carbonfr-eligibility` dont la
    version reste couplée au workspace (ADR-0030, dépendance interne versionnée nécessaire à `cargo package`). Tag de release
    **`rust-sdk-v*`**, symétrique de `sdk-v*` (SDK TS). Publication : première version **manuelle** (jeton crates.io ponctuel,
    révoqué aussitôt — le Trusted Publishing ne se configure que sur une crate déjà publiée), puis **Trusted Publishing** (OIDC
    GitHub, `id-token: write`, même modèle que `release-sdk.yml`/`release-crates.yml`), déclaration distincte pour `carbonfr-sdk`
    (I6, après sa première publication). **`cargo-semver-checks` bloquant** sur `carbonfr-sdk`, comme ADR-0030 §4 pour
    `core`/`eligibility` : `--baseline-rev` avant la première publication, mode registre ensuite, avant chaque tag `rust-sdk-v*`. **Dépend explicitement de I5** : l'implémentation ne démarre pas avant que `reqwest` 0.13
    soit en place côté serveur (décision 3), sans quoi le SDK serait à retravailler immédiatement après.

## Conséquences

- **Positives** : surface v1 bornée et vérifiable mécaniquement (test de parité) sans dépendre d'un générateur immature pour notre
  OpenAPI 3.1 ; aucune dette de version `reqwest` introduite par le choix SSE ; erreurs typées exploitables (`#[non_exhaustive]`, RFC
  9457) sans promettre une garantie de reconnexion que le serveur ne peut pas tenir ; axe de version découplé (ADR-0019) et MSRV
  commune aux crates publiées (ADR-0030), jamais tirée vers le haut par `sqlx` 0.9 ; **un seul provider crypto (`ring`) dans tout
  le workspace**, y compris si le SDK devient un jour une dépendance de test du serveur.
- **Négatives / limites assumées** : écriture manuelle = plus de code à maintenir en parallèle du SDK TS (mitigé par le test de
  parité) ; `eventsource-stream` est une dépendance ancienne (2022) sous surveillance `unmaintained = "all"` — à réévaluer si RustSec
  l'y classe un jour (repli documenté : `sse-stream`, plus récent, voir alternatives) ; reconnexion SSE volontairement limitée (pas
  de `Last-Event-ID`) tant que le serveur n'évolue pas ; duplication assumée des petits enums de domaine (ADR-0030 §6), tenue en
  cohérence par le test de parité.
- **Engage** : ne pas démarrer l'implémentation avant la fin d'I5 (`reqwest` 0.13 en prod) ; tenir à jour le test de parité à chaque
  évolution de `/v1` ; ne jamais promettre dans la doc une garantie de non-perte d'événements SSE non tenue par le serveur.

## Alternatives envisagées

- **Génération automatique** (progenitor, OpenAPITools, générateurs « 3.1-natives ») — écartée : mismatch de version OpenAPI
  documentée ou immaturité, cf. décision 2.
- **`ureq`** — écarté : aucune feature async/tokio dans son manifeste (`GET /api/v1/crates/ureq/3.4.2`), inadapté au flux SSE et à
  l'architecture 100 % tokio (ADR-0001).
- **`hyper-util` direct** — écarté : trop bas niveau, réimplémenterait JSON/query/TLS déjà fournis par `reqwest`, déjà présent dans
  l'arbre via les adapters existants.
- **Laisser le provider par défaut de `reqwest`** (`aws-lc-rs`) — écarté : double provider dans les builds `--workspace` par
  unification des features, build C d'`aws-lc-sys` imposé aux utilisateurs ; un provider passé explicitement ne gêne pas celui
  d'une application cliente, cf. décision 3.
- **`reqwest-eventsource`** (reconnexion + `Last-Event-ID` complets) — écarté : dépendance normale `reqwest ^0.12.0` figée, contredit
  I5, sans release depuis 2024-03-29, bénéfice `Last-Event-ID` inutile côté serveur actuel.
- **`eventsource-client` (LaunchDarkly)** — écarté malgré une maintenance active (2026-08-10) : transport HTTP propriétaire
  (`launchdarkly-sdk-transport`), pas `reqwest` — introduirait une seconde pile HTTP complète.
- **Parsing SSE 100 % maison** (sans aucune crate) — écarté par défaut : la norme WHATWG (CRLF/BOM/coupures UTF-8, réinitialisation
  d'`id` vide…) représente un ordre de grandeur de 250-350 lignes et 20-30 cas de test pour dupliquer un travail déjà couvert par une
  fondation à 23,4 M+ téléchargements ; reste le repli si `eventsource-stream` devait un jour être classée `unmaintained`.
- **Coupler la version du SDK au workspace** — écartée : contredirait ADR-0019 (axe SDK découplé, déjà décidé pour le SDK TS).

---

> **Points à confirmer par Morgan** (choix sans réponse objectivement meilleure, ou dépendant d'une décision produit distincte)
>
> 1. **Contrat exposé par le flux** : type nommé concret (ex. `pub struct IntensityStream`, plus explicite pour docs.rs et plus
>    stable comme engagement SemVer) ou `impl Stream<Item = Result<Evenement, Erreur>>` opaque (plus simple, évolution interne libre)
>    ?
> 2. **Timeout par défaut** : le SDK TS n'en a aucun (`CarbonFrOptions`, `client.ts:38-45`) — ajouter un timeout Rust par défaut
>    (hors parité stricte) ou reproduire l'absence à l'identique ?
> 3. **Mode bloquant** : rester strictement async-only en 0.1 (décision 4) ou l'ouvrir dès maintenant pour des consommateurs
>    hors-tokio (scripts, CLI) ?
> 4. **`eventsource-stream` vs `sse-stream`** (0.3.0, publiée le 2026-09-18, activement maintenue mais popularité tirée surtout par
>    l'écosystème MCP/`rmcp`) : valider `eventsource-stream` comme choix par défaut malgré son âge (décision 7), avec `sse-stream`
>    documentée comme repli ?

