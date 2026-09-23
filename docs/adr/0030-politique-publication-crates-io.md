# ADR-0030 — Politique de publication crates.io

- **Statut** : Accepté (la fusion de la PR vaut acceptation par Morgan)
- **Date** : 2026-09-23
- **Décideurs** : Morgan (Kovelt / carbon-fr)
- **ADR liés** : ADR-0001 (le `core` « conçu pour être publiable » — ce que cet ADR concrétise), ADR-0002 (architecture hexagonale — pourquoi `core`/`eligibility` sont les seules candidates), ADR-0019 (amendé par un addendum daté 2026-09-23 — 5ᵉ axe de versionnement), ADR-0027 (gouvernance solo — décideur unique, pas d'approbation externe requise), ADR-0031 (SDK Rust `carbonfr-sdk`, rédigé le même jour — périmètre et axe de version distincts, cf. §1 et §8)

## Contexte

`carbon-fr` porte une contradiction non résolue depuis son origine. ADR-0001 (2026-06-14) décide : « Le `core` est conçu pour être publiable sur crates.io comme bibliothèque indépendante. » ADR-0019 (2026-06-17), en Contexte, affirme l'inverse : « les crates `carbonfr-*` ne sont pas publiées séparément sur crates.io — le service se distribue en image Docker […]. Les versionner individuellement n'a donc aucun intérêt. » (`docs/adr/0019-politique-de-versionnement.md:16`), et l'inscrit en Conséquences : « Ne nous engage pas à du SemVer par crate ni à de la publication crates.io […] les crates restent `publish = false`. » (`:47`). Cet ADR tranche en faveur d'ADR-0001 et amende ADR-0019 (addendum séparé, même date, symétrique de la mention déjà présente en tête d'ADR-0019 quand un ADR ultérieur en amende un antérieur).

Itération I3 du plan (`docs/plan-iterations.md`, §I3) : décision seule, aucune ligne de code hors `docs/`. La préparation technique (I4) et la première publication (I5) en découlent mais ne sont pas exécutées ici.

**État vérifié le 2026-09-23** (commandes rejouées dans le dépôt ; `crates/core` et `crates/eligibility` en `publish = false`, seuls membres sans IO du workspace ; version de workspace `0.8.2`, édition 2024, `repository = "https://github.com/Kovelt/carbon-fr"`, `Cargo.toml:27-30`) :

| Point | État | Preuve |
|---|---|---|
| Noms `carbonfr-core`, `carbonfr-eligibility`, `carbonfr-sdk` | libres | `GET crates.io/api/v1/crates/<nom>` → HTTP 404 `does not exist`, les trois, revérifié ce jour |
| `cargo package -p carbonfr-core` | passe | 48 fichiers, 333,4 KiB / 87,8 KiB compressés (aucune dépendance interne) |
| `cargo package -p carbonfr-eligibility` | échoue | « all dependencies must have a version requirement specified when packaging. dependency `carbonfr-core` does not specify a version » — `carbonfr-core = { path = "crates/core" }` sans `version`, `Cargo.toml:34` |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p carbonfr-core -p carbonfr-eligibility` | échoue | 2 liens vers des items privés (`forecast.rs:84` → `BAND_QUANTILE`, `price.rs:307` → `Filiere::merit_order`) + 8 liens `[FAIT]`/`[ESTIMATION]` pris pour des liens intra-doc dans `crates/eligibility/src/ruleset.rs` (lignes 13, 16, 19, 23, 28, 127, 132, 135) |
| `#[non_exhaustive]` | 0 sur 25 enums publics | 18 dans `core`, 7 dans `eligibility` (`grep -rn "^pub enum"`) |
| Structs publiques | 81 (70 `core` + 11 `eligibility`), 54 à champs publics | `grep -rn "^pub struct"`, recoupé |
| MSRV | non déclarée | `cargo metadata` : `rust_version: None` sur les 11 membres ; CI (`.github/workflows/ci.yml:36/88/101`) sur `dtolnay/rust-toolchain@…#stable` flottant |

## Décision

### 1. Périmètre publié

**`carbonfr-core` et `carbonfr-eligibility`, et rien d'autre pour l'instant.** Ce sont les deux seules crates du workspace sans IO (règle d'or, ADR-0002) : `core` ne dépend d'aucune autre crate interne, `eligibility` ne dépend que de `core` (`crates/eligibility/Cargo.toml:11`). **Jamais** `bin/server` ni les 8 adapters (couplés à axum/sqlx/reqwest/l'infra — publier un adapter romprait l'esprit « bibliothèque réutilisable » ; `adapter-gbdt` embarque en outre un modèle non servi, ADR-0012). `carbonfr-sdk` est **hors périmètre** de cet ADR : nom, client HTTP et surface sont l'objet d'ADR-0031 ; sa publication (I6) suit la même procédure (§8) mais garde un axe de version propre (`rust-sdk-v*`, comme le SDK TS sous `sdk-v*`).

### 2. Couplage des versions

Les deux crates publiées sont versionnées **à l'identique de la version de workspace** (`[workspace.package].version`, aujourd'hui `0.8.2`) — un tag `vX.Y.Z` publie `carbonfr-core` et `carbonfr-eligibility` à `X.Y.Z`. C'est le 5ᵉ axe ajouté par l'addendum à ADR-0019.

Ce n'est pas qu'une préférence : c'est un **correctif technique requis dès I4**. `cargo package -p carbonfr-eligibility` échoue *aujourd'hui* faute de contrainte de version sur la dépendance interne `carbonfr-core` (preuve ci-dessus). Correction : ajouter `version = "0.8.2"` (ou une plage `"0.8"`) à `carbonfr-core = { path = "crates/core", version = "…" }` dans `[workspace.dependencies]`. La version elle-même reste héritée du workspace (rien à synchroniser à la main à chaque release) — mais ce champ `version` de la dépendance interne devra être relevé en même temps qu'un éventuel changement de plage majeure, garde-fou à ajouter en CI dans l'esprit du garde-fou tag ↔ workspace déjà posé par ADR-0019.

### 3. SemVer des bibliothèques

**En 0.x (pré-1.0)**, une rupture (retrait/renommage public, ajout de variante sur un enum exhaustif consommé par `match`, ajout de champ sur un struct à champs publics) = bump **minor** (`0.x → 0.(x+1)`), jamais patch — cohérent avec l'esprit pré-1.0 d'ADR-0019.

**`#[non_exhaustive]`, décidé enum par enum (25/25, aucune règle en bloc).** Critère retenu : l'enum est un **catalogue voué à grandir** avec le domaine, la donnée ou l'infrastructure de bord (nouvelle source de coût, nouveau canal, nouvelle raison d'indétermination…) → marqué, pour ne jamais casser un `match` externe le jour où une variante s'ajoute. À l'inverse, un ensemble **fini par construction du domaine** (méthodologie à 3 valeurs, direction binaire, topologie figée d'un réseau physique) reste exhaustif : l'étendre serait de toute façon un changement de méthodologie nécessitant un nouvel ADR (garde-fou `CONTRIBUTING.md`), donc rien à protéger côté SemVer.

| Catégorie | `core` (13/18) | `eligibility` (5/7) |
|---|---|---|
| **`#[non_exhaustive]`** (catalogue extensible) | `ApplicationError`, `SourceError`, `RepositoryError`, `ForecastError`, `WebhookUrlError` (les 5 erreurs) ; `ApiTier` (1 variante aujourd'hui, ADR-0015 anticipe des tiers payants) ; `Perimeter` (1 variante `Plateau` aujourd'hui, `cost.rs:218`) ; `CostSource`, `CostTechnology`, `CostBasis` (catalogue LCOE multi-sources, ADR-0024) ; `PriceComponentKind` (composantes réglementaires du prix, ADR-0023) ; `Filiere` ; `Neighbor` (frontières électriques, ENTSO-E) | `IndeterminateReason`, `Pillar`, `EligibilityFramework`, `RulesetStatus`, `EligibilitySignal` (couplé 1:1 à `Pillar` — une variante à champs par pilier + `Indeterminate`, `verdict.rs:82-116`) |
| **Exhaustif** (fini par construction) | `Vintage` (3, ADR-0006) ; `ThresholdDirection` (2, binaire) ; `Region` (13 = national + 12 régions, topologie fixe de la France métropolitaine éCO2mix) ; `Granularity` (2, `Hourly`/`Daily`) ; `WindowEstimator` (2, `Central`/`Prudent`) | `TemporalGranularity` (2, bascule fixée par ADR-0026) ; `ShareSource` (2, `Observed`/`Forecast`) |

**Structs à champs publics (54 : 46 `core` + 8 `eligibility`) : champs publics assumés, aucun `#[non_exhaustive]`, aucun constructeur privé ni builder généralisé.** Ce sont des types de données (`Measurement`, `GenerationMix`, `ForecastPoint`, `Subscription`…), pas des invariants à protéger par construction — le SDK Rust (ADR-0031) et les consommateurs externes doivent pouvoir les construire par littéral. Conséquence assumée : **ajouter un champ public à l'une d'elles est un bump minor**, pas patch (règle ci-dessus) — jamais un `#[non_exhaustive]` posé après coup pour l'éviter, ça casserait justement la construction par littéral que cette doctrine protège. Les structs déjà opaques (cas d'usage génériques type `GetCurrentIntensity`, marqueurs unitaires `RteDirect`/`AcvAdemeProduction`/`AcvAdemeConsumption`, `CarbonIntensity` à champ privé + accesseurs) n'ont besoin d'aucun changement : elles contrôlent déjà leur construction.

**Types tiers exposés, assumés et documentés (pas cachés)** : `time::OffsetDateTime` (17 fichiers `core`, 5 `eligibility`) et `async-trait` (`#[async_trait]` sur les 17 traits publics de `ports.rs` — un implémenteur externe d'un port en dépend lui-même, ou désucre `Pin<Box<dyn Future>>` à la main). `url::{Url, Host}` reste interne à `validate_webhook_url` (signature `fn validate_webhook_url(url: &str) -> Result<(), WebhookUrlError>`, `webhook.rs:128`) — pas un type exposé, donc rien à trancher pour lui. Documenter les deux couplages réels dans le README de chaque crate (déjà prévu au plan I4) plutôt que les masquer : les retirer casserait plus qu'assumer une dépendance stable, déjà dans l'arbre de tout consommateur `tokio`/async.

### 4. `cargo-semver-checks` en CI

Bloquant dès I4, **avant** toute publication — outil en version 0.50.0 (vérifié ce jour, `GET crates.io/api/v1/crates/cargo-semver-checks`). Son mode par défaut compare à la **dernière version publiée sur crates.io** : inutilisable tant que rien n'est publié. Pour I4 : `--baseline-rev <dernier commit où le fichier existait déjà>` (ou `--baseline-root` sur une copie locale), via l'action `obi1kenobi/cargo-semver-checks-action@v2`. **À partir d'I5** (première publication faite), basculer sur le mode par défaut (registre). Tourne sur Rust stable — pas de toolchain nightly à installer.

### 5. MSRV

**`rust-version = "1.88.0"` dans `[workspace.package]`**, à déclarer en I4. Le plancher n'est **pas** l'édition 2024 (1.85) comme l'estimait le plan en survol, mais la crate `time` : verrouillée à `0.3.55` dans `Cargo.lock` (`rust_version: "1.88.0"`), et même la version plancher déclarée par le workspace (`time = "0.3.49"`, `Cargo.toml`) exige déjà `1.88.0` (revérifié via l'API crates.io ce jour). `time` est une dépendance **directe** de `core` et de `eligibility` (`[dependencies]` des deux `Cargo.toml`), pas une transitive évitable. Aucun let-chain ni async-fn-in-trait natif dans le code (`let-else`, stable depuis 1.65, seul sucre récent utilisé ; les traits async passent par `#[async_trait]`) : rien d'autre ne pousserait le plancher plus haut aujourd'hui.

CI (I4) : un job **dédié**, toolchain **épinglée à `1.88.0`** exactement (pas `stable`), en plus du job `stable` flottant déjà en place (`ci.yml:36/88/101`). **Relever la MSRV = bump minor** (même règle qu'en §3), jamais un patch — et seulement quand une dépendance directe l'impose (comme ici), pas par anticipation.

### 6. Feature `serde`

**Exclue explicitement avant 1.0 : aucune feature `serde`, aucun stub réservé.** `core` et `eligibility` n'ont aujourd'hui **aucune** dépendance `serde` (vérifié dans les deux `Cargo.toml`), cohérent avec la règle d'or (« idéalement pas de `serde` dans `core` », `CONTRIBUTING.md`). Contrairement à l'intuition « réserver la structure de `[features]` maintenant pour ne pas casser plus tard » : **ajouter une feature opt-in est additif sous SemVer** (personne ne l'active par défaut, rien ne change pour l'existant) — il n'y a donc aucun bénéfice à la poser en avance, et un coût réel (surface à maintenir sans consommateur identifié). Le jour où un besoin concret apparaît (un consommateur tiers), une feature `serde = ["dep:serde"]` s'ajoute en version *minor*, sans rupture. Le SDK Rust n'en fait pas partie : il a ses propres types sérialisables et duplique les petits enums de domaine (ADR-0031, décision 5).

### 7. Contenu du paquet

`cargo package --list` (core, seul empaquetable aujourd'hui) ne remonte que du code source : `src/`, `tests/use_cases.rs` (28 Kio, fakes en mémoire — pas de fixture lourde), plus les fichiers générés par `cargo package` (`Cargo.toml.orig`, `Cargo.lock`, `.cargo_vcs_info.json`). Rien à exclure. À **ajouter** en I4, sur les deux crates : `readme = "README.md"` (fichier à créer — aucune des deux crates n'en a aujourd'hui), `documentation` (docs.rs, une fois publié), `homepage`, `keywords` (≤ 5), `categories` (taxonomie crates.io). Le SPDX `license.workspace = true` (`"MIT OR Apache-2.0"`) suffit sans embarquer `LICENSE-MIT`/`LICENSE-APACHE` (présents à la racine du dépôt, pas dans chaque crate). **Hors des crates publiées, sans ambiguïté** : migrations SQL, données de la carte `/hydrogene` (ADR-0029), tout adapter, `bin/server` — aucun des deux ne les référence (règle d'or ADR-0002).

> **Points à confirmer par Morgan** (aucun ADR existant ne les tranche — ADR-0027 fixe la gouvernance de `main`, pas la propriété d'un compte de packaging externe) :
> 1. **Propriétaire crates.io** : compte GitHub personnel (`Morgan-Voltz`) ou organisation `Kovelt` à créer ? *Recommandation : `Kovelt`*, cohérent avec `repository = "https://github.com/Kovelt/carbon-fr"` et avec le Trusted Publisher npm déjà déclaré sous `Organization or user: Kovelt` pour le SDK TS (`release-sdk.yml:12`).
> 2. **Trusted Publisher** : dépôt `Kovelt/carbon-fr` (déjà celui de `repository.workspace`) et fichier `release-crates.yml` — à confirmer que c'est bien ce dépôt (pas un fork/mirroir) et ce nom de fichier exact avant de le saisir sur crates.io.
> 3. **Mode « enforcement »** crates.io (désactive les jetons API classiques une fois Trusted Publishing configuré) : durcissement optionnel, décision de sécurité pure sans contrainte technique — à trancher en I5, non bloquant pour la publication elle-même.

### 8. Publication : procédure et ordre

Compte crates.io : voir encadré ci-dessus (décision de Morgan, non tranchée ici faute d'élément qui l'impose techniquement). Gouvernance de la décision : Morgan, mainteneur unique (ADR-0027).

**Ordre imposé par le graphe de dépendance** : `carbonfr-core` d'abord (aucune dépendance interne), puis `carbonfr-eligibility` (dépend de `core`, qui doit déjà exister sur crates.io pour que son `version = "…"` résolve, §2).

**Première publication de chaque crate : manuelle, jeton ponctuel révoqué aussitôt.** Ce n'est pas une préférence de séquencement mais une **contrainte de la plateforme** : le RFC officiel de Trusted Publishing l'énonce explicitement — « A Trusted Publisher Configuration can only be created after an initial manual publishing of a crate. » (RFC 3691, consulté le 2026-09-23 ; un état `PENDING` pré-publication est évoqué en « Future possibilities » mais **non livré** à ce jour). Séquence par crate : revérifier que le nom est toujours libre → `cargo publish --dry-run -p <crate>` → `cargo publish -p <crate>` avec un jeton crates.io **ponctuel** → révocation immédiate du jeton.

**Ensuite, Trusted Publishing pour toutes les versions suivantes** (les deux crates, puis `carbonfr-sdk` en I6 séparément — sa propre déclaration n'est possible qu'après sa propre première publication) : déclarer sur la page crates.io de chaque crate le *trusted publisher* (dépôt, workflow, environnement — cf. encadré) ; workflow `release-crates.yml`, permissions minimales `contents: read` + `id-token: write` (même schéma que `release-sdk.yml:28-30` pour npm), action `rust-lang/crates-io-auth-action@v1` (échange le jeton OIDC GitHub contre un jeton crates.io de courte durée, révoqué par le post-step de l'action en fin de job) ; déclenché par le tag `vX.Y.Z`, alignement garanti par le même garde-fou CI qu'ADR-0019. `--dry-run` systématique avant chaque `cargo publish` réel, y compris en Trusted Publishing.

**Retrait d'une version (`cargo yank`)** : une version publiée ne se supprime ni ne se republie — un correctif est **toujours** une nouvelle version. `cargo yank` ne sert qu'à empêcher les **nouvelles** résolutions d'une version défectueuse (`^`/`~` ne la choisissent plus) ; il n'efface rien et ne casse pas les projets qui l'ont déjà dans leur `Cargo.lock`. Réservé à trois cas : faille de sécurité, build cassé (docs.rs ou en aval), rupture SemVer publiée par erreur en patch — jamais pour une coquille de métadonnées (on publie le patch suivant). Décision de Morgan (ADR-0027), accompagnée d'une entrée CHANGELOG et, pour une faille, d'un avis de sécurité GitHub.

### 9. docs.rs

Aucune configuration spéciale requise (crates pures, pas de `build.rs`, pas de dépendance native) : le build par défaut de docs.rs suffit. docs.rs **ne rejette pas** un avertissement rustdoc (contrairement au job CI local `-D warnings`) — il publierait quand même avec des liens morts si les 10 liens cassés (§ état vérifié) ne sont pas corrigés avant publication ; correction déjà prévue en I4, hors du périmètre de cet ADR. **Suivi 24–48 h après chaque publication** (I5) : l'environnement de build docs.rs peut différer du local, remédiation = version patch au besoin. Badges crates.io/docs.rs ajoutés au README une fois la première page en ligne.

### 10. Conséquences pour I4/I5/I6

Cet ADR ne modifie pas `docs/plan-iterations.md` ; il en fixe les entrées. I4 (préparation, sans publier) exécute §3–§5 et la partie technique de §2 et §7 ; I5 (première publication) exécute §8 dans l'ordre prescrit, puis §9 ; I6 applique la même procédure à `carbonfr-sdk` sous ADR-0031, avec son propre axe de version.

## Conséquences

**Positives** :
- Résout la contradiction ADR-0001/ADR-0019 par une décision tracée, plutôt que par un oubli qui aurait fini par se voir dans le code.
- Chaque décision SemVer/MSRV est **chiffrée** (25 enums classés un par un, 54 structs sous une doctrine unique) : I4 démarre sans ligne directrice à inventer au fil de l'eau.
- Sépare proprement une contrainte de plateforme prouvée (RFC 3691 : pas de Trusted Publishing avant une 1re publication manuelle) d'une préférence de séquencement — rien à re-discuter en I5.

**Négatives / limites assumées** :
- MSRV 1.88.0 est un engagement **public** plus élevé que l'estimation initiale du plan (1.85) : les utilisateurs downstream doivent avoir rustc ≥ 1.88 — assumé, imposé par une dépendance directe (`time`), pas par un choix de confort.
- Aucun builder ni constructeur pour les 54 structs à champs publics : toute nouvelle donnée exposée par ces types est un bump minor, indéfiniment — accepté tant que le projet reste en 0.x.
- Propriété du compte crates.io et détails exacts du Trusted Publisher restent des points ouverts (encadré §7) : cet ADR recommande sans trancher, faute de contrainte technique qui imposerait un choix.

## Alternatives envisagées

- **Publier aussi un adapter** (ex. `adapter-http`, pour exposer les DTO HTTP) — *écartée* : couplé à `axum`, hors de l'esprit « bibliothèque réutilisable » ; rien ne l'exige, les DTO restent un détail d'implémentation du service.
- **Versions indépendantes par crate publiée** (au lieu du couplage à la version de workspace) — *écartée* : désolidariserait le tag `vX.Y.Z` de la version des libs, complexifierait ADR-0019 pour un mainteneur solo sans bénéfice identifié ; et `cargo package -p carbonfr-eligibility` a de toute façon besoin d'*une* version de `core`.
- **`#[non_exhaustive]` partout par défaut** — *écartée* : casserait la construction par littéral des types de données purs (`Measurement`, `GenerationMix`…) dont le SDK Rust (ADR-0031) et les consommateurs externes ont besoin ; le classement enum par enum (§3) demande plus de travail mais évite une règle en bloc contre-productive.
- **Réserver dès maintenant une feature `serde` vide** — *écartée* (§6) : une feature opt-in s'ajoute sans rupture le jour venu, la réserver à vide n'a aucun bénéfice SemVer et ajoute une surface non demandée.
- **Attendre un état `PENDING` de Trusted Publishing pré-publication** (évoqué par le RFC en « Future possibilities ») — *écartée* : non livré à ce jour, aucune confirmation qu'il existe en 2026-09 ; la première publication manuelle reste la seule voie prouvée.
