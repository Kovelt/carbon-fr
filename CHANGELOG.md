# Changelog

Tous les changements notables de ce projet sont consignés dans ce fichier.

Le format s'inspire de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/),
et le projet suit le [versionnage sémantique](https://semver.org/lang/fr/). En
phase `0.x`, des ruptures d'API peuvent survenir en *minor* (cf. GOUVERNANCE §6).

## [Non publié]

## [0.4.0] - 2026-06-21

Nouvelle fonctionnalité **couche A « électrolyseur »** (éligibilité carbon-aware
RFNBO / bas-carbone) ; **gouvernance** du dépôt durcie (verrouillage de `main`,
ADR-0027) ; documentation alignée sur l'état réel de l'API.

### Ajouté

- **Couche A « électrolyseur » — éligibilité carbon-aware** (ADR-0025/0026) : overlay
  d'**éligibilité au niveau réseau** par créneau, sous deux cadres neutres et versionnés —
  `rfnbo` (renouvelable, Règl. délégués UE 2023/1184-1185) et `low-carbon` (bas-carbone
  inclusif nucléaire/CCS, acte délégué 2025/2359) — exposé en **extension rétro-compatible**
  de `GET /v1/intensity/greenest-window` (`?eligibility=rfnbo|low-carbon`, axe **orthogonal**
  à `methodology`) + catalogue `GET /v1/eligibility/rulesets`. Nouveau crate domaine **pur**
  `carbonfr-eligibility` (zéro IO). `rfnbo` = *disjonction* (part renouvelable instantanée
  ≥ 0,90 **OU** prix day-ahead ≤ 20 €/MWh — proxies explicitement étiquetés) ; `low-carbon` =
  intensité ≤ seuil **dérivé** `round(3384/53) ≈ 64 gCO₂eq/kWh` (proxy `indicative`,
  `indeterminate` si le seuil tombe dans l'intervalle de prévision). Zone de dépôt toujours
  `FR` ; prix jamais extrapolé au-delà du day-ahead. **SDK TypeScript** + Bruno mis à jour.
  Hors périmètre (disclaimer de neutralité) : gCO₂eq/kgH₂, certification, additionnalité PPA.

### Documentation

- **ADR-0025 — extension hydrogène carbon-aware** (couche A « électrolyseur ») : ADR de
  cadrage + brief d'implémentation intégrés (renumérotés depuis « 0015 », déjà pris ;
  cross-refs réalignées). *Documentation seule* — l'implémentation est livrée à part (cf.
  *Ajouté* ci-dessus).
- **Audit & mise à jour exhaustive de la documentation** vers l'état réel de l'API (v0.3.2,
  contrat `/v1`) : `ARCHITECTURE.md` (5 → 9 crates + `bin/server`, 11 ports réels, roadmap =
  5 phases livrées, déploiement Traefik/GHCR, rollups incrémentaux, §Sources complétée) ;
  `README.md` (corrige l'affirmation « tous les endpoints acceptent `?region=`/`?methodology=` »,
  ajoute tier hébergé/clés API, états 503/404, sous-commandes) ; addenda datés sur plusieurs
  ADR ; `CLAUDE.md`, `GOUVERNANCE.md`, READMEs `deploy`/`bruno`/`sdk`, `.env.example`.
- **ADR-0024 — contre-source France (renouvelables) recherchée puis écartée** : une 2ᵉ source
  française (Cour des comptes EnR mars 2026 + appels d'offres CRE) a été recherchée et vérifiée
  pour réduire l'asymétrie géographique, mais le **GATE de neutralité (re-jeu n°4) est revenu
  ROUGE** (non-commensurabilité grande/petite hydro ; test aveugle : enrichir les seuls
  renouvelables rend la famille devinable, le rééquilibrage nucléaire étant bloqué par les
  licences NC). Au titre du **Principe 0** d'ADR-0024 (« si la neutralité n'est pas garantie,
  ne pas livrer »), le changement est **annulé** ; l'état GREEN (v0.3.2) est conservé.
  `cost.rs` inchangé. Trace : ADR-0024 §1 + addendum re-jeu n°4.

### Gouvernance

