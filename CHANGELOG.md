# Changelog

Tous les changements notables de ce projet sont consignés dans ce fichier.

Le format s'inspire de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/),
et le projet suit le [versionnage sémantique](https://semver.org/lang/fr/). En
phase `0.x`, des ruptures d'API peuvent survenir en *minor* (cf. GOUVERNANCE §6).

## [Non publié]

### Ajouté

- **Expérience `share-meteo@2`** (addendum de l'[ADR-0028](docs/adr/0028-prevision-part-renouvelable-eligibilite.md)) —
  variante **météo-pilotée** de la part renouvelable prévue : dérivation par
  canal (éolien/solaire via le `RenewableModel` d'ADR-0018 calibré par origine,
  anti-fuite ; autres canaux en climatologie + anomalie d'ancre), **repli exact
  sur `share-clim@1`** hors couverture météo. Nouvelle sous-commande **dédiée**
  `backtest-share-meteo` : **comparaison à trois** (météo vs climatologie vs
  persistance, mêmes origines/cibles) — `backtest-share` reste le GATE de
  production de `share-clim@1`, inchangé. GO formel du gate météo sur les deux
  fenêtres de l'ADR-0028 (bat `share-clim@1` en RMSE global, 0 faux verdict),
  gain concentré à h+1/h+6 (−7,7 %/−6,6 % à h+1 ; parité par construction
  au-delà de la couverture d'archive 24 h). **Non servi** (décision du
  2026-07-04, documentée dans l'addendum) : `share-clim@1` reste le modèle en
  production ; à re-mesurer quand la couverture météo de service dépassera le
  cadre du backtest.

- **`Cache-Control: public, max-age=60` sur les lectures stables** (audit perf
  2026-08) — les `GET` dont la donnée ne change qu'au cycle du poller (~15 min)
  ou au démarrage (`/now`, `/mix`, `/forecast`, `/greenest-window`, `/price`,
  `/cost-reference`, catalogues…) n'annonçaient aucune politique de cache : un
  navigateur, un proxy ou le CDN d'une instance self-hostée re-frappait l'API à
  chaque polling. En-tête posé sur les seules réponses `200` de ces chemins —
  jamais sur le SSE (`no-cache` d'axum conservé), les endpoints à clé, le
  compteur de visiteurs ni les erreurs. Pas d'`ETag` (corps petits, la
  revalidation n'apporterait rien).

### Modifié

- **`/v1/intensity/date` et `/v1/intensity/stats` en `acv-ademe@2` : fenêtre
  plafonnée à 92 jours** (audit perf 2026-08) — la série `@2` est dérivée à la
  lecture (mix × flux transfrontaliers rechargés et joints en mémoire, sans
  rollup — ADR-0010 §6) : la garde générique de 366 j autorisait ~175 k lignes
  de flux lues + jointure + sérialisation par requête anonyme. Le plafond des
  **séries denses** (92 j, celui de `/exchanges/date`, `/weather/date` et
  `/price/date`) s'applique désormais à ces deux chemins : 400 explicite
  au-delà (OpenAPI mise à jour). Les autres méthodologies conservent 366 j.

### Corrigé

- **Rollups : fin des parcours séquentiels complets à chaque cycle de poll**
  (audit perf 2026-08) — le rafraîchissement incrémental filtre par `at >= $1`
  seul, prédicat qu'aucun index ne servait (PK `(region, at, …)`, index
  `(region, methodology_id, at DESC)`) : chaque cycle faisait 2 seq scans de
  toute la table `measurement` (~600 k lignes, en croissance), contredisant le
  « coût O(7 j) » visé par la migration 0010. Nouvelle migration `0012` : index
  **BRIN** sur `measurement (at)` (table écrite en ordre chronologique — index
  de quelques pages, scan borné aux blocs récents).
- **`weather_latest` : plus de lecture de tous les runs de la fenêtre** (audit
  perf 2026-08) — le `DISTINCT ON` lisait (et heap-fetchait) chaque run de
  chaque échéance avant de n'en garder qu'un, or la table anti-fuite (ADR-0012)
  les conserve tous (~192 par échéance en régime établi) : ~420 k tuples lus
  pour ~2 200 rendus sur `GET /v1/weather/date` à 92 j. Remplacé par une
  descente d'index par échéance (`DISTINCT valid_at` index-only + `LATERAL …
  LIMIT 1`) — un tuple rapatrié par échéance, réponse inchangée.
- **`upsert_weather` : doublon de clé toléré dans un même lot** (audit
  2026-08) — un couple `(valid_at, run_at)` dupliqué dans le même lot faisait
  échouer tout l'INSERT multi-lignes (« ON CONFLICT ne peut affecter deux fois
  la même ligne ») : dédup avant l'upsert (dernière occurrence conservée, même
  sémantique que l'upsert), sur le patron de `upsert_flows`/`dedup_by_key`.
- **ENTSO-E : courbes `A03` développées en série complète** (audit 2026-08) —
  le parseur IEC 62325 ignorait `curveType` et `timeInterval.end` : les
  positions omises d'une courbe A03 (valeur reconduite jusqu'au point suivant)
  étaient traitées comme absentes. Un flux A11 stable (une seule position
  émise) se réduisait à son premier pas — net transfrontalier faussé jusqu'à
  l'inversion de signe dans `acv-ademe@2` et `/v1/exchanges` — et un prix A44
  constant laissait des trous dans `spot_price` (pilier prix rfnbo indéterminé
  à tort). Chaque point est désormais reconduit jusqu'à la position du point
  suivant ou la fin de période (comblement inconditionnel, sans effet sur une
  courbe A01 complète), dans les trois développements (flux, génération, prix),
  avec garde contre les périodes démesurées (esprit F14) ; complétude testée
  sur la fixture officielle A11 (24 pas) + prix PT15M à positions omises.
- **`acv-ademe@2` : contexte d'import borné en fraîcheur** (audit 2026-08) — la
  jointure « au plus proche ≤ » reconduisait le dernier snapshot d'échanges sans
  limite d'ancienneté : en cas de panne ENTSO-E, `/v1/intensity/now`, `/date` et
  `/stats` en `?methodology=acv-ademe&version=2` servaient un contexte figé
  (heures, voire jours) sous l'horodatage frais du mix. Nouvelle constante de
  domaine `MAX_FLOW_CONTEXT_AGE` (1 h, cadence des flux A11) appliquée à la
  jointure de série (créneau omis), au chemin courant (`404` plutôt qu'une
  valeur périmée) et à `flows_at` côté SQL — `/v1/exchanges` cesse de même de
  servir un snapshot périmé comme courant.
- **`/v1/price/date` : prix spot borné en fraîcheur** (audit 2026-08) — la série
  reportait le dernier prix spot connu sur tous les créneaux suivants, sans
  limite : après un trou d'ingestion ENTSO-E, une semaine entière pouvait
  recevoir le prix d'avant la panne, présenté comme factuel. La jointure de
  `price_series` omet désormais les créneaux dont le prix a plus de 6 h
  (`MAX_SPOT_STALENESS`, promue constante de domaine partagée avec la garde du
  chemin courant `/v1/price`).
- **Fraîcheur de l'ingestion des flux transfrontaliers observable** (audit
  2026-08) — nouvelle jauge Prometheus
  `carbonfr_poller_last_flows_timestamp_seconds` (sur le modèle de
  `last_price`) : une panne ENTSO-E côté flux n'était visible que dans les
  logs, contrairement à l'esprit de l'ADR-0022 (« alerte phare = fraîcheur »).
- **Éligibilité : part « observée » du nowcast bornée à un pas** (audit
  2026-08) — avec un `?from=` passé sur
  `greenest-window?eligibility=rfnbo`, tous les créneaux passés recevaient la
  part renouvelable de la DERNIÈRE mesure, servie `observed` avec verdict
  ferme, alors que le pilier prix du même verdict était, lui, évalué à
  l'horodatage du créneau. La branche nowcast est désormais restreinte aux
  créneaux à ≤ 15 min de la dernière mesure ; un créneau passé plus ancien
  relit sa **propre** part observée dans le batch d'historique (second batch
  borné à l'étendue des créneaux si `from` précède la fenêtre climatologique),
  sinon `Indeterminate` (donnée manquante) — jamais la part courante.
- **Éligibilité : fraîcheur du prix day-ahead stricte** (audit 2026-08) — la
  garde « ≤ 1 h » inclusive appliquait le prix de l'heure de livraison
  précédente à un créneau situé exactement 1 h après (un prix horaire couvre
  `[t, t + 1 h)`) : borne désormais stricte (`< 1 h`) dans `freshest_price`
  et `spot_price_at` — à défaut de prix propre au créneau, le pilier prix est
  indéterminé, jamais reconduit.
- **`share-meteo@2` : mix dégénérés écartés de l'apprentissage** (audit
  2026-08) — un mix présent mais de total ≤ 0 (trou de donnée) alimentait les
  climatologies de canal (zéros dans les moyennes de créneau) et la
  calibration éolien/solaire (0 MW face à une vraie météo) ; ces mesures sont
  désormais ignorées entièrement (l'ancre, déjà protégée, ne change pas).
  Expérience non servie — aucun impact sur le contrat `/v1`.
- **`/v1/intensity/forecast` : 400 (et non 500) pour `acv-ademe@2` hors
  national** (audit 2026-08) — le handler était le seul chemin `@2` sans garde
  de région : l'erreur client finissait en `ForecastError::Unavailable` → 500
  `internal`, en contradiction avec `/now`, `/date` et `/stats` (400 explicite,
  ADR-0010 §8). La garde 400 est posée avant l'état de câblage du modèle (la
  faute client prime sur le 404 « non câblé »).
- **`POST /v1/webhooks` : rejets du corps JSON en Problem Details** (audit
  2026-08, ADR-0021) — l'extracteur `axum::Json` brut renvoyait ses rejets en
  `text/plain` (JSON malformé, champ manquant, Content-Type absent, corps trop
  grand) sans `type`/`title`/`code`. Nouvel extracteur `ValidatedJson`
  (symétrique de `ValidatedQuery`, audit F15) : corps `application/problem+json`
  au code stable `bad_request`, statut de la réjection conservé
  (400/413/415/422) ; le 422 (champ manquant/mal typé) est désormais documenté
  dans l'OpenAPI.
- **`version` validée sur `greenest-window`, `/schedule`, `/schedule/slots` et
  `/intensity/below`** (audit 2026-08) — le paramètre y était silencieusement
  ignoré : `?methodology=acv-ademe&version=2` servait la prévision `@1` en
  laissant croire à du `@2`. Comme `/v1/mix` (audit F12) : version inconnue →
  400, et `acv-ademe&version=2` (servie uniquement par
  `/v1/intensity/forecast`) → 400 explicite. Paramètre ajouté à l'OpenAPI.
- **NaN/infini rejetés pour `energy_kwh` (`/v1/schedule`) et `below`
  (`/v1/intensity/stream`)** (audit 2026-08) — `energy_kwh=NaN` passait la
  validation `< 0` et infectait toute l'économie calculée ; `below=NaN`
  désactivait silencieusement le filtre SSE (toute comparaison avec NaN est
  fausse). Rejet 400 « nombre fini » exigé, comme `threshold` sur `/below`.
- **CORS redevenue la couche la plus externe** (audit 2026-08) — le middleware
  d'auth/quota (`enforce`, tier hébergé opt-in) était posé par la composition
  root **au-dessus** de la `CorsLayer` : les préflights `OPTIONS` étaient
  décomptés du seau anonyme (jusqu'à bloquer une appli navigateur à clé dont le
  quota propre était intact) et les 401/429/503 partaient sans
  `Access-Control-Allow-Origin` — réponses opaques en navigateur, `RateLimit-*`/
  `Retry-After` illisibles malgré `expose_headers`. Le layer d'auth est
  désormais appliqué par `router()` **sous** la couche CORS, les `OPTIONS` sont
  exemptés de quota dans `enforce` (défense en profondeur) et le préflight est
  mis en cache côté navigateur (`Access-Control-Max-Age: 3600`).
- **RateLimiter : purge au changement de minute + plafond dur** (audit
  2026-08) — la « purge légère » (`len` > 10 000 → `retain` de la minute
  courante) ne retirait rien pendant une inondation d'identifiants distincts
  (tous de la minute courante) et re-scannait toute la carte **sous le mutex
  partagé à chaque requête** `/v1` (sérialisation de tout le trafic). La purge
  ne tourne plus qu'une fois par changement de minute, et au-delà de 10 000
  identifiants suivis, les identifiants inédits partagent un seau de
  débordement unique — mémoire et CPU bornés même sous rotation d'adresses.
- **Prévision : fin de la relecture de ~10 semaines d'historique à chaque
  requête** (audit perf 2026-08) — les 5 endpoints de prévision (`/forecast`,
  `/greenest-window`, `/schedule`, `/schedule/slots`, `/below`) relisaient
  ~70 j de mesures en base et rebâtissaient la climatologie **par requête**
  (jusqu'à ~13 500 lignes avec `?eligibility=rfnbo`), pour une donnée qui ne
  change qu'au cycle du poller. Nouveau décorateur `CachedForecaster`
  (adapter, port `ForecastModel` inchangé) : série mémorisée par clé
  `(region, methodology, from aligné sur le pas, horizon)`, TTL = intervalle
  de poll (`CARBONFR_POLL_SECS`), taille bornée — seul le trafic
  `from ≈ maintenant` est mis en cache (un `from` explicite passé/futur garde
  exactement le comportement d'avant), appliqué à `climatology@1` **et**
  `acv-ademe@2`. La fenêtre climatologique de part renouvelable de l'overlay
  `rfnbo` (`share-clim@1`) est de même mise en cache (clé = ancre nowcast +
  TTL) — la sémantique mono-forecast (ADR-0026 D16) est préservée : fenêtre
  verte et overlay partagent toujours la même série. ADR-0009 intact : la
  prévision reste calculée à la lecture, jamais persistée.

### Sécurité

- **Les clés API invalides ne contournent plus le quota** (audit 2026-08) —
  une requête à Bearer inconnu sortait en 401 **avant** le contrôle de quota :
  le chemin non authentifié le plus coûteux (SHA-256 + un SELECT Postgres par
  requête, pool partagé avec le poller) était le seul jamais throttlé. Les
  échecs de résolution (clé inconnue, base injoignable) sont désormais
  décomptés du **seau anonyme de l'IP** (429 au-delà de la limite), un seau
  déjà épuisé coupe court **avant** l'aller-retour base, et un cache négatif
  borné (empreinte → inconnue, TTL 60 s) évite de re-résoudre la même clé
  invalide rejouée en boucle.
- **`X-Real-Ip` n'est plus lu par défaut, IP toujours validée** (audit
  2026-08) — sous `CARBONFR_TRUST_PROXY=1`, l'en-tête `X-Real-Ip` **cru**
  primait sur le dernier segment de `X-Forwarded-For` : derrière un proxy qui
  ne l'écrase pas (dont l'exemple `deploy/Caddyfile` du dépôt tel quel), le
  quota anonyme était contournable à volonté et le compteur de visiteurs
  gonflable par valeurs forgées. Défaut désormais : **dernier segment de
  `X-Forwarded-For`** (sûr par construction avec tout proxy qui appende),
  valeur toujours parsée comme adresse IP — sinon seau `unknown` ; en-tête
  dédié en **opt-in** explicite via `CARBONFR_REAL_IP_HEADER` (le proxy doit
  l'écraser — `header_up X-Real-IP {remote_host}` ajouté au Caddyfile,
  `deploy/README.md` corrigé).

## [0.6.0] - 2026-07-03

La couche **B-light** d'ADR-0025 : `GET /hydrogene`, carte auto-contenue
« électrolyseurs × carbone live » — le croisement infra hydrogène × carbone
temps réel qui n'existe nulle part ailleurs. Hors contrat `/v1`, aucun
changement d'API.

### Ajouté

- **Carte « électrolyseurs × carbone live »** (`GET /hydrogene`, couche B-light —
  [ADR-0029](docs/adr/0029-carte-electrolyseurs-carbone-live.md), chantier H6 de
  la roadmap hydrogène) : page **auto-contenue** (zéro CDN, zéro tuile, zéro
  bibliothèque — SVG maison, thème clair/sombre, palettes validées) croisant les
  **233 électrolyseurs européens géolocalisés** de l'European Hydrogen
  Observatory (© Clean Hydrogen JU, instantané semestriel Dec2025, filtre
  `Water electrolysis`) avec la donnée live de l'API : choropleth des 12 régions
  (`acv-ademe`), bandeau national temps réel (SSE), fenêtres d'éligibilité
  `rfnbo`/`low-carbon`. Fond de carte : IGN Admin Express (Licence Ouverte 2.0)
  + Natural Earth (domaine public) — GISCO/Eurostat écarté (clause commerciale
  EuroGeographics), Vig'Hy écarté (pas de licence publiée). **Hors contrat
  `/v1`** (comme `/docs`) + trois jeux de données embarqués avec provenance
  (`/hydrogene/{sites.json,regions.geojson,pays.geojson}`). Neutralité : la
  page n'affiche jamais une éligibilité **par site** (donnée niveau site
  absente) — la couleur carbone est celle du réseau. Gardé par tests
  (auto-contenance, provenance, contrat du dataset, routes).

## [0.5.0] - 2026-07-03

Le pilier renouvelable du cadre `rfnbo` devient **prévisionnel** : `share-clim@1`
(ADR-0028), gardé par un double GATE (backtest walk-forward + re-jeu de la revue
de neutralité, GREEN). Contrat `/v1` enrichi de façon **purement additive**
(`provenance`, `value_lower`/`value_upper`, `reason`, `share_model`) — aucun
changement cassant.

### Ajouté

- **Part renouvelable prévue pour l'éligibilité rfnbo** (`share-clim@1`,
  [ADR-0028](docs/adr/0028-prevision-part-renouvelable-eligibilite.md), chantier
  H4 de la roadmap hydrogène) — le pilier `renewable-share` de
  `greenest-window?eligibility=rfnbo` était indéterminé sur 100 % des créneaux
  futurs (constat C4 de la revue de neutralité) ; il est désormais servi par une
  climatologie horaire-de-semaine de la part renouvelable, corrigée d'anomalie
  ancrée sur le nowcast, avec **intervalle calibré par quantiles de résidus par
  horizon** : verdict ferme seulement hors recouvrement du seuil 0,90 (règle
  symétrique de l'intervalle bas-carbone), `Indeterminate` sinon, **jamais** de
  prévision au-delà de l'horizon calibré (72 h) ni sans bandes calibrées.
  - **GATE de backtest franchi** (sous-commande `backtest-share`, walk-forward,
    vérité dérivée du mix) sur deux fenêtres indépendantes : RMSE 0,0410 vs
    0,0435 (persistance) sur mars-avril 2026 et 0,0595 vs 0,0640 sur
    oct.-nov. 2025, **0 faux verdict ferme sur 450**.
  - Champs **additifs** : `provenance` (`observed`/`forecast`) servi sur **tous**
    les piliers tranchés (parité de divulgation — l'intensité de l'overlay est
    toujours une prévision, le prix day-ahead une donnée publiée),
    `value_lower`/`value_upper` sur le signal de part prévue, `reason` sur tout
    signal indéterminé (`missing-data`/`beyond-calibrated-horizon`/
    `threshold-within-interval`/`surplus-not-established`), `share_model` sur
    l'overlay ; disclaimer réécrit (provenance de chaque pilier explicitée, sans
    sur-promesse d'horizon). SDK TypeScript et OpenAPI à jour.
  - **GATE de neutralité re-joué** (engagement de la revue) : RED étroit
    (4 constats F1/F3/F6/F12) → 4 correctifs → **GREEN** — revue §6 de
    [`docs/adr/0026-revue-neutralite.md`](docs/adr/0026-revue-neutralite.md).
  - Calibration au démarrage : `CARBONFR_SHARE_CALIBRATE_WEEKS` (défaut 8,
    `0` = off → comportement précédent) et `CARBONFR_SHARE_CALIBRATE_TO`
    (reproductibilité). +1 requête SQL batch par appel `?eligibility=rfnbo`
    (motif F05, garanti par test anti-N+1).

## [0.4.5] - 2026-07-03

GATE de neutralité de la couche « éligibilité électrolyseur » (ADR-0026) :
verdict **GREEN** après 3 correctifs additifs, plus la roadmap hydrogène et
l'addendum de vérification réglementaire sur sources primaires. Aucun
changement cassant.

### Ajouté

- **Roadmap hydrogène** ([`docs/roadmap-hydrogene.md`](docs/roadmap-hydrogene.md)) —
  séquencée par déclencheurs réglementaires (activation de `rfnbo:2026-revision`
  sur texte adopté uniquement, `MixForecast`, couche B-light, signaux de
  veille) ; **addendum ADR-0026** de vérification sur sources primaires
  (2026-07-03) : l'annexe du Règl. (UE) 2025/2359 ne fixe aucun seuil
  électrique (proxy `indicative` confirmé), la révision RFNBO n'est pas
  adoptée ; doc de l'overlay `?eligibility=` ajoutée aux README (racine + SDK).

### Modifié

- **GATE de neutralité de la couche « éligibilité électrolyseur » franchi**
  (ADR-0026 ; revue datée
  [`docs/adr/0026-revue-neutralite.md`](docs/adr/0026-revue-neutralite.md)) —
  évaluation adversariale multi-agents (critiques pro et anti-nucléaire +
  auditeurs symétrie/provenance/mélecture/usage, contre-instruction à 3
  réfutateurs par constat) rejouée sur la **sortie réellement servie**. Verdict
  RED (3 constats majeurs) → 3 correctifs → GREEN au re-test. Correctifs, tous
  **additifs** (aucune rupture de contrat) :
  - le signal d'un pilier dont le seuil a été **surchargé par l'appelant**
    (`?surplus_price_eur_mwh=`, `?low_carbon_threshold_g_per_kwh=`,
    `?electrolyzer_kwh_per_kg=`) est désormais étiqueté `basis: "user-override"`
    au lieu de conserver `regulatory`/`indicative-non-regulatory` (constat C14 —
    un seuil écrasé ne dérive plus du texte canonique) ; suivi granulaire par
    pilier dans `EligibilityRuleset` (`surplus_price_overridden`,
    `low_carbon_threshold_overridden`, méthode `basis_for`) ;
  - le champ `score` des créneaux d'éligibilité est documenté dans le contrat
    OpenAPI comme **interne au cadre** (jamais comparable entre `framework`s :
    `low-carbon` = intensité brute, `rfnbo` = heuristique composite ; comparer
    via `intensity`) (constat C8) ;
  - le `legal_basis` servi pour `low-carbon:2025-2359` attribue correctement le
    comparateur 94 gCO₂eq/MJ au renvoi vers le Règl. (UE) 2023/1185 et
    requalifie l'échéance de consultation nucléaire en **considérant non
    contraignant** (échéance 30/06/2026, lancement non constaté au 2026-07-03 ;
    évaluation contraignante d'ici 07/2028, art. 3) (constat C9 — aligné sur
    l'addendum de vérification sources primaires de l'ADR-0026).

## [0.4.4] - 2026-07-03

Remédiation des **24 constats restants** de l'audit du 2026-07-02 (moyens F07–F19,
bas F20–F31 ; le critique F01 et les hauts F03–F06 sont sortis en `v0.4.3`).
Aucun changement d'API cassant ; le contrat OpenAPI est enrichi (media-types
d'erreur, paramètres requis).

### Sécurité

- **Quota d'abonnements webhook rendu atomique** (audit F22). Le contrôle
  « ≤ 50 abonnements par clé » était un check-then-act (lecture puis insertion
  séparées) contournable par des créations concurrentes. Le comptage et
  l'insertion se font désormais dans une transaction avec verrou consultatif
  Postgres scindé par propriétaire (`pg_advisory_xact_lock`) — plus de fenêtre
  TOCTOU. Le port `SubscriptionRepository::create` porte le plafond et renvoie un
  booléen (inséré / quota atteint).
- **Fuite temporelle à l'entraînement GBDT corrigée** (audit F11). La sélection
  de la météo pour l'entraînement gardait le `run_at` le plus récent sur toute la
  fenêtre, sans le borner par l'origine de chaque exemple — une prévision publiée
  *après* l'origine pouvait fuiter dans les features. La sélection se fait
  désormais **par origine** (`run_at ≤ origine`), comme à l'inférence. Sans effet
  sur les modèles servis (`gbdt@1` ne battait pas `climatology@1`), mais assainit
  toute itération ML future.

### Robustesse

- **XML ENTSO-E malformé ne fait plus paniquer le poller** (audit F14). Un
  `<position>` anormalement grand ou un horodatage tronqué provoquait un panic
  (dépassement `OffsetDateTime + Duration`, slice non-UTF-8) qui tuait le
  processus — contraire au principe « échec par source non bloquant ». Le calcul
  passe par `checked_add`/`checked_mul` → `EntsoeError::Parse`, et le slicing par
  `str::get`. Motif corrigé sur les deux chemins (génération **et** prix).
- **`statement_timeout` Postgres** (audit F09). Le pool ne bornait pas la durée
  d'exécution côté serveur : une requête lente ou une session
  idle-in-transaction pouvait monopoliser une connexion indéfiniment. Ajout de
  `statement_timeout` + `idle_in_transaction_session_timeout` (défaut 30 s,
  `CARBONFR_DB_STATEMENT_TIMEOUT_MS`).
- **Le quota (opt-in) n'engloutit plus `/metrics`, `/health`, `/health/ready`**
  (audit F07). Quand `CARBONFR_RATELIMIT_ENABLED=1`, le middleware s'appliquait à
  toutes les routes, dont les sondes et le scrape Prometheus (429 possible → panne
  auto-infligée). Le middleware ne s'applique plus qu'au contrat `/v1`.

### Corrigé

- **`greenest_window_before` : un créneau unique avant l'échéance est renvoyé**
  (audit F10). Une échéance très proche ne laissant qu'un créneau candidat rendait
  `404 « série insuffisante »` au lieu de ce créneau — le cas d'usage le plus
  critique de `/v1/schedule`.
- **Webhooks : région non-nationale refusée explicitement** (audit F08). Le
  watcher ne surveille que le national ; un abonnement régional était accepté mais
  ne se déclenchait jamais silencieusement. `POST /v1/webhooks` renvoie désormais
  400 pour toute région ≠ nationale.
- **`/v1/mix` valide `version`** (audit F12). Le paramètre `version` documenté
  était ignoré : `version=999` renvoyait 200, `acv-ademe&version=2` servait
  silencieusement le mix `@1`. Les deux renvoient désormais 400 (`/v1/mix` ne sert
  que le mix de production).
- **Méthodologie inconnue → 400** (audit F30). `?methodology=rte-driect` (faute de
  frappe) produisait un `404 no_data` trompeur ; c'est désormais un 400
  « méthodologie inconnue », symétrique au traitement des régions.
- **Intensité consommation indéfinie → `None`** (audit F25). Un cas de transit net
  négatif était clampé à `0 gCO₂eq/kWh` (trompeur) au lieu d'être rapporté absent,
  contrairement à la méthode production-based.
- **`solar_capacity_factor` borné à [0, 1]** (audit F28). Une irradiance > 1000
  W/m² (réflexion de bord de nuage) pouvait dépasser l'invariant documenté.
- **Fusion champ-à-champ dans `upsert_loads`** (audit F24). Deux `LoadRecord`
  complémentaires (réalisée seule + prévue seule) du même lot s'écrasaient au lieu
  de fusionner ; défense contre une future source livrant des lots mixtes.

### Contrat & API

- **Toutes les erreurs de l'OpenAPI en `application/problem+json`** (audit F13).
  Les 34 réponses d'erreur documentaient `application/json` alors que le serveur
  émet bien `application/problem+json` (RFC 9457).
- **Erreurs de désérialisation de paramètres en Problem Details** (audit F15). Une
  valeur non coercible (`?horizon_hours=abc`) renvoyait un `400 text/plain` brut ;
  un extracteur `ValidatedQuery` produit désormais un Problem Details `bad_request`.
- **404 route inconnue en Problem Details** (audit F16). Un chemin inexistant
  recevait le 404 vide d'axum (sans `Content-Type`) ; un fallback renvoie un
  Problem Details `not_found`.
- **`WWW-Authenticate` sur les 401** (audit F21, RFC 6750).
- **`from`/`to` marqués requis dans l'OpenAPI** (audit F26) : ils étaient
  documentés optionnels alors que leur absence donne un 400.
- **Description de `count` clarifiée** sur `/v1/schedule/slots` (audit F27) : le
  plafonnement silencieux au nombre de créneaux disponibles est désormais
  documenté.

### Performance

- **`/v1/intensity/stats` (acv-ademe@2) : dérivation calculée une seule fois**
  (audit F18). Le résumé et la série agrégée refaisaient chacun toute la lecture +
  dérivation ; l'historique est désormais dérivé une fois puis réutilisé
  (`summarize`/`bucketize`).
- **`/v1/weather*` : déduplication déléguée à Postgres** (audit F19). La lecture
  ramenait tout l'historique des runs par échéance avant de le dédupliquer en
  Rust ; une nouvelle méthode `weather_latest` (`DISTINCT ON (valid_at)`) ne
  transfère qu'une ligne par échéance. `weather_range` (historique brut) reste
  intact pour l'anti-fuite GBDT.
- **`record_visit` ne recalcule plus un `COUNT(DISTINCT)` complet** à chaque visite
  déjà comptée (audit F31) : cache mémoire par processus, recalcul seulement à
  l'insertion effective.

### Durcissement

- **Génération de secrets via CSPRNG userspace** (audit F29) : `random_hex`
  (secrets webhook) **et** `generate_api_key` (sous-commande `mint-key`) n'ouvrent
  plus `/dev/urandom` en I/O synchrone sur le runtime Tokio (`rand::rng()`).

### Documentation

- **Invariant de version du port `IntensityRepository`** (audit F17) : les lectures
  ne filtrent que sur `methodology_id` ; l'invariant « au plus une version
  persistée par id » (vrai aujourd'hui) est désormais explicite dans le trait.
- **Compromis fenêtre-fixe du rate-limit documenté** (audit F20) : garde de
  dégradation anti-abus, pas un SLA strict (comptage exact = futur `UsageMeter`).

## [0.4.3] - 2026-07-03

Release patch de sécurité : corrige le contournement SSRF critique du filtre
d'IP des webhooks (F01) et trois autres constats hauts de l'audit du 2026-07-02
(F03–F06). Aucun changement d'API.

### Sécurité

- **SSRF webhooks — contournement du filtre d'IP par encodage alternatif corrigé**
  (audit F01/F23). `validate_webhook_url` détectait un hôte « littéral IP » avec
  `str::parse::<IpAddr>()`, qui ne reconnaît que la forme décimale pointée. Les
  formes décimale entière (`2130706433`), octale, hexadécimale et courte (`127.1`)
  étaient traitées comme des noms de domaine, mais reqwest les normalise en IP et
  s'y connecte **sans** passer par le resolver anti-SSRF — un porteur de clé API
  pouvait ainsi faire joindre par le serveur des services internes (loopback,
  `169.254.169.254`, autres conteneurs). L'analyse passe désormais par `url::Url`
  (le même analyseur WHATWG que reqwest), de sorte que l'hôte validé est
  exactement celui qui sera contacté. Corrige aussi la plage IETF `192.0.0.0/24`
  (seule l'adresse `.0` était filtrée). Aucun changement d'API.
- **Fuite du token ENTSO-E dans les logs corrigée** (audit F06). À chaque erreur
  réseau vers la Transparency Platform, `e.to_string()` propageait l'URL complète
  de la requête — qui porte le `securityToken` en query-string — dans un `warn!`
  du poller, donc dans les logs (surtout `CARBONFR_LOG_FORMAT=json` agrégé). Seule
  la **nature** de l'erreur est désormais journalisée, jamais l'URL (même blindage
  que le DSN Postgres).
- **DoS de l'overlay d'éligibilité corrigé** (audit F05). `GET /v1/intensity/greenest-window?eligibility=…`
  (anonyme, sans rate-limit par défaut) faisait jusqu'à **288 requêtes prix
  séquentielles** (une par créneau) vers le pool Postgres partagé, permettant
  d'affamer le poller et les autres routes. Un **seul** aller-retour couvre
  désormais tous les créneaux, et **aucun** prix n'est requêté pour un cadre sans
  pilier prix (`low-carbon`).

### Corrigé

- **Éligibilité électrolyseur — seuil bas-carbone dérivé borné** (audit F03). Un
  `electrolyzer_kwh_per_kg` absurde (ex. `0.53`, erreur d'unité) dérivait un seuil
  d'intensité gigantesque (~6385 gCO₂eq/kWh) qui échappait à la borne `]0, 1000]`
  du seuil direct et rendait le pilier `low-carbon` trivialement toujours vrai. La
  validation HTTP est resserrée à `[10, 200]` (borne physique) et le seuil dérivé
  est plafonné dans le crate de domaine (défense en profondeur).
- **`GET /v1/weather/date` & `/v1/exchanges/date` — paramètres sans effet retirés**
  (audit F04). Ces deux endpoints documentaient et acceptaient `region`,
  `methodology` et `version` alors qu'ils les **ignorent** (la météo est nationale,
  les échanges n'ont pas de méthodologie). `region=bretagne` renvoyait 200 avec les
  données nationales au lieu du 400 « région inconnue » de `/v1/intensity/date`. Ils
  utilisent désormais une struct dédiée `from`/`to` uniquement (OpenAPI mis à jour).

## [0.4.2] - 2026-07-02

Release patch de sécurité : mise à jour de dépendances sur advisories RustSec
(aucun changement fonctionnel ni d'API).

### Sécurité

- **Dépendances mises à jour sur advisories RustSec** (porte `cargo-deny` de la CI) :
  `quick-xml` 0.40.1 → **0.41.0** (RUSTSEC-2026-0194 : vérification des attributs
  dupliqués en temps quadratique ; RUSTSEC-2026-0195 : allocation non bornée des
  déclarations d'espaces de noms dans `NsReader` — deux DoS sur XML non fiable,
  `adapter-entsoe` parse les réponses ENTSO-E) et `anyhow` 1.0.102 → **1.0.103**
  (RUSTSEC-2026-0190 : *unsoundness* de `Error::downcast_mut()` après `context()`).
  Aucun changement d'API : compilation, tests et parsing des fixtures ENTSO-E inchangés.

## [0.4.1] - 2026-06-22

Correctif d'ergonomie de l'API : la racine de version ne renvoie plus un 404.

### Modifié

- **`GET /v1` redirige (307) vers `/docs`** au lieu de renvoyer un `404`. `/v1` est un
  préfixe de version, pas un endpoint de données — aucune route n'y était montée. La
  redirection oriente l'utilisateur qui tape l'URL de base vers la documentation
  interactive (catalogue des routes). Redirection *temporaire* : `/docs` reste un détail
  d'implémentation, non gravé en cache navigateur.

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
