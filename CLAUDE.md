# CLAUDE.md

Contexte et conventions du projet `carbon-fr` pour les sessions Claude Code. Lis ce fichier en début de session avant toute modification.

## Le projet en une phrase

API d'intensité carbone de l'électricité française (gCO₂eq/kWh), souveraine, open source et dev-first — l'équivalent français de [carbonintensity.org.uk](https://carbonintensity.org.uk/), basée sur les données ouvertes RTE/éCO2mix via ODRÉ.

> **Nom** : `carbon-fr` (confirmé, libre sur crates.io). Les crates publiables sont préfixées `carbonfr-*` (ex. `carbonfr-core`, `carbonfr-adapter-odre`) même si les dossiers restent `crates/core`, `crates/adapter-odre`, etc.

## Stack

- **Langage** : Rust (edition 2024), runtime async **tokio**.
- **Architecture** : hexagonale (ports & adapters), workspace Cargo multi-crates.
- **Web** : axum (en place — `adapter-http`, API `/v1`).
- **Base** : PostgreSQL **natif, sans extension** (pas de TimescaleDB — choix de licence, voir ADR-0004).
- **Erreurs** : `thiserror` dans les bibliothèques, `anyhow` toléré uniquement dans le binaire.
- **Ports async** : `async-trait`.

## Structure du workspace

```
carbon-fr/
├── Cargo.toml                # workspace
├── crates/
│   ├── core/                 # lib PURE : domaine + cas d'usage + ports (zéro IO)
│   ├── eligibility/          # lib PURE : éligibilité électrolyseur rfnbo/low-carbon (ADR-0026) ✅
│   ├── adapter-odre/         # impl Eco2mixSource/Eco2mixArchive (ODRÉ)         ✅
│   ├── adapter-postgres/     # impl repositories (sqlx/Postgres)               ✅
│   ├── adapter-http/         # API axum (adapter entrant, /v1) + auth/SSE       ✅
│   ├── adapter-forecast/     # impl ForecastModel (climatology@1, acv-ademe@2)  ✅
│   ├── adapter-meteo/        # impl WeatherForecastSource (Open-Meteo)          ✅
│   ├── adapter-entsoe/       # impl CrossBorderSource + SpotPriceSource (ENTSO-E) ✅
│   ├── adapter-webhook/      # impl Notifier (livraison signée, anti-SSRF)      ✅
│   └── adapter-gbdt/         # impl ForecastModel ML (GBDT, gardé par backtest) ✅
└── bin/
    └── server/               # composition root : câble adapters + poller       ✅
```

## Règle d'or de l'architecture

**Les dépendances pointent vers l'intérieur. Le domaine ne dépend de rien.**

- `core` ne contient **aucune** IO : pas de `reqwest`, pas de `sqlx`, pas d'`axum`, idéalement pas de `serde`. La (dé)sérialisation et la persistance sont des préoccupations d'**adapters**.
- Le domaine définit des **ports** (traits). Les adapters les **implémentent**.
  - Ports sortants : `Eco2mixSource`, `IntensityRepository`, `ForecastModel`, `Clock`.
  - Ports entrants : les cas d'usage (`GetCurrentIntensity`, `IngestLatest`, `FindGreenestWindow`, …).
- Seul `bin/server` (la *composition root*) connaît les implémentations concrètes et les assemble.

Conséquence pratique : un changement de source de données, de base, ou de modèle de prévision = **un nouvel adapter**, sans toucher au domaine ni à l'API.

## Conventions de code

- Pas d'`unwrap()` / `expect()` hors tests et hors `main` de bootstrap.
- Les cas d'usage sont génériques sur leurs ports (`struct UseCase<R: IntensityRepository>`), dispatch statique, zéro coût.
- Les tests du `core` se font **sans IO**, avec des *fakes* en mémoire implémentant les ports (c'est le bénéfice de l'hexagonal — on le démontre dans les tests).
- `cargo fmt` + `cargo clippy --all-targets -- -D warnings` doivent passer avant tout commit.
- Unité canonique : **gCO₂eq/kWh**. Pas de stépsite quart d'heure implicite — l'horodatage est porté explicitement par chaque mesure.

## Domaine — repères

- **Régions** : `National` + 12 régions métropolitaines (couverture éCO2mix régional).
- **Filières** : nucléaire, gaz, charbon, fioul, hydraulique, éolien, solaire, bioénergies, pompage, échanges.
- **Méthodologie carbone** (ADR-0005, **Accepté**) : base MVP = estimation RTE (`rte-direct`, émissions de la production FR). La méthodologie est un **attribut versionné porté par chaque `Measurement`** (champ `methodology`) — pas une constante globale. Enrichissement engagé : une méthode `acv-ademe` (cycle de vie + imports) coexistera plus tard sans rupture. Toute nouvelle méthode = nouvelle version + nouvel ADR ; **jamais** de modification silencieuse d'une méthode publiée.
- **Millésime / révisions** (ADR-0006) : la donnée RTE est révisée (`tr` → `consolidated` → `definitive`). Chaque `Measurement` porte un champ `vintage`. Clé d'unicité = `(region, horodatage, methodology)`. L'ingestion fait un **upsert conditionnel** : un millésime n'écrase l'existant que s'il est de qualité ≥ (`definitive` > `consolidated` > `tr`). Donc `IntensityRepository` expose un upsert, pas un simple insert. On sert toujours la meilleure version et on expose le millésime.

## À NE PAS faire

- Mettre `serde` / `sqlx` / `axum` dans `core`.
- Faire taper RTE directement à chaque requête utilisateur : un **poller unique** (singleton) alimente la base, l'API sert depuis la base (le quota de 50 000 appels/mois est consommé à moins de 8 % par construction).
- **Backfiller l'historique via l'API paginée** : utiliser l'**export en masse** d'ODRÉ (un téléchargement), sinon on brûle le quota.
- Traiter la donnée comme **append-only** : elle est révisée → upsert conditionnel au millésime (voir repères Domaine + ADR-0006).
- **Exposer l'API sans préfixe de version** : tout endpoint public est sous `/v1` (l'URL est un contrat — ADR-0007).
- Étendre le périmètre méthodologique (cycle de vie, imports) sans ADR.
- Reproduire la donnée RTE comme si elle était nôtre : on re-traite, on cite la source.