- **Politique de contribution & verrouillage de `main`** (ADR-0027) : `main` est
  désormais protégée par un **ruleset GitHub** appliqué (Phase A — solo) — PR
  obligatoire (zéro push direct), **5 status checks stricts** requis (`fmt + clippy`,
  `cargo-deny (licences + advisories)`, `tests (avec PostgreSQL)`, `build release`,
  `SDK TypeScript`), conversations résolues, historique linéaire (squash/rebase),
  force-push & suppression de `main` interdits, `bypass_actors` **vide** (zéro
  exception, mainteneur admin compris). État déclaratif versionné dans
  [`.github/ruleset-main-phaseA.json`](.github/ruleset-main-phaseA.json). Ajout de
  `.github/CODEOWNERS` (inerte en Phase A, prêt pour la Phase B) et des gabarits
  **PR / issue** ; `GOUVERNANCE.md`, `CONTRIBUTING.md` et `README.md` alignés. La
  **Phase B** (1 approbation + revue Code Owners) s'activera à la première
  contribution externe (checklist dans l'ADR).

## [0.3.2] - 2026-06-20

### Ajouté

- **`/v1/cost-reference` — dispersion inter-sources (multi-sources)**. 2e source par
  filière : **IRENA** (LCOE mondiaux 2024) pour les 5 renouvelables, **CRE** pour le
  nucléaire existant — la fourchette mêle désormais dispersion intra-source ET
  inter-sources. Le **nucléaire nouveau reste mono-source** (RTE) faute de 2e source
  primaire licence-compatible (IPCC/NEA/IEA écartés pour clause NC). Deux nouveaux
  champs par entrée : `geography` (`france`/`monde` — IRENA est mondial, souvent plus
  bas que la France) et `technology_source_count` (≥ 2 = multi-sources). **GATE de
  neutralité re-joué (n°3) : GREEN** (asymétrie du neuf jugée *content-blind* —
  IRENA, la source la plus pro-EnR, est incluse ; IEA/NEA pro-pilotable exclues).

### Modifié

- **`/v1/cost-reference` — licences confirmées** (recherche 2026-06-20, sources
  primaires). Pré-condition « licences CdC/RTE » de l'ADR-0024 **levée sous
  conditions**. L'attribution servie (`source_attribution`) est corrigée pour
  refléter le vrai fondement de réutilisation : ADEME = Licence Ouverte / Etalab 2.0 ;
  Cour des comptes = CRPA art. L321-1 + absence de clause non commerciale ; **RTE =
  non-protection des faits (CPI L112-1) + extraction non substantielle** — la mention
  antérieure « données RTE largement en Licence Ouverte » était **inexacte** (la
  valeur vient du rapport, aux mentions légales restrictives). Conditions : chiffres-
  faits uniquement, attribution nominative, lien externe ; **confirmation écrite RTE
  recommandée avant un palier payant** sur sa donnée. ADR-0024 (§5, §risques, statut)
  et revue de neutralité (§licences) mis à jour. *Best-effort, pas un avis juridique.*

## [0.3.1] - 2026-06-20

### Modifié

- **`/v1/price` — valeurs réglementaires 2026 sourcées** (remplacent les
  placeholders best-effort de la 0.3.0). `TrvReference::trv_2026` :
  accise **30,85 €/MWh** (CRE délib. TRVE 2026 n°2026-06 + BOFiP `BOI-RES-EAT-000240`),
  TVA **20 %** unique (BOFiP `ACTU-2025-00057` ; le taux réduit 5,5 % a été supprimé),
  commercialisation **18,11 €/MWh HT** (CRE délib. n°2026-06), acheminement **≈ 78 €/MWh**
  dérivé du **TURPE 7** (CRE délib. n°2025-78) pour un profil 6 kVA / ~2 400 kWh/an.
  L'acheminement en €/MWh reste une conversion profil-dépendante (plage 53–116) ;
  TURPE +3,04 % au 1/8/2026 et accise possiblement réindexée au 2e semestre → à re-millésimer.

## [0.3.0] - 2026-06-20

### Ajouté

- **Prix de l'électricité** (ADR-0023) : `GET /v1/price` et `GET /v1/price/date` —
  décomposition complète du prix payé ancrée sur le **TRV**. Composante énergie =
  **prix spot day-ahead ENTSO-E** (`documentType=A44`) ; + acheminement (TURPE) +
  accise + TVA + résidu commercialisation (constantes de domaine versionnées,
  best-effort 2026 *à sourcer*) ; contexte : mix par filière + technologie
  marginale **estimée**. National. Table `spot_price` (migration `0011`), ingérée
  par le poller si `CARBONFR_ENTSOE_TOKEN`. SDK : `price()` / `priceHistory()`.
- **Couche comparative LCOE** (ADR-0024) : `GET /v1/cost-reference` — coût de
  production par filière en **fourchette** (estimation), nucléaire scindé
  existant/nouveau, **jamais** mis en différence avec le prix de marché. *GATE de
  neutralité* franchi par évaluation adversariale (revue datée
  `docs/adr/0024-revue-neutralite.md`). SDK : `costReference()`. Reste, avant
  publication ferme : confirmer les licences CdC/RTE, multi-source par filière.

## [0.2.1] - 2026-06-17

Aucun changement fonctionnel du service (image identique à 0.2.0 côté binaire).

### CI

- **Release automatisée** : `release.yml` crée désormais la **GitHub Release** au
  push du tag (notes extraites de la section CHANGELOG correspondante), en plus de
  publier l'image GHCR — tag, image et Release restent alignés en une opération.

## [0.2.0] - 2026-06-17

Durcissement de maintenabilité d'API publique : contrat verrouillé, erreurs
standardisées, observabilité et gouvernance de sécurité.

### Ajouté

- **Garde-fou de contrat OpenAPI** : un instantané commité (`openapi.snapshot.json`)
  est comparé en CI au document généré ; toute évolution du contrat `/v1` devient un
  acte volontaire visible dans le diff (ADR-0019).
- **`SECURITY.md`** : politique de signalement de faille (privé via GitHub).
- **Politique de dépréciation** (ADR-0020) : cycle de vie public Actif → Déprécié →
  Retiré, annonce via en-têtes `Deprecation` (RFC 9745) + `Sunset` (RFC 8594), fenêtre
  de retrait ≥ 6 mois (post-1.0) / ≥ 30 j (pré-1.0).
- **`.github/dependabot.yml`** : mises à jour de dépendances (cargo, npm SDK, actions).
- **Observabilité** (ADR-0022) : endpoint `GET /metrics` (format Prometheus, hors `/v1`) —
  fraîcheur du poller, volume/erreurs d'ingestion, appels amont par source (proxy de
  quota), `build_info`. Registre maison, zéro dépendance.

### Modifié

- **Format d'erreur → Problem Details (RFC 9457)** (ADR-0021) : les réponses d'erreur
  passent de `{error, message}` (`application/json`) à `application/problem+json`
  (`type`/`title`/`status`/`detail` + extension **`code`** stable). **Rupture** de
  contrat assumée pré-1.0. Le **SDK** (`@carbon-fr/sdk`) est mis à jour en conséquence
  (`CarbonFrError.code`/`.message`, `ProblemDetails`).

## [0.1.0] - 2026-06-17

Première release publique. Image de production sur GHCR
(`ghcr.io/kovelt/carbon-fr:0.1.0`), déployée sur VPS FR/EU.

### Performance & infra (audit, lot 5)

- **Rollups incrémentaux** (migration `0010`) : les vues matérialisées (rafraîchies
  en entier à chaque cycle — coût O(table) croissant) deviennent de **vraies tables**
  upsertées **par seau touché**. Le poller (`refresh_rollups`) ne réagrège que la
  **fenêtre récente** (7 j) ; le backfill (`rebuild_rollups`) reconstruit tout. Lecture
  `rollup()` inchangée. **Validé sur Postgres réel** (17 tests d'intégration, dont le
  chemin incrémental). Supprime le coût croissant du « partitionnement reporté ».
- **Dockerfile** : cache de build (cache mounts BuildKit pour le registre Cargo et
  `target/`) → recompilations rapides ; note sur l'épinglage par digest.
- **`deploy/README.md`** : clarifie les deux voies (self-hosting Caddy/systemd vs
  prod Traefik d'org), avec les labels Traefik et le rappel `CARBONFR_TRUST_PROXY=1`.

### Contrat & documentation (audit, lot 4)

- **OpenAPI** : ajout du schéma `StreamEventBody` (charge utile du flux SSE,
  jusque-là absent de la spec) + test des schémas étendu (anti-régression).
- **Bruno** : ajout des requêtes manquantes (`/v1/stats`, `/stats/visit`,
  `GET`/`DELETE /v1/webhooks`, `/health/ready`, cas `?version=` invalide).
- **Doc à jour** : CLAUDE.md « État d'avancement » (Phase 5 : échanges, météo,
  renouvelable, déploiement, SDK, audit) + liste des ADR (0017/0018) ; index ADR ;
  ADR-0010 §6 corrigé (acv-ademe@2 dérivé en mémoire, pas de rollup matérialisé) ;
  README roadmap ; `.env.example` (vars ENTSO-E/calibration manquantes) ; tableau
  des variables d'env (`CARBONFR_LOG_FORMAT`) ; message CLI des sous-commandes.

### Sécurité (audit, lot 3)

- **IP client non spoofable derrière proxy** : on lit désormais `X-Real-Ip` (posé
  par le reverse proxy de confiance), sinon le **dernier** segment de
  `X-Forwarded-For` (le proxy ajoute l'IP réelle à droite ; les segments de gauche
  sont fournis par le client). Corrige le contournement du quota anonyme et la
  pollution du compteur de visiteurs via un XFF forgé.
- **Sel visiteur obligatoire en production** : le serveur **refuse de démarrer**
  si `CARBONFR_VISIT_SALT` est absent **et** `CARBONFR_TRUST_PROXY=1` (= derrière
  un proxy = prod) — un sel public rendrait les empreintes d'IP réversibles. En
  dev/self-hosting direct, simple avertissement (parité préservée).
- **Quota d'abonnements webhook par clé** (max 50) : borne le stockage et
  l'amplification de livraisons sortantes.

### Robustesse runtime & données (audit, lot 2)

- **Démarrage borné en temps** : les 3 calibrations au démarrage (prévision,
  acv-ademe@2, renouvelable) sont désormais sous **timeout** (120 s) → repli sur
  non-calibré plutôt que de pendre si la base est lente (gros historique, REFRESH
  concurrent, pool saturé).
- **Séries denses bornées** : `/v1/exchanges/date` et `/v1/weather/date` plafonnés
  à **92 jours** (au lieu de 366) — ~576 lignes/jour (échanges) ou multi-runs
  horaires (météo) gonflaient une réponse non paginée.