## Commandes

```bash
cargo check --workspace
cargo test --workspace                       # hermétique (sans réseau ni base)
cargo clippy --all-targets -- -D warnings
cargo fmt --all
cargo deny check                             # licences + advisories RustSec + sources (deny.toml)

# Lancer l'API (migrations appliquées au démarrage du serveur) :
DATABASE_URL=postgres://localhost/carbonfr cargo run -p server

# Backfill de l'historique national par export de masse (one-shot) :
DATABASE_URL=postgres://localhost/carbonfr cargo run -p server backfill

# Backtest du modèle de prévision climatology@1 (walk-forward, MAE/RMSE) :
DATABASE_URL=postgres://localhost/carbonfr cargo run -p server backtest
# Backtest de la prévision acv-ademe@2 (vérité dérivée, national) :
DATABASE_URL=postgres://localhost/carbonfr cargo run -p server backtest-acv
# Calage des paramètres (balayage N × τ, classé par RMSE) :
DATABASE_URL=postgres://localhost/carbonfr cargo run -p server backtest-sweep
# Calibration des intervalles (quantiles de résidus par horizon) :
DATABASE_URL=postgres://localhost/carbonfr cargo run -p server backtest-bands
# Backtest de la dérivation renouvelable météo→production (ADR-0018, CARBONFR_BACKTEST_FROM/_TO) :
DATABASE_URL=postgres://localhost/carbonfr cargo run -p server backtest-renewable
# Analyse-gate : l'anomalie de renouvelable améliore-t-elle la climatologie d'intensité ? (ADR-0018 étape A) :
DATABASE_URL=postgres://localhost/carbonfr cargo run -p server analyze-renewable-signal
# Entraîner le modèle ML GBDT + comparer au backtest (CARBONFR_TRAIN_FROM/_TO, _GBDT_MODEL) :
DATABASE_URL=postgres://localhost/carbonfr cargo run -p server train
# Délivrer une clé API tier gratuit (ADR-0015 ; empreinte stockée, clé affichée 1 fois) :
DATABASE_URL=postgres://localhost/carbonfr CARBONFR_KEY_LABEL=projet cargo run -p server mint-key

# Tests d'intégration nécessitant des ressources externes :
DATABASE_URL=postgres://localhost/carbonfr_test \
  cargo test -p carbonfr-adapter-postgres --test pg     # Postgres réel
cargo test -p carbonfr-adapter-odre --test live -- --ignored   # API ODRÉ réelle
```

## Décisions (ADR)

Le « pourquoi » des choix vit dans [`docs/adr/`](docs/adr/). Lire au minimum :
- ADR-0002 (architecture hexagonale + workspace) — cadre tout le reste.
- ADR-0003 (périmètre & source ODRÉ) — d'où vient la donnée, et ce qui n'y est pas.
- ADR-0004 (Postgres natif) — pourquoi pas de TimescaleDB.
- ADR-0005 (méthodologie carbone) — **Accepté** : `rte-direct` en MVP, méthodologie versionnée par mesure, enrichissement `acv-ademe` engagé.
- ADR-0006 (cycle de vie & révision) — **Accepté** : millésime `tr`/`consolidated`/`definitive`, upsert conditionnel sur `(region, horodatage, methodology)`.
- ADR-0007 (déploiement) — **Accepté** : API sur VPS FR/EU (PostgreSQL co-localisé), site statique sur o2switch, sous-domaine Kovelt, API versionnée `/v1`.
- ADR-0008 (méthodologie `acv-ademe` & régional) — **Accepté** : intensité cycle de vie (facteurs ADEME × mix), `acv-ademe@1` basée production (imports = v2), dérivée à l'ingestion, sélectionnable via `?methodology=`. Base du futur régional.
- ADR-0009 (modèle de prévision) — **Accepté** (+ addendum calibration) : `climatology@1` = climatologie horaire-de-semaine glissante (`N` sem.) + correction d'anomalie décroissante (`τ`). Pur/explicable, sans dépendance externe, alimenté par le backfill ; prévisions **non persistées** (calculées à la lecture). **Défauts calés par backtest : N=10 sem., τ=2 sem.** (bat la persistance ; τ court la dégradait). Versionné comme la méthodologie. Évolution engagée : `forecast@2` dérivé des prévisions RTE J-1, même port.

### ADR de la vision forward (phases 4-5 — statut indiqué par ADR)

> Ces décisions cadrent l'enrichissement. La plupart sont désormais **acceptées et (au moins partiellement) implémentées** — le statut réel est porté par chaque entrée ci-dessous (et détaillé dans la section « État d'avancement »). Seuls quelques chantiers restent partiels (`[~]`) ou à venir.