- **Migration `0002` idempotente** (`CREATE MATERIALIZED VIEW/INDEX IF NOT EXISTS`).
- **Pool PostgreSQL** : défaut 10 → **20** (partagé API + poller + watcher ; un
  `REFRESH … CONCURRENTLY` monopolise une connexion).

### Ajouté — SDK TypeScript (`@carbon-fr/sdk`)

- Client **TypeScript** ([`sdk/typescript/`](sdk/typescript/)) couvrant tous les
  endpoints `/v1` : typé de bout en bout (une méthode + un type par endpoint),
  **zéro dépendance runtime** (`fetch` natif — navigateur, Node ≥ 18, Deno, Bun),
  flux **SSE** exposé en `AsyncGenerator`, erreurs `CarbonFrError` (`status`/`code`).
  Job CI `sdk-typescript` (typecheck + build) ajouté.

### Mesuré & écarté — prévision météo-pilotée (ADR-0018 étape A)

- **`AnalyzeRenewableSignal`** + sous-commande **`analyze-renewable-signal`** :
  mesure (borne supérieure, renouvelable réel, hors échantillon) si l'anomalie de
  renouvelable améliore la climatologie d'intensité. **Mesuré (2024, national)** :
  gain **0,48 gCO₂eq/kWh (~4 %)**, β ≈ 0. L'outil est validé par tests (détecte un
  signal synthétique, donne β≈0 sans lien). **Conclusion** : le réseau FR
  (nucléaire-dominé, déjà bas carbone) ne tire **pas** de gain notable d'une
  prévision d'intensité météo-pilotée → `forecast@N` **non construit** (même
  discipline que l'ajustement de charge ADR-0011 §4 et le GBDT ADR-0012). La
  dérivation reste précieuse comme **produit** (`/v1/renewable`), pas comme levier
  de précision de prévision.

### Ajouté — exposition de la dérivation renouvelable (ADR-0018)

- **`GET /v1/renewable`** : production renouvelable **estimée** depuis la météo
  courante (éolien/solaire en MW) + **facteur de charge** (0–1, part de la
  capacité installée réalisée), avec les capacités effectives calibrées
  (transparence). Le *moat* rendu visible : « given le vent/soleil actuels, voici
  la production attendue ». Modèle **auto-calibré au démarrage** sur l'historique
  récent (`CARBONFR_RENEWABLE_CALIBRATE_WEEKS`, défaut 52) ; `503` si non calibré.
  Valeurs **modélisées, non mesurées** (champ `source`, attribution Open-Meteo
  CC-BY 4.0). Cas d'usage pur `CalibrateRenewable`. OpenAPI + Bruno.

### Ajouté — météo nationale (ADR-0012/0018)

- **`GET /v1/weather`** (courante) et **`GET /v1/weather/date?from=&to=`**
  (historique depuis 2016) : vent à 100 m (km/h) + irradiance (W/m²), moyenne
  nationale 7 points. Donnée déjà ingérée (substrat de la dérivation
  renouvelable), exposée telle quelle. **Attribution Open-Meteo (CC-BY 4.0)**
  portée dans le champ `source` (crédit + lien + mention de transformation),
  comme l'exige la licence. OpenAPI + Bruno. *(Note gouvernance : l'API gratuite
  Open-Meteo est non-commerciale ; un tier hébergé payant nécessitera un
  abonnement Open-Meteo pour l'ingestion.)*

### Ajouté — dérivation renouvelable, fondation (ADR-0018)

- **Calculateur de domaine pur `RenewableModel`** : météo (vent à 100 m,
  irradiance) → production **éolien/solaire estimée** (MW). Courbe de puissance
  éolienne agrégée (sigmoïde) + modèle PV linéaire ; capacités effectives
  **calibrées par moindres carrés** sur l'historique (`calibrate_renewable`).
- **Backtest `BacktestRenewable`** + sous-commande **`backtest-renewable`** :
  calibration 70 % / test 30 % hors échantillon, vs baseline « moyenne ».
  **Mesuré (2024 S1, national)** : la météo bat le baseline **×2,4 (éolien)** et
  **×3,4 (solaire)** au RMSE ; les capacités calibrées (~22 GW éolien, ~18 GW
  solaire) **retrouvent le parc réellement installé** — dérivation physiquement
  juste. Fondation du *moat* ; exposition (prévision, attribution carbone) à venir.

### Ajouté — échanges transfrontaliers (ADR-0017)

- **`GET /v1/exchanges`** : expose les échanges transfrontaliers par frontière
  (flux net signé FR↔voisin, `> 0` = import vers la France) et l'**intensité
  carbone de chaque voisin** (cycle de vie ADEME), au pas quart d'heure. La
  donnée ENTSO-E était déjà ingérée pour `acv-ademe@2` ; l'endpoint la **sert**
  sans nouvelle ingestion (cas d'usage pur `GetCrossBorderExchanges`, projection
  de lecture). Solde net + totaux import/export + détail par pays. `gb`
  indisponible côté ENTSO-E (Brexit) → absent. OpenAPI + collection Bruno.
- **`GET /v1/exchanges/date?from=&to=`** : série historique des échanges
  (fenêtre ≤ 366 j, pas quart d'heure), même DTO par snapshot.

### Sécurité & robustesse (durcissement pré-déploiement, audit)

- **SSRF webhooks — faille TOCTOU corrigée** : la livraison utilise désormais un
  **resolver DNS custom** interne à reqwest (`PublicOnlyResolver`) qui filtre les
  IP non publiques *au moment où reqwest résout l'hôte* — l'IP contactée est
  exactement celle validée, éliminant le DNS rebinding (l'ancienne « valider puis
  laisser reqwest re-résoudre » était contournable). Redirections refusées,
  `no_proxy`, `connect_timeout`. **Deny-list SSRF complétée** : `0.0.0.0/8`,
  `240/4`, 6to4 `2002::/16`, Teredo `2001::/32`, NAT64 `64:ff9b::/96`.
- **Timeouts sur les clients amont** (ODRÉ, Open-Meteo, ENTSO-E) : sans eux, une
  source qui *pend* gelait l'ingestion indéfiniment. `connect_timeout`/`timeout`.
- **`X-Forwarded-For` non cru par défaut** (`CARBONFR_TRUST_PROXY`, défaut off) :
  sans proxy de confiance l'en-tête est spoofable (contournement du quota anonyme,
  pollution du compteur visiteurs) → ignoré par défaut. À activer derrière le
  reverse proxy de prod.
- **Sel visiteur** : avertissement au démarrage si `CARBONFR_VISIT_SALT` absent
  (le défaut public rendrait les empreintes d'IP réversibles).
- **Supervision des tâches de fond** : le poller/watcher étaient des `spawn` non
  surveillés (panique = mort silencieuse). Supervision **fail-fast** (`select!`)
  → le process s'arrête en erreur si une tâche critique meurt (relance superviseur).
- **Arrêt gracieux sur SIGTERM** (en plus de SIGINT) — signal d'arrêt orchestré.
- **Pool PostgreSQL** : `max_connections` configurable (défaut 10, était 5),
  `acquire_timeout` (échec rapide sous saturation au lieu de pendre), recyclage
  (`idle`/`max_lifetime`).
- **Readiness** : `GET /health/ready` vérifie l'accès à la base (`503` si
  injoignable), distinct de `/health` (liveness). **Retry de connexion DB au boot**.

### Exploitation & contrat d'API (durcissement pré-déploiement, suite)

- **Packaging de production** : `Dockerfile` multi-stage (build `--release
  --locked`, runtime Debian slim, utilisateur **non-root** uid 10001), unité
  **systemd** (`deploy/carbonfr.service`, `Restart=on-failure`, durcissement),
  **Caddyfile** (reverse proxy TLS + en-têtes de sécurité, sonde `/health/ready`),
  `.env.example` documenté. `Cargo.lock` désormais **versionné** (binaire reproductible).
- **Profil release optimisé** (`lto = "thin"`, `codegen-units = 1`,
  `strip = "debuginfo"`) — binaire plus petit et plus rapide.
- **CI** : job **`build-release`** (garantit que le binaire de prod compile et que
  le lockfile est cohérent, `--locked`) + **scan d'advisories quotidien** (cron) —
  une CVE publiée hors fenêtre de PR serait sinon invisible.
- **Observabilité** : `TraceLayer` (tracing par requête) + **logs JSON** optionnels
  (`CARBONFR_LOG_FORMAT=json`) pour l'agrégation en prod.
- **Contrat d'API durci** : `?version=` **inconnue rejetée en 400** (au lieu d'être
  silencieusement ignorée) ; seuil `NaN`/infini rejeté sur `/v1/intensity/below` ;
  **limite de taille du corps** (16 Kio) ; `callback_url` de webhook plafonnée (2048).
- **Robustesse webhooks** : payload JSON centralisé et **échappé**
  (`render_webhook_payload`), **concurrence de livraison bornée** (sémaphore), état
  « précédent » mémorisé **après** lecture réussie de la base (pas de transition ratée).
- **Fuite de DSN évitée** : l'erreur de connexion PostgreSQL ne ré-expose plus la
  chaîne de connexion (mot de passe) dans le message remonté.

### Ajouté