- ADR-0010 — `acv-ademe` **consumption-based** (**Accepté, engagé**) : imports valorisés à l'intensité du pays d'origine (ENTSO-E via `adapter-entsoe`), `MethodologyCalculator` (trait domaine pur), endpoints `/v1/methodologies` & `/v1/factors`. **Fait évoluer** l'ADR-0008 (livré en `@1` production-based). **Livré** (tranches A + B) : domaine pur (`MethodologyCalculator`/`RteDirect`/`AcvAdemeProduction`/`AcvAdemeConsumption`, `CrossBorderFlows`, calcul `acv-ademe@2` + pertes T&D) ; endpoints de vérifiabilité `/v1/methodologies` & `/v1/factors` ; adapter `adapter-entsoe` (flux A11 + intensité voisine via génération A75) ; store Postgres `cross_border_flow` + ingestion poller (si token) + service de `@2` via `?methodology=acv-ademe&version=2` (national). **Reste** : valider les chemins XML ENTSO-E live, sourcer le facteur T&D, `@2` sur historique/stats.
- ADR-0011 — **contrat de prévision `ForecastPoint`** (**Accepté, implémenté**) : type domaine dédié (intervalles `lower`/`expected`/`upper`, `ModelVersion`, **pas de `vintage`**, invariant garanti), remplaçant le `Vec<Measurement>` du port. `/v1/intensity/forecast` expose l'intervalle ; `greenest-window` a un sélecteur `central`/`prudent`. Intervalle v1 = dispersion empirique par créneau ; quantiles par horizon = raffinement derrière le même contrat.
- ADR-0012 — modèle de prévision **ML** (`GbdtForecaster`, GBDT tout-Rust + features météo) derrière le même port ; ne livre que s'il bat le `StatForecaster` au backtest. **Accepté, engagé** : store météo livré (port `WeatherForecastSource` + `adapter-meteo` Open-Meteo + table `weather_forecast` anti-fuite `(run_at, valid_at)`) ; framework GBDT (`adapter-gbdt`) + sous-commande `train` + `GbdtForecaster` livrés, mais **mesuré : `gbdt@1` ne bat pas `climatology@1`** (RMSE ≈15 vs 7,5, même avec météo) → `@1` reste servi, gain ML = itération ouverte.
- ADR-0013 — **prévision `acv-ademe`** (**Accepté, engagé**) : prévoir les entrées (mix + imports) puis appliquer le calculateur ; `MixForecaster` + `CrossBorderForecastSource`. **Tranches A + B livrées** : `acv_ademe_forecast` (climatologie par canal → calculateur, converge au nowcast), adapter `AcvAdemeForecaster<R,C>`, routage par méthode (`ForecastState` + modèle `@2` dynamique), servi via `GET /v1/intensity/forecast?methodology=acv-ademe&version=2` ; backtest dédié (`backtest-acv`, vérité dérivée) + intervalles calibrés. **À venir** : GBDT `MixForecaster`, ENTSO-E day-ahead.
- ADR-0014 — **usage** (**Accepté, livré** sauf webhooks) : primitives carbon-aware (créneau sous échéance, lowest-k, seuil, annotation d'économie) + livraison live **SSE** (`/v1/intensity/stream`) ; webhooks reportés (gated sur le tier hébergé). **Tranche A** : primitives pures + `CarbonAwareScheduler` + endpoints `/v1/schedule`, `/v1/schedule/slots`, `/v1/intensity/below`. **Tranche B** : SSE via canal mémoire `tokio::broadcast` (migration `LISTEN`/`NOTIFY` documentée pour `bin/poller` séparé).
- ADR-0016 — **webhooks** (**Accepté, implémenté**) : notification sortante signée, gardée par l'*ownership* de clé (ADR-0015). **Tranche A** : fondation **pure** de sécurité (`should_fire` edge-triggered, `validate_webhook_url`/`is_public_ip` anti-SSRF, `hmac_sha256_hex` validé RFC 4231) ; ports `SubscriptionRepository`/`Notifier`. **Tranche B** : store `webhook_subscription` (scopé propriétaire) + crate `adapter-webhook` (`HttpNotifier` : re-validation SSRF à la résolution DNS, pas de redirect, retries backoff) + watcher (sur le flux `IntensityUpdate`) + endpoints `POST`/`GET` `/v1/webhooks` + `DELETE /v1/webhooks/{id}` (clé requise).
- ADR-0015 — **tier hébergé** (**Accepté, engagé**) : clés API (`Bearer`) en **middleware de bord** (`core` intact, port `ApiKeyRepository` sur Postgres), **anonyme conservé par défaut** (auth opt-in, self-hosting préservé), payant = extension future non-bloquante. Débloque les webhooks (fournit l'*ownership*). **Tranche A livrée** : middleware `enforce` (401/429 + en-têtes `RateLimit-*`), quota fenêtre-minute en mémoire, table `api_key`, sous-commande `mint-key`, opt-in via `CARBONFR_RATELIMIT_ENABLED`. Webhooks **débloqués et livrés** (ADR-0016). **À venir** : `UsageMeter` persistant + palier payant.
- ADR-0017 — **endpoint public des échanges transfrontaliers** (**Accepté, implémenté**) : `GET /v1/exchanges` (+ `/exchanges/date`) expose la donnée ENTSO-E déjà ingérée (flux net signé par frontière + intensité du voisin), projection de lecture pure (`GetCrossBorderExchanges`), sans nouvelle ingestion. `gb` absent (indispo ENTSO-E depuis le Brexit).
- ADR-0018 — **dérivation renouvelable** (**Accepté, engagé**) : météo → production éolien/solaire via un intermédiaire physique explicable, capacités calibrées sur l'historique (`RenewableModel`, `calibrate_renewable`). Prouvé au backtest (×2,4/×3,4 vs baseline), exposé `GET /v1/renewable` (+ `/v1/weather` et historique, attribution Open-Meteo CC-BY 4.0). **Addendum** : la prévision d'intensité météo-pilotée (`forecast@N`) est **écartée** (gate mesuré : gain marginal sur le réseau FR nucléaire ; `analyze-renewable-signal`).
- ADR-0019 — **politique de versionnement** (**Accepté, engagé**) : voir la section dédiée ci-dessous.
- ADR-0020 — **politique de dépréciation** (**Accepté**) : on ne retire jamais un élément public (version d'API, endpoint, champ, méthodologie) sans préavis ; annonce via en-têtes `Deprecation` (RFC 9745) + `Sunset` (RFC 8594) + CHANGELOG + `deprecated` OpenAPI ; fenêtre ≥ 6 mois (post-1.0) / ≥ 30 j (pré-1.0). En-têtes **non implémentés tant que rien n'est déprécié** (pas de code mort) — l'utilitaire arrivera avec sa première dépréciation.
- ADR-0021 — **format d'erreur Problem Details** (**Accepté, implémenté**) : toutes les erreurs en `application/problem+json` (RFC 9457) — `type`(=`about:blank`)/`title`/`status`/`detail` + extension **`code`** stable. Fonction unique `problem_response` (partagée handlers + middleware auth). DTO `ProblemDetails` (jamais `ErrorBody`). Le SDK lit `code`/`detail`.
- ADR-0022 — **observabilité `/metrics`** (**Accepté, implémenté**) : endpoint Prometheus **hors `/v1`** (texte, pas dans l'OpenAPI), registre **fait maison** (`bin/server/src/metrics.rs`, atomiques) alimenté par le poller. Fraîcheur (`*_last_success_timestamp_seconds`), volume/erreurs, `upstream_requests_total{source}` (proxy quota ODRÉ), `build_info{version}`. Alerte phare = fraîcheur > 2× intervalle de poll.
- ADR-0023 — **affichage du prix de l'électricité** (**Accepté, implémenté**) : `GET /v1/price` (+ `/price/date`) = **décomposition complète** du prix payé ancrée sur le TRV, jamais deux chiffres en regard. Composante **énergie = prix spot day-ahead ENTSO-E** (`documentType=A44`, port `SpotPriceSource`/`SpotPriceRepository`, table `spot_price`, ingéré par le poller si token ENTSO-E) ; acheminement (TURPE 7) + accise + TVA + résidu commercialisation = **constante de domaine versionnée** (`TrvReference::trv_2026`, **valeurs 2026 sourcées** CRE/BOFiP : accise 30,85 €/MWh, TVA 20 %, commercialisation 18,11 €/MWh HT, TURPE ≈ 78 €/MWh dérivé pour 6 kVA/2 400 kWh). Contexte : mix par filière + **technologie marginale estimée** (ordre de mérite, `estimated:true`). National. Domaine pur (`price.rs`), zéro `serde`/`sqlx` dans `core`.
- ADR-0024 — **couche comparative LCOE** (**Accepté ; GATE de neutralité franchi le 2026-06-20**, cf. [`docs/adr/0024-revue-neutralite.md`](docs/adr/0024-revue-neutralite.md)) : `GET /v1/cost-reference` = **estimation** versionnée en **fourchette** (min/médiane/max) par filière (`cost.rs`), clé `source × technologie × périmètre × millésime`, **jamais** mise en différence avec le prix de marché. Nucléaire **scindé** existant (coût comptable amorti) / nouveau (LCOE prospectif, champ `basis`), périmètre `plateau` **uniforme et explicité** (exclut coûts système ET démantèlement/déchets), note neutre obligatoire. Sources ADEME/IRENA (renouvelables) + Cour des comptes/CRE (nucléaire existant) + RTE (nucléaire nouveau, mono-source). **Multi-sources** depuis 2026-06-20 (dispersion inter-sources ; champs `geography` france/monde + `technology_source_count` exposés). **Licences confirmées** : ADEME/IRENA = licences ouvertes/permissives (sans NC) ; CdC/CRE = CRPA L321-1 ; RTE = non-protection des faits + extraction non substantielle ; AIE/IPCC/Fraunhofer/NEA écartés pour clause NC, Lazard propriétaire — réutilisation **chiffres-faits uniquement** ; **confirmation écrite RTE recommandée pour un palier payant**. GATE de neutralité **re-joué (n°3) GREEN** après l'ajout des 2e sources. Reste (gouvernance) : 2e source licence-compatible pour le nucléaire neuf, contre-source France pour les EnR. Le GATE a été conduit par **évaluation adversariale multi-agents** (critiques pro **et** anti-nucléaire + audits structurels) sur la sortie réelle ; verdict RED initial → 7 correctifs → GREEN.
- ADR-0025 — **extension hydrogène carbon-aware** (**Accepté ; couche A implémentée**) : sert l'opportunité H₂ comme **extension de la couche carbon-aware** (ADR-0014), **pas** un produit sœur. **Couche A** = overlay « électrolyseur » évaluant l'éligibilité **au niveau réseau** par créneau, sous deux cadres explicitement étiquetés et neutres — `rfnbo` (renouvelable, Règl. délégués UE 2023/1184-1185) et `low-carbon` (bas-carbone inclusif nucléaire/CCS, acte délégué 8/7/2025) — au-dessus des données régionales horodatées et de `/greenest-window`. **Hors périmètre** : gCO₂eq/kgH₂ et certification (donnée au niveau site absente). ⚠️ La corrélation **géographique** s'évalue à la **zone de dépôt nationale** (FR = 1 zone), pas aux 12 sous-régions. Seuils **paramétrables/versionnés**. Méthodologie fine = **ADR-0026** (rédigée). **Couches B-light** (carte infra × carbone live) et **B-full** (observatoire H₂) documentées en réserve.
- ADR-0026 — **méthodologie des overlays d'éligibilité** (**Accepté, implémenté**) : crate domaine pur `carbonfr-eligibility` (rulesets versionnés `rfnbo:2023-1184` / `low-carbon:2025-2359` servis + `rfnbo:2026-revision` planifié non servi). `rfnbo` = **disjonction** des exceptions réseau (part renouvelable instantanée ≥ 0,90 **OU** prix day-ahead ≤ 20 €/MWh) ; `low-carbon` = **condition nécessaire** intensité ≤ seuil **dérivé** `round(3384/53) ≈ 64 gCO₂eq/kWh` (proxy `indicative`, non réglementaire — l'acte 2025/2359 ne fixe qu'un seuil produit). Exposé via `GET /v1/intensity/greenest-window?eligibility=rfnbo|low-carbon` (param orthogonal à `methodology`, réponse additive) + catalogue `GET /v1/eligibility/rulesets`. **Mono-forecast** (un seul `forecast()` partagé fenêtre/éligibilité), estimateur propagé, `low-carbon` indéterminé si le seuil ∈ [lower, upper] (ADR-0011). **Piège 1** : `bidding_zone` toujours `FR`. **Piège 2** : prix indéterminé au-delà du day-ahead (filtre ≤ 1 h), jamais extrapolé. Part renouvelable future = nowcast-only (ancre `rte-direct`) → `MixForecast` réservé. `core` intact (orchestration côté adapter HTTP). Branche EUA du surplus & additionnalité PPA **hors périmètre**, documentées dans le `disclaimer`. Brief d'implémentation : [`docs/brief-claude-code-couche-A-electrolyseur.md`](docs/brief-claude-code-couche-A-electrolyseur.md) ; plan : [`docs/plan-couche-A-electrolyseur.md`](docs/plan-couche-A-electrolyseur.md).
- ADR-0027 — **politique de contribution & verrouillage de branche** (**Accepté ; Phase A appliquée le 2026-06-21**) : `main` protégée par un **ruleset GitHub** (`protect-main`, mis à jour — pas un doublon). **Phase A (solo)** : PR obligatoire (zéro push direct), **CI verte = les 5 jobs de `ci.yml`** (`fmt + clippy`, **`cargo-deny`** = porte de pureté de licence, `tests`, `build release`, `SDK TypeScript`), branche à jour (`strict`), conversations résolues, historique linéaire (squash/rebase), force-push/suppression interdits, **`bypass_actors` vide** (zéro exception, mainteneur compris) ; pas d'approbation humaine (auto-approbation impossible en solo). **Phase B (1re contrib externe)** : +1 approbation + `CODEOWNERS`. Artefacts : `CONTRIBUTING.md`, `.github/CODEOWNERS`, templates PR/issue, [`.github/ruleset-main-phaseA.json`](.github/ruleset-main-phaseA.json). Brief : [`docs/brief-claude-code-ruleset-main.md`](docs/brief-claude-code-ruleset-main.md). ⚠️ Renuméroté depuis « 0015 » (déjà pris par le tier hébergé). ⚠️ Les `context` de status checks = le **`name:` du job**, pas son id YAML (`integration_id` 15368 = GitHub Actions).

## Versionnement (ADR-0019)

**Quatre axes découplés, aucun ne pilote les autres** :

1. **Version applicative** — SemVer **unique de workspace** (`[workspace.package] version` dans `Cargo.toml` racine, héritée par toutes les crates via `version.workspace = true`). Pas de version par crate (les crates ne sont **pas** publiées sur crates.io ; le service se distribue en image Docker). Pré-`1.0` tant que des ruptures internes restent possibles.
2. **Contrat d'API** — l'URL `/v1` ; on bump vers `/v2` **seulement** sur rupture, et les versions coexistent (ADR-0007/0011).
3. **Méthodologies & modèles** — version **portée par la donnée** (`rte-direct`, `acv-ademe@1`/`@2`, `climatology@1`, `gbdt@1`), **immuable une fois publiée** (ADR-0005/0006/0009).
4. **SDK** — `@carbon-fr/sdk`, tag `sdk-v*`, suit le **contrat d'API** (pas le code serveur).

Invariant : `v0.3.2` (code) ≠ `/v1` (contrat) ≠ `acv-ademe@2` (donnée) ≠ `sdk-v0.1.0` (client).

**Release applicative** : `git tag vX.Y.Z` (doit refléter la version du workspace — garde-fou CI) → le workflow `release.yml` construit et **publie l'image sur GHCR** taguée `X.Y.Z`/`X.Y`/`latest` (`ghcr.io/kovelt/carbon-fr`, publique). Le binaire expose sa version au démarrage (log) et via `--version`. En prod : **épingler une version exacte** (rollback = redéployer le tag précédent). La version OpenAPI servie sur `/docs` est câblée sur `CARGO_PKG_VERSION`.

## État d'avancement

- [x] Cadrage + documentation (ADR, ARCHITECTURE).
- [x] Phase 1 — socle `core` + adapters ODRÉ/Postgres/HTTP + `bin/server` (poller unique + `/v1/intensity/now`, `/v1/mix`, `/health`). Validé de bout en bout (national).
- [x] Phase 2 — historique & régional :
  - [x] **backfill historique national** par export de masse (`carbonfr-server backfill`, dataset `eco2mix-national-cons-def`). Validé de bout en bout.
  - [x] endpoint de lecture d'historique `/v1/intensity/date?from=&to=` (cas d'usage `GetIntensityHistory`, fenêtre ≤ 366 j).
  - [x] rollups (vues matérialisées horaire/journalier) + `/v1/intensity/stats` (résumé exact sur `measurement` + série depuis les rollups ; rafraîchis par poller & backfill).
  - [x] **méthodologie `acv-ademe`** (cycle de vie, ADR-0008) : définie + dérivée/stockée à l'ingestion + `?methodology=`. **National** (dérivé du mix complet) **et 12 régions** (mix régional `eco2mix-regional-*`, `thermique` agrégé → facteur gaz). `rte-direct` reste national.
- [x] Phase 3 — prévision (modèle livré & calé ; **contrat `ForecastPoint` posé**, ADR-0011) :
  - [x] **ADR-0009** — modèle `climatology@1` (climatologie horaire-de-semaine glissante + correction de persistance décroissante). Pur, explicable, sans dépendance externe, alimenté par le backfill. Prévisions **non persistées** (calculées à la lecture, ADR-0006 intacte). Endpoints `/v1/intensity/forecast` et `/v1/intensity/greenest-window`.
  - [x] fonction pure de domaine (`climatology_forecast`) + adapter `ClimatologyForecaster` (`ForecastModel`, lit l'historique via `IntensityRepository`).
  - [x] handlers `/v1` (`forecast` + `greenest-window`) + DTO (id de modèle `climatology@1`) + OpenAPI + câblage composition root.
  - [x] collection Bruno des deux endpoints de prévision.
  - [x] **backtest** walk-forward (`carbonfr-server backtest`) : MAE/RMSE global + par horizon (h+1/h+6/h+24), modèle vs persistance. Maths d'erreur pures (`ErrorAccumulator`/`ErrorMetrics`), orchestration en cas d'usage `BacktestForecast` (testée avec fakes).
  - [x] **calage N/τ mesuré** (`backtest-sweep`, balayage N × τ) sur la vraie donnée 2024 (national `rte-direct`, 2 mois indépendants). Défauts révisés : **N = 10 sem., τ = 2 sem.** (l'ancien τ=6 h sous-performait la persistance ; un τ long = climatologie corrigée de l'anomalie, bat la persistance). Cf. addendum ADR-0009. ⚠️ Le jeu consolidé est au **pas 30 min** (`CARBONFR_BACKTEST_STEP_MINUTES`).
  - [x] **rework de contrat `ForecastPoint`** (ADR-0011) — type domaine `ForecastPoint` (`expected`/`lower`/`upper` + `ModelVersion`, **sans `vintage`**, invariant garanti) remplaçant le `Vec<Measurement>` ; port + `greenest_window` retypés ; `/v1/intensity/forecast` expose l'intervalle, `greenest-window` gagne `?estimator=central|prudent`.
  - [x] **intervalles par quantiles de résidus par horizon** (ADR-0011 §5) — type `HorizonBands` calibré par `BacktestForecast::calibrate_bands` (erreur observé−prévu par horizon, quantiles 10/90) ; **s'élargit avec l'horizon** (mesuré 2024 : ~8→12→17 à h+1/h+6/h+24). Serveur **auto-calibre au démarrage** (`CARBONFR_FORECAST_CALIBRATE_WEEKS`, repli dispersion par créneau) ; sous-commande `backtest-bands`.
  - [x] **ajustement charge (ADR-0011 §4) — essayé & écarté** : `climatology@2` (`β·anomalie de charge prévue`) **dégrade** `@1` au backtest (même avec charge parfaite) → le signal de charge ira dans le ML (ADR-0012), pas en ajustement linéaire. **Conservé** : le **store de charge** (port `ConsumptionRepository`/`ConsumptionSource`, table `consumption`, ingestion poller + backfill) — entrée réutilisable du futur ML.

- [ ] Phase 4 — **enrichissement & usage** (ADR proposés 0010, 0012-0014) :
  - [x] `acv-ademe` **consumption-based** + `adapter-entsoe` + `/v1/factors` (ADR-0010) :
    - [x] **tranche A** — domaine pur : trait `MethodologyCalculator` (`RteDirect`/`AcvAdemeProduction`/`AcvAdemeConsumption`), value object `CrossBorderFlows` (+ enum `Neighbor`), calcul pur `acv_ademe_consumption_intensity` (imports à l'intensité du voisin − exports + pertes T&D), `acv-ademe@2` (version distincte de `@1`, `@1` reste publié). Endpoints **`/v1/methodologies`** (catalogue + versions) & **`/v1/factors`** (table des facteurs + facteur T&D), **sans dépendance externe** (vérifiabilité, ADR-0010 §7). ⚠️ `TD_LOSS_FACTOR_V1=0,072` à sourcer précisément (RTE/ADEME) avant publication de `@2`.
    - [x] **tranche B** — ENTSO-E :
      - [x] **B 1/2** : port `CrossBorderSource` + `CrossBorderSnapshot` (domaine) + crate `carbonfr-adapter-entsoe` (flux physique net signé `documentType=A11` ; intensité voisine dérivée de la génération par type `A75`/`processType=A16` via les facteurs ADEME ; mapping `PsrType`→filières ; zones EIC 6 frontières ; assemblage en snapshots alignés). Parsing XML testé sur fixtures ; chemins **à valider live** (`tests/live.rs --ignored`, `CARBONFR_ENTSOE_TOKEN`). ⚠️ codes/chemins ENTSO-E calés sur le guide, non confirmés contre l'API réelle.
      - [x] **B 2/2** : `CrossBorderRepository` Postgres (migration `0007`, table `cross_border_flow` `(at, neighbor)`, `upsert_flows`/`flows_at`/`flows_range`, testé sur Postgres réel) + ingestion poller (si `CARBONFR_ENTSOE_TOKEN`, source optionnelle) + service `acv-ademe@2` **à la lecture** (cas d'usage `GetConsumptionIntensity`, sans stockage `@2`) sur `/now`, **`/intensity/date` et `/intensity/stats`** (`?methodology=acv-ademe&version=2`, national). Série dérivée par `derive_consumption_series` (jointure mix × contexte d'import ≤) ; agrégats `summarize`/`bucketize` en mémoire (pas de rollup `@2`). `@2` = `served` dans `/v1/methodologies`. **Reste** : valider chemins XML ENTSO-E live, sourcer le facteur T&D.
  - [~] modèle **ML GBDT** (tout-Rust) + features météo, derrière le port, gardé par le backtest (ADR-0012) :
    - [x] **store météo prévisionnel** : port `WeatherForecastSource` + adapter `carbonfr-adapter-meteo` (Open-Meteo, vent 100 m + irradiance, 7 points FR moyennés), store `WeatherRepository` (table `weather_forecast`) daté `(run_at, valid_at)` **anti-fuite**, ingestion poller.
    - [x] **framework GBDT** (crate `carbonfr-adapter-gbdt`, `gbdt` pur Rust) : feature engineering partagé train/inférence (ancre = dernière obs. avant l'origine), `build_training_examples`/`train_model`, `GbdtForecaster` (artefact chargé par chemin), sous-commande `train` (entraîne → sauve → compare au backtest). **Mesuré (sans météo) : `gbdt@1` ne bat pas `climatology@1`** (RMSE ≈15,8 vs 7,5 nov. 2024) — attendu, la météo est le levier. `@1` reste servi.
    - [x] **tranche 2b** : backfill historique météo (API archive Open-Meteo, anti-fuite `run_at=valid_at−24h`) + features météo (vent/irradiance *as-of*) + climatologie de créneau (apprentissage résiduel), calcul **identique** train/inférence. **Mesuré : `gbdt@1` ne bat toujours pas `climatology@1`** (~2× pire, RMSE ≈15 vs 7,5), y compris entraîné sur l'année entière (les arbres n'extrapolent pas) → la climatologie calibrée est une baseline difficile. `@1` reste servi ; faire gagner le GBDT = itération ML ouverte (tuning/features), non engagée. Correctif : dédup `(region,at)` dans `upsert_loads`.
  - [~] **prévision `acv-ademe`** (prévoir les entrées → calculateur ; ADR-0013) :
    - [x] **tranche A** — pipeline + baseline climatologique : `acv_ademe_forecast` (domaine pur : climatologie par canal des entrées → calculateur `AcvAdemeConsumption`, **converge au nowcast**), adapter `AcvAdemeForecaster<R,C>`, routage par méthode au composition root (`ForecastState` + modèle `@2` dynamique `Arc<dyn ForecastModel>`), servi via `GET /v1/intensity/forecast?methodology=acv-ademe&version=2` (national).
    - [x] **tranche B** — backtest & calibration `@2` : cas d'usage `BacktestConsumptionForecast` (vérité **dérivée** de l'observé mix+import via `derive_consumption_series`, walk-forward anti-fuite vs persistance), sous-commande `backtest-acv`, intervalles calibrés par quantiles de résidus (`calibrate_bands`) + auto-calibrés au démarrage (`AcvAdemeForecaster::with_bands`).
    - [ ] **à venir** : `MixForecaster` GBDT multi-sorties + `CrossBorderForecastSource` ENTSO-E day-ahead (proxy actuel = climatologie du contexte stocké). GBDT gardé par promotion (doit battre ce baseline au `backtest-acv`).
  - [x] **usage** : primitives de scheduling carbon-aware + streaming **SSE** (ADR-0014) ; webhooks reportés (tier hébergé) :
    - [x] **tranche A** — primitives **pures** (`schedule.rs` : `greenest_window_before` sous échéance, `lowest_slots` divisible, `slots_below` seuil, `savings_vs_now`/`Savings` Δ vs maintenant + absolu si `energy_kwh`), cas d'usage `CarbonAwareScheduler`, endpoints `GET /v1/schedule`, `/v1/schedule/slots`, `/v1/intensity/below`. Zéro nouveau port, anonyme/sans état.
    - [x] **tranche B** — livraison live **SSE** : `GET /v1/intensity/stream` (`text/event-stream`, événement `intensity`, filtres `region`/`below`, keep-alive). Type domaine `IntensityUpdate` + `StreamState` (canal `tokio::broadcast`). **Mécanisme = canal mémoire** (poller intégré) ; migration `LISTEN`/`NOTIFY` documentée pour un futur `bin/poller` séparé. Le poller publie chaque mesure nationale `rte-direct`. Webhooks toujours reportés (gated ADR-0015).
  - [~] **tier hébergé** : clés API en middleware de bord, anonyme par défaut, `core` intact (ADR-0015) ; débloque les webhooks :
    - [x] **tranche A** — port `ApiKeyRepository` (+ `ApiTier`/`ApiKeyRecord`, **avec le port, pas dans le domaine**) + table `api_key` (empreinte SHA-256, validée Postgres réel) ; middleware `enforce` (`adapter-http`) : principal anonyme/clé, quota fenêtre-minute en mémoire, `401`/`429` + en-têtes `RateLimit-*`. **Opt-in** (`CARBONFR_RATELIMIT_ENABLED`, défaut off → parité self-hosting). Sous-commande `mint-key`. **`core` strictement intact** (aucun cas d'usage).
    - [ ] **à venir** : `UsageMeter` persistant (métering/analytics), identité email/lien magique, payant (adapter facturation cantonné à l'instance).
  - [~] **webhooks** (ADR-0016, débloqués par les clés API) :
    - [x] **tranche A** — fondation **pure** de sécurité (`webhook.rs`) : `should_fire` (*edge-triggered*), `validate_webhook_url`/`is_public_ip` (anti-SSRF, HTTPS + deny-list), `hmac_sha256_hex` (validé RFC 4231), modèle `Subscription` ; ports `SubscriptionRepository`/`Notifier`. **`core` pur** (sha2 = calcul, pas d'IO).
    - [x] **tranche B** — store Postgres `webhook_subscription` (CRUD scopé propriétaire, validé base réelle, migration `0009`) + crate `carbonfr-adapter-webhook` (`HttpNotifier` : **re-validation SSRF à la résolution DNS** + refus des redirects + retries backoff borné) + watcher de fond (flux `IntensityUpdate` → `should_fire` → signature HMAC → livraison) + endpoints `POST`/`GET` `/v1/webhooks` + `DELETE /v1/webhooks/{id}` (**clé requise**, secret affiché une fois). **Quota par clé (50)** ajouté (audit). Reste : désactivation sur échecs.

- [x] Phase 5 — **enrichissement, déploiement & SDK** :
  - [x] **échanges transfrontaliers** (ADR-0017) : `GET /v1/exchanges` (+ `/exchanges/date`), flux signé par frontière + carbone du voisin (donnée ENTSO-E déjà ingérée, exposée).
  - [x] **météo** (ADR-0012/0018) : `GET /v1/weather` (+ `/weather/date`), vent 100 m + irradiance (Open-Meteo CC-BY 4.0, attribué).
  - [x] **dérivation renouvelable** (ADR-0018) : `RenewableModel` + backtest (×2,4/×3,4) ; `GET /v1/renewable` (production estimée + facteur de charge, auto-calibré au démarrage). Prévision météo-pilotée **écartée** (gate `analyze-renewable-signal`).
  - [x] **déploiement** (ADR-0007) : **live** sur VPS Hetzner Kovelt (Traefik + PostgreSQL dédié), historique backfillé, backups quotidiens. Cf. mémoire de session.
  - [x] **SDK TypeScript** (`@carbon-fr/sdk`, `sdk/typescript/`) : tous les endpoints `/v1` + SSE, zéro dépendance runtime, CI typecheck/build. (Inclut `price()`/`priceHistory()`/`costReference()`.)
  - [x] **audit profond pré-launch** : durcissement sécurité (IP non spoofable, sel obligatoire en prod), robustesse (démarrage borné, séries denses plafonnées, pool, migration idempotente), contrat (OpenAPI/Bruno), perf.
  - [x] **prix de l'électricité** (ADR-0023) : `GET /v1/price` (+ `/price/date`) — décomposition TRV (énergie spot ENTSO-E A44 + TURPE + accise + TVA + résidu), contexte mix + techno marginale estimée. Domaine pur (`price.rs`), ports `SpotPriceSource`/`SpotPriceRepository`, table `spot_price` (migration `0011`, validée base réelle), ingestion poller (token ENTSO-E), métrique fraîcheur `last_price`. **Valeurs réglementaires 2026 sourcées** (CRE délib. 2025-78 TURPE 7 / 2026-06 TRVE + BOFiP) ; seul l'acheminement €/MWh est une conversion profil-dépendante (≈ 78, plage 53–116).
  - [x] **couche comparative LCOE** (ADR-0024) : `GET /v1/cost-reference` — fourchette d'estimations par filière (`cost.rs`), nucléaire scindé existant/nouveau, périmètre `plateau` explicité, note neutre, jamais d'écart calculé. **GATE de neutralité franchi** par évaluation adversariale multi-agents (pro/anti-nucléaire + audits), revue datée `docs/adr/0024-revue-neutralite.md` (+ addendum re-jeu n°3). **Licences confirmées** (recherche 2026-06-20) + **multi-sources** atteint (ADEME+IRENA, CdC+CRE ; neuf mono-source RTE faute de 2e source licence-compatible) ; champs `geography`/`technology_source_count` exposés. Reste (gouvernance) : 2e source pour le neuf ; contre-source France EnR ; confirmation écrite RTE avant palier payant sur sa donnée.

### Repères d'implémentation (phases 1-2)

- **`rte-direct` = national-only** (taux_co2 publié seulement au national, addendum ADR-0003). Le **régional** est servi en **`acv-ademe`** : `latest`/`range` de l'adapter ODRÉ, pour une région, lisent le mix régional (`eco2mix-regional-tr`, refine `code_insee_region`) et dérivent l'intensité. ⚠️ `pompage` y est typé **chaîne** (`"0"`) → non décodé.
- **Millésime stocké en rang `SMALLINT`** (0/1/2) côté Postgres → upsert conditionnel = `WHERE EXCLUDED.vintage_rank >= measurement.vintage_rank`. Mix = 10 colonnes (pas de `serde` dans le `core`).
- **`upsert_many` = INSERT multi-lignes** (`QueryBuilder`, paquets de 1000) + **dédup par clé** (`dedup_by_key`, garde le meilleur millésime) — obligatoire pour le volume du backfill (~494k lignes).
- **Backfill** : port `Eco2mixArchive` (export de masse, dataset `eco2mix-national-cons-def`), cas d'usage `BackfillHistory` qui **découpe en tranches** (une tranche = un export, pas l'API paginée — ADR-0003). Jamais de backfill via `range()` (plafonné).
- **Rollups** : `measurement_rollup_{hourly,daily}`, seaux `date_trunc(..., 'UTC')`. **D'abord** des vues matérialisées (migration `0002`, index unique requis par `REFRESH … CONCURRENTLY`), **désormais de vraies tables incrémentales** upsertées par seau (migration `0010`, lecture inchangée). Le **résumé** `/v1/intensity/stats` est exact (agrégat sur `measurement`) ; la **série** (`interval=`) vient des rollups. Rafraîchies par le poller (si `written > 0`) et en fin de backfill.
- **`acv-ademe`** : facteurs ACV versionnés en **constante de domaine** (`EmissionFactors::acv_ademe_v1`, ADR-0008), calcul pur `acv_ademe_intensity` + `derive_acv_ademe`. Dérivée et **stockée à l'ingestion** (poller + backfill) au même horodatage/millésime ; servie via `?methodology=acv-ademe`. **National + 12 régions** (le mix régional agrège le fossile en `thermique` → `GenerationMix.thermique: Option`, facteur gaz). **Basée production** : pour une région importatrice, reflète la production locale, pas la conso (imports = `acv-ademe@2`).
- **Partitionnement mensuel + BRIN** (ADR-0004) : toujours reporté (table simple, cf. commentaire de la migration `0001`). À reconsidérer maintenant que l'historique complet est ingérable.
- **OpenAPI code-first** (`utoipa`) : `ToSchema` sur les **DTO de l'adapter HTTP** uniquement (jamais le `core`), `#[utoipa::path]` sur les handlers (fonctionne malgré la généricité), `ApiDoc` dans `carbonfr_openapi.rs` → `/v1/openapi.json` + Swagger UI `/docs`. Collection **Bruno** dans `bruno/` (cf. [[dx-openapi-bruno]]).
- **Compteur de visiteurs** : port `VisitCounter` (`/v1/stats`, `POST /v1/stats/visit`). **IP jamais stockée** — empreinte SHA-256 salée (`CARBONFR_VISIT_SALT`, défaut `carbon-fr` à surcharger en prod), dédup `(ip_hash, jour)`. IP lue via `X-Forwarded-For`/`X-Real-IP` (derrière le proxy ; pas de `ConnectInfo` car `Option<ConnectInfo>` n'est pas un extracteur axum 0.8).
- **sqlx en requêtes runtime** (pas les macros `query!`) → `cargo check` reste hermétique, sans base.
- Tests : `core`/adapters hermétiques ; intégration Postgres pilotée par `DATABASE_URL` ; ODRÉ « live » en `--ignored`. ⚠️ postgres-alpine se relance pendant son init → attendre une vraie requête SQL stable avant de lancer les tests (pas seulement `pg_isready`).
- Serveur configurable par env : `DATABASE_URL`, `CARBONFR_BIND` (déf. `0.0.0.0:8080`), `CARBONFR_POLL_SECS` (déf. 900), `CARBONFR_BACKFILL_FROM`/`_TO`/`_WINDOW_DAYS` (déf. `2012-01-01`→maintenant, 90 j), `RUST_LOG`.