- **Socle hexagonal** : crate `core` (domaine, cas d'usage, ports, sans IO),
  adapters `odre` (ODRÉ/éCO2mix), `postgres` (PostgreSQL natif) et `http`
  (axum), et binaire `carbonfr-server` (composition root + poller unique).
- **API `/v1`** (couverture nationale) :
  - `GET /v1/intensity/now` — dernière intensité carbone (gCO₂eq/kWh) ;
  - `GET /v1/mix` — mix de production par filière (MW) ;
  - `GET /v1/intensity/date?from=&to=` — série historique sur un intervalle ;
  - `GET /v1/intensity/stats?from=&to=[&interval=hour|day]` — résumé
    (moyenne/min/max) et série agrégée depuis les rollups ;
  - `GET /health` — sonde de disponibilité.
- **Backfill historique** national par export de masse ODRÉ
  (`carbonfr-server backfill`), upsert conditionnel au millésime.
- **Rollups** : vues matérialisées horaires et journalières, rafraîchies par le
  poller et le backfill.
- **Méthodologie `acv-ademe@1`** (cycle de vie ADEME, basée production, ADR-0008)
  coexistant avec `rte-direct` : dérivée et stockée à l'ingestion, sélectionnable
  via `?methodology=` sur les endpoints `/v1`.
- **Couverture régionale** (12 régions métropolitaines) : le poller ingère le
  mix régional (éCO2mix régional, `thermique` agrégé) et en dérive l'intensité
  `acv-ademe`. `rte-direct` reste national (taux_co2 publié par RTE).
- **OpenAPI 3.1** dérivée du code (`utoipa`) sous `GET /v1/openapi.json` +
  **Swagger UI** sous `GET /docs`.
- **Collection Bruno** versionnée (`bruno/`) couvrant tous les endpoints
  (cas nominaux national/régional × `rte-direct`/`acv-ademe`, et erreurs 400/404).
- **Prévision d'intensité** (phase 3, ADR-0009) : modèle `climatology@1`
  (climatologie horaire-de-semaine glissante + correction de persistance
  décroissante), fonction de domaine pure + adapter `ClimatologyForecaster`
  (alimenté par l'historique stocké). Exposée sous
  `GET /v1/intensity/forecast?from=&horizon_hours=` (série prévue) et
  `GET /v1/intensity/greenest-window?from=&horizon_hours=&window_minutes=`
  (créneau le plus bas-carbone). Prévisions **non persistées** (calculées à la
  lecture) ; l'identité du modèle est exposée dans chaque réponse.
- **Contrat de prévision `ForecastPoint`** (ADR-0011) : type domaine dédié avec
  **intervalle d'incertitude** (`expected`/`lower`/`upper`), `ModelVersion` et
  **sans `vintage`** — remplace le `Vec<Measurement>` du port `ForecastModel`.
  `GET /v1/intensity/forecast` expose l'intervalle ; `greenest-window` gagne un
  sélecteur `?estimator=central|prudent`.
- **Intervalles par quantiles de résidus par horizon** (ADR-0011 §5) : type
  `HorizonBands` calibré par backtest *walk-forward* (`backtest-bands`) ; les
  bornes **s'élargissent avec l'horizon**. Le serveur auto-calibre au démarrage
  (`CARBONFR_FORECAST_CALIBRATE_WEEKS`), avec repli sur la dispersion par créneau.
- **Framework de prévision ML GBDT** (ADR-0012, tranche 2a) : crate
  `carbonfr-adapter-gbdt` (`gbdt` pur Rust) — *feature engineering* partagé
  train/inférence (anti-fuite), `train_model`, `GbdtForecaster` (artefact
  versionné chargé par chemin), sous-commande `carbonfr-server train`
  (entraîne → sauve → compare `gbdt@1` vs `climatology@1` au backtest).
  *Mesuré* : sans features météo, le GBDT **ne bat pas** la climatologie calibrée
  (attendu — la météo est le levier) ; `climatology@1` **reste servi**.
- **Backfill météo historique + features météo/climatologie** (ADR-0012,
  tranche 2b) : archive des prévisions Open-Meteo (anti-fuite `run_at`), features
  vent/irradiance *as-of* + climatologie de créneau (apprentissage résiduel),
  calcul identique train/inférence. *Mesuré* : `gbdt@1` ne bat **toujours pas**
  `climatology@1` (~2× pire), même entraîné sur l'année entière → baseline
  calibrée difficile ; `@1` reste servi. Correctif : dédup `(region, at)` dans
  l'upsert de charge.
- **Store de prévision météo** (ADR-0012, tranche 1 du modèle ML) : port
  `WeatherForecastSource` + adapter `carbonfr-adapter-meteo` (Open-Meteo, vent à
  100 m + irradiance, agrégés sur 7 points de métropole), store
  `WeatherRepository` (table `weather_forecast`) **daté `(run_at, valid_at)`**
  pour l'anti-fuite, ingéré par le poller. Entrée du futur `GbdtForecaster`.
- **Store de charge** (consommation réalisée + prévue RTE) : table `consumption`,
  ports `ConsumptionRepository`/`ConsumptionSource`, ingestion par le poller
  (conso récente + prévisions J-1/J) et backfill de la réalisée. Entrée
  réutilisable pour le futur modèle ML (ADR-0012). *Note* : l'ajustement
  **linéaire** de la prévision par la charge (ADR-0011 §4) a été essayé puis
  **écarté** — mesuré moins bon que la climatologie seule (cf. ADR-0011).
- **Backtest** du modèle de prévision (`carbonfr-server backtest`, ADR-0009) :
  évaluation *walk-forward* sur l'historique, MAE/RMSE global et par horizon
  (h+1/h+6/h+24), comparés à une référence de persistance — pour mesurer la
  précision plutôt que la supposer. Mode `backtest-sweep` (balayage N × τ).
- **Calibration de `climatology@1`** (addendum ADR-0009) : défauts révisés
  `N = 10 semaines`, `τ = 2 semaines`, calés par backtest sur la donnée réelle
  2024 — le modèle bat désormais la persistance à tous les horizons (l'ancien
  `τ = 6 h` la sous-performait). Formule et contrat d'API inchangés.
- **Méthodologie `acv-ademe@2` consumption-based — domaine pur + vérifiabilité**
  (ADR-0010, tranche A) : trait de domaine `MethodologyCalculator`
  (`RteDirect` / `AcvAdemeProduction` / `AcvAdemeConsumption`), value object
  `CrossBorderFlows` (flux signés par voisin + intensité du voisin, enum
  `Neighbor`), calcul pur *consumption-based* (imports valorisés à l'intensité
  du voisin − exports + **pertes T&D**) — **sans IO**. `acv-ademe@2` est une
  version **distincte** de `@1` (production), qui reste publié (gouvernance
  ADR-0005). Deux endpoints de **vérifiabilité**, sans dépendance externe :
  `GET /v1/methodologies` (catalogue + versions) et `GET /v1/factors` (table des
  facteurs par filière + facteur de pertes T&D). *Le calcul de `@2` sera **servi**
  une fois la source d'import ENTSO-E branchée (tranche B) ; il apparaît `planned`
  dans `/v1/methodologies`.* Défaut de l'API inchangé : `rte-direct`.
- **Adapter ENTSO-E — contexte d'import transfrontalier** (ADR-0010, tranche B
  1/2) : port `CrossBorderSource` + value object horodaté `CrossBorderSnapshot`
  (domaine) et crate `carbonfr-adapter-entsoe`. Pour chaque frontière de la
  France métropolitaine : **flux physique net signé** (`documentType=A11`, import
  − export) et **intensité carbone du voisin** dérivée de sa génération par type
  (`documentType=A75`/`processType=A16`) via les **mêmes facteurs ADEME** que le
  domaine (mapping `PsrType` B01–B25 → filières, zones EIC). Token
  `CARBONFR_ENTSOE_TOKEN` ; jamais appelé par requête utilisateur. Parsing XML
  testé sur fixtures ; *chemins XML/codes calés sur le guide RESTful API ENTSO-E,
  **à valider contre l'API live** (`tests/live.rs`, `--ignored`).*
- **`acv-ademe@2` servie : store + ingestion + lecture** (ADR-0010, tranche B
  2/2) : port + store Postgres `CrossBorderRepository` (table `cross_border_flow`,
  migration `0007`, testé sur Postgres réel) ; le poller ingère le contexte
  d'import à chaque cycle **si `CARBONFR_ENTSOE_TOKEN` est défini** (source
  optionnelle, non bloquante) ; cas d'usage `GetConsumptionIntensity` (calcul
  **à la lecture**, sans stockage de ligne `@2`) exposé via
  **`GET /v1/intensity/now?methodology=acv-ademe&version=2`** (national).
  `acv-ademe@2` passe `served` dans `/v1/methodologies`. Défaut de l'API inchangé
  (`rte-direct`) ; sans token, le calcul renvoie `404` faute de contexte d'import.
- **`acv-ademe@2` sur l'historique et les stats** (ADR-0010 §6) : la méthode
  consommation est servie **à la lecture** au-delà de `/now`, via
  `GET /v1/intensity/date` et `GET /v1/intensity/stats`
  (`?methodology=acv-ademe&version=2`, national). Port
  `CrossBorderRepository::flows_range`, fonction pure `derive_consumption_series`
  (jointure mix × contexte d'import le plus proche), agrégats `summarize`/
  `bucketize` calculés dans le domaine (la série `@2` n'est pas matérialisée).
  `@2` n'existe que là où le contexte d'import a été ingéré.
- **Webhooks — fondation de sécurité** (ADR-0016, tranche A) : tout le code
  **dangereux** posé d'abord, **pur et testé à froid** dans `core` — déclenchement
  **edge-triggered** (`should_fire` : notifie au *franchissement* de seuil, pas à
  chaque cycle), validation **anti-SSRF** de l'URL de rappel (`validate_webhook_url`
  : HTTPS only + deny-list des IP privées/loopback/link-local/réservées, IPv4 et
  IPv6), **signature HMAC-SHA256** tout-Rust (`hmac_sha256_hex`, sans nouvelle
  dépendance, **validée contre les vecteurs RFC 4231**), modèle `Subscription`.
  Ports `SubscriptionRepository` et `Notifier`. Débloqué par l'*ownership* des
  clés API.
- **Webhooks — store, livraison, watcher, endpoints** (ADR-0016, tranche B) :
  table `webhook_subscription` (CRUD **scopé au propriétaire**) ; crate
  `carbonfr-adapter-webhook` (`HttpNotifier`) qui **re-valide l'IP à la résolution
  DNS** (parade TOCTOU), **refuse les redirections** et **réessaie** à *backoff*
  borné ; **watcher** de fond branché sur le flux `IntensityUpdate` (détecte les
  franchissements, signe en HMAC, délègue la livraison) ; endpoints
  `POST`/`GET /v1/webhooks` et `DELETE /v1/webhooks/{id}` (**clé API requise**, le
  secret de signature n'est affiché qu'à la création).
- **Tier hébergé — clés API + quota au bord** (ADR-0015, tranche A) : middleware
  d'authentification (`Authorization: Bearer …`) et de **quota par minute**
  (`401` clé inconnue, `429` quota dépassé + en-têtes `RateLimit-*`/`Retry-After`),
  **opt-in** (`CARBONFR_RATELIMIT_ENABLED`, désactivé par défaut → l'API reste
  anonyme et sans limite, parité self-hosting). Port `ApiKeyRepository` + table
  `api_key` (empreinte SHA-256, **jamais la clé en clair**) ; sous-commande
  `carbonfr-server mint-key`. **`core` strictement intact** : aucun cas d'usage ne
  voit le principal — l'identité reste une préoccupation de bord. *(Métering
  persistant `UsageMeter` et webhooks à venir.)*
- **Prévision `acv-ademe@2` (consumption-based)** (ADR-0013, tranche A) : on
  prévoit les **entrées** (mix par filière + contexte d'import : flux et intensité
  de chaque voisin) par climatologie horaire-de-semaine + correction de
  persistance (formule `climatology@1`, par canal), puis on applique le **même**
  calculateur pur `AcvAdeme` (ADR-0010) — la prévision hérite de la version de
  méthode, reste **auditable** et **converge vers le nowcast** quand l'horizon → 0
  (invariant testé). Fonction domaine `acv_ademe_forecast`, adapter
  `AcvAdemeForecaster<R, C>`, **routage par méthode** au composition root, servi
  via `GET /v1/intensity/forecast?methodology=acv-ademe&version=2` (national).
  *Modèle `acv-clim@1`* ; baseline que le futur `MixForecaster` GBDT + ENTSO-E
  day-ahead devront battre (garde de promotion).
- **Backtest & calibration `acv-ademe@2`** (ADR-0013 §6-7) : cas d'usage
  `BacktestConsumptionForecast` — la vérité `@2` n'étant pas stockée, elle est
  **dérivée** de l'observé (mix + contexte d'import) puis comparée à la prévision
  en *walk-forward* (anti-fuite, vs persistance). Sous-commande
  `carbonfr-server backtest-acv` (MAE/RMSE global + par horizon). Intervalles
  `@2` **calibrés par quantiles de résidus par horizon** et **auto-calibrés au
  démarrage** du serveur (repli sur la dispersion par créneau).
- **Primitives de scheduling carbon-aware** (ADR-0014, tranche A) : fonctions
  **pures** du domaine (zéro nouveau port) sur la prévision, réutilisant le
  sélecteur `central`/`prudent` — créneau contigu le plus bas-carbone **avant une
  échéance**, **lowest-k** créneaux (job divisible), créneaux **sous un seuil**, et
  **annotation d'économie** vs « maintenant » (delta + %, et gCO₂eq absolus si
  l'énergie du job est fournie). Cas d'usage `CarbonAwareScheduler` + endpoints
  `GET /v1/schedule`, `GET /v1/schedule/slots`, `GET /v1/intensity/below`. Posture
  **anonyme/sans état** préservée ; ce sont des conseils sur prévision, **pas du
  pilotage**.
- **Flux live SSE** (ADR-0014, tranche B) : `GET /v1/intensity/stream`
  (`text/event-stream`) pousse un événement `intensity` à chaque mise à jour
  nationale du read-model (cadence du poller), avec filtres optionnels `region`
  et `below=X` et heartbeat keep-alive. Type domaine léger `IntensityUpdate`,
  diffusion par **canal mémoire `tokio::broadcast`** (poller intégré ; migration
  `LISTEN`/`NOTIFY` documentée pour un futur `bin/poller`). **Sans état
  par-client**, anonyme, auto-hébergeable.
- **Compteur de consultation** : `GET /v1/stats` + `POST /v1/stats/visit`
  (port `VisitCounter`). IP **jamais stockée** — empreinte SHA-256 salée
  (`CARBONFR_VISIT_SALT`), déduplication unique par IP/jour ; IP lue via
  `X-Forwarded-For`/`X-Real-IP`.
- **Documentation & gouvernance** : ADR 0001–0009 acceptés (+ addendum ADR-0003),
  ADR 0010–0015 **proposés** (vision forward : `acv-ademe` consumption-based,
  contrat `ForecastPoint`, modèle ML, prévision `acv-ademe`, usage/streaming,
  tier hébergé),
  `ARCHITECTURE.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `GOUVERNANCE.md`,
  et intégration continue GitHub Actions (fmt, clippy, tests + PostgreSQL).
- **Chaîne d'approvisionnement** : politique `cargo-deny` (`deny.toml`) vérifiée
  en CI — licences permissives en liste blanche (compatibles MIT/Apache-2.0),
  avis de sécurité RustSec, et sources de confiance.

### Notes

- `acv-ademe@1` est **basée production** : pour une région importatrice,
  l'intensité reflète la production locale, pas la consommation (imports =
  version consommation, `acv-ademe@2`).
- La prévision (`/forecast`, `/greenest-window`) relève de la phase 3.
