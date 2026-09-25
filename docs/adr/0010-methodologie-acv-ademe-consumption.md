# ADR-0010 — Méthodologie `acv-ademe` (cycle de vie + imports, *consumption-based*)

- **Statut** : Accepté (mise en œuvre **engagée** — domaine pur + endpoints de vérifiabilité livrés ; source d'import ENTSO-E branchée et validée live, ingestion poller + service `@2` à la lecture livrés ; cf. § État d'implémentation)
- **Date** : 2026-06-15
- **Raffine** : ADR-0005 (qui *engageait* `acv-ademe`) ; **fait évoluer l'ADR-0008** (de l'`acv-ademe@1` *production-based* livré vers une méthode *consumption-based*, imports ENTSO-E inclus)

## État d'implémentation (2026-06-15)

**Tranche A — domaine pur + vérifiabilité : livrée.** Le calcul *consumption-based*
est entièrement spécifié et testé **sans IO** :

- value object [`CrossBorderFlows`] (flux signés par voisin + intensité du voisin)
  porté à côté du mix (§4) ; enum `Neighbor` (6 frontières métropolitaines) ;
- trait **`MethodologyCalculator`** (§4) avec trois implémentations pures —
  `RteDirect` (report de la valeur publiée), `AcvAdemeProduction` (`@1`),
  `AcvAdemeConsumption` (`@2`) ;
- fonction pure `acv_ademe_consumption_intensity` : production − exports (à
  l'intensité de production) + imports (à l'intensité du voisin), rapporté à la
  consommation, **uplift pertes T&D** (`TD_LOSS_FACTOR_V1`, §3) ;
- `acv-ademe@2` est une **version distincte** de `@1` (gouvernance ADR-0005 : `@1`
  production reste publié, pas de modification silencieuse) ;
- **`GET /v1/methodologies`** (catalogue + versions) et **`GET /v1/factors`**
  (table des facteurs + facteur T&D) — le levier de vérifiabilité (§7), servis
  **sans dépendance externe**.

`TD_LOSS_FACTOR_V1 = 0,072` (≈ 7,2 %) est **sourcé** (§3 : transport RTE ~2,3 % +
distribution Enedis ~6 %, pertes techniques, hors non technique) ; tout
changement = bump de version.

**Tranche B (1/2) — adapter ENTSO-E + port : livrée.** Port sortant
`CrossBorderSource` + value object horodaté `CrossBorderSnapshot` (domaine) ;
crate **`carbonfr-adapter-entsoe`** :

- flux physique net **signé** par frontière (`documentType=A11`, import − export) ;
- **intensité du voisin** dérivée de sa génération par type (`documentType=A75`,
  `processType=A16`) via les **mêmes facteurs ADEME** que le domaine (méthode
  cohérente et vérifiable) — mapping `PsrType` (B01..B25) → filières ;
- zones EIC des 6 frontières métropolitaines ; assemblage en `CrossBorderSnapshot`
  alignés par horodatage (intensité voisine au plus proche ≤) ;
- token `CARBONFR_ENTSOE_TOKEN` ; **jamais appelé par requête utilisateur**.

Parsing XML **testé sur fixtures** (hermétique) ; chemins XML/codes calés sur le
guide RESTful API ENTSO-E, **à valider contre l'API live** (test `tests/live.rs`,
`--ignored`, token requis).

**Tranche B (2/2) — store + ingestion + service : livrée.**

- **Store** : port `CrossBorderRepository` + table `cross_border_flow`
  (`(at, neighbor)`, migration `0007`) ; `upsert_flows` (multi-lignes, dédup
  `(at, neighbor)`) et `flows_at` (snapshot du dernier horodatage ≤ cible).
  Validé par test d'intégration Postgres réel.
- **Ingestion** : le poller ingère le contexte d'import à chaque cycle **si**
  `CARBONFR_ENTSOE_TOKEN` est défini (source optionnelle, échec non bloquant).
- **Service** : cas d'usage `GetConsumptionIntensity` (calcul **à la lecture**,
  ADR-0010 §6 — aucune ligne `@2` stockée) ; servi via
  **`GET /v1/intensity/now?methodology=acv-ademe&version=2`** (national, §8).
  `acv-ademe@2` passe `served` dans `/v1/methodologies`.

Le défaut de l'API **reste `rte-direct`**. Sans token ENTSO-E, le chemin de calcul
existe mais renvoie `404` faute de contexte d'import ingéré.

**Historique & stats `@2` (§6) : livrés.** `acv-ademe@2` est désormais servi
**à la lecture** au-delà de `/now` :

- port `CrossBorderRepository::flows_range` (snapshots d'import sur un
  intervalle) + impl Postgres (validée sur base réelle) ;
- fonction pure `derive_consumption_series` (jointure par fusion mix × contexte
  d'import le plus proche ≤, O(n+m), créneaux non couverts omis) ;
- agrégats **calculés dans le domaine** (`summarize`/`bucketize`) — la série
  `@2` n'étant pas matérialisée, le résumé et la série par pas sont dérivés en
  mémoire, sans rollup SQL ;
- exposés via `GET /v1/intensity/date` et `GET /v1/intensity/stats` avec
  `?methodology=acv-ademe&version=2` (national). `@2` n'existe que là où le
  contexte d'import a été ingéré.

**Résolu (2026-06-16)** : (a) **chemins XML ENTSO-E validés** contre l'API live
(test `--ignored`, token réel) — 5 frontières actives (BE/DE/ES/IT/CH ; GB
indisponible depuis le Brexit), flux et intensités voisines plausibles ; URL de
base corrigée (`tp`, pas `tps`). (b) `TD_LOSS_FACTOR_V1 = 0,072` **sourcé** (§3 :
Bilans électriques RTE + Enedis, pertes techniques ≈ 7 %).

## Contexte

L'ADR-0005 a acté que la méthodologie carbone est un **attribut versionné de premier ordre**, et a *engagé* une méthode additionnelle `acv-ademe` — cycle de vie + imports, façon modèle UK — **coexistant** avec `rte-direct`, sans en spécifier le calcul. Le domaine a été préparé en conséquence :

- `Measurement` porte `methodology` (ADR-0005) et `vintage` (ADR-0006) ;
- la clé d'unicité est `(region, horodatage, methodology)` (ADR-0006) — deux méthodes = deux valeurs distinctes, sans collision ;
- `GenerationMix` porte déjà `echanges` (solde net des interconnexions) avec la mention explicite « porté pour la future méthode `acv-ademe` ».

Le présent ADR **spécifie** `acv-ademe`. Forces en présence : souveraineté FR/EU, posture *dev-first* (transparence et vérifiabilité de la méthode), résilience au quota (un **poller unique** alimente la base, jamais d'appel source par requête utilisateur — ADR §3), et la règle « pas d'extension méthodologique sans ADR ».

## Décision

### 1. Périmètre : *consommation* (consumption-based), imports inclus

`acv-ademe` mesure l'empreinte de l'électricité **réellement consommée** en France, en **cycle de vie**, au pas quart d'heure :

> émissions ACV de la **production FR** (facteurs ADEME par filière) **− exports** **+ imports** valorisés à l'**intensité du pays d'origine**, le tout rapporté à la consommation.

C'est le périmètre le plus exigeant, et le seul qui reflète qu'un import charbon allemand n'a pas l'empreinte du mix français. La méthode **coexiste** avec `rte-direct` ; le **défaut de l'API reste `rte-direct`** pour préserver la comparabilité directe à éCO2mix.

### 2. Facteurs : Base Carbone ADEME, versionnés

Une **table de facteurs cycle de vie** (gCO₂eq/kWh) par filière, issue de la **Base Carbone ADEME**, identifiée et **versionnée**, **injectée au composition root** (c'est une *donnée*, pas du code). Tout changement de facteurs = **bump de version** (`acv-ademe@N`) + trace ADR/journal. **Jamais** de modification silencieuse d'une méthode publiée (gouvernance ADR-0005).

### 3. Pertes de transport/distribution (T&D)

**Incluses**, cohérent avec un périmètre consommation et le modèle UK, via un **facteur versionné** (`TD_LOSS_FACTOR_V1`).

**Valeur v1 = 0,072 (≈ 7,2 %)**, *uplift* `× (1 + facteur)` sur l'intensité réseau. Sourcée sur les pertes **techniques** du système français, ramenées à l'énergie injectée :

| Segment | Taux | Source |
|---|---|---|
| Transport (RTE) | ~2,3 % (2,16 % en 2018, 2,22 % en 2019, 2,31 % en 2020) | [Bilan électrique RTE](https://www.rte-france.com/donnees-publications/publications/bilans-electriques-nationaux-regionaux) |
| Distribution (Enedis) | ~6 % (≈ 23 TWh/an) | [Bilan électrique Enedis](https://www.enedis.fr/) |

En cascade, livrer au consommateur BT impose `1,023 × 1,06 ≈ 1,084` (8,4 %) ; pondéré par la part de consommation transitant par la distribution (les gros industriels sont raccordés directement au réseau de transport), la moyenne système ressort à **≈ 7 %**. Les pertes **non techniques** (fraude, erreurs de comptage — le ~10 % parfois cité les inclut) sont **exclues** : cette énergie est consommée, pas dissipée, donc hors d'un périmètre carbone. Cohérent avec les facteurs ADEME, eux-mêmes dérivés des bilans RTE/Enedis.

Le raffinement (taux instantané si la donnée est disponible *vs* cette constante documentée) est tranché à l'implémentation et **porté par la version** de la méthode.

### 4. Calcul : une *stratégie de domaine* pure

Un trait `MethodologyCalculator` dans `core`, avec deux implémentations : `RteDirect` et `AcvAdeme`. Une méthodologie est une **fonction pure** :

> `(mix, contexte d'import, facteurs) → CarbonIntensity`

Aucune IO, **testable avec des fakes en mémoire**, sélectionnée **par requête**. Le domaine introduit un *value object* `CrossBorderFlows` (MW **signés par voisin**) porté à côté du mix pour le chemin `acv-ademe` ; le `echanges` net existant reste pour `rte-direct` et l'affichage.

### 5. Source des imports : ENTSO-E (nouvel adapter)

Un **nouveau port sortant** `CrossBorderSource` (flux par frontière **+** intensité du voisin, au pas quart d'heure), implémenté par un crate **`adapter-entsoe`** (ENTSO-E Transparency Platform). Cela s'inscrit exactement dans l'ADR-0002 : une source additionnelle = un adapter derrière un port, sans toucher au domaine. ENTSO-E est un organisme **européen** → cohérent avec la contrainte de souveraineté. **Jamais appelé par requête utilisateur** : le **poller** l'ingère dans la base, comme pour ODRÉ.

### 6. Stockage : hybride (lecture + rollups)

- **Lectures point** (`/intensity/now`, date unique) → **calcul à la lecture**, depuis le **meilleur millésime** du mix + le contexte d'import stocké. Cohérence automatique aux révisions (`tr → consolidated → definitive`), **aucune ligne `acv` dans la table primaire** (pas de doublement de volume, pas de drift).
- **Lectures agrégées** (stats, `greenest-window`) → ⚠️ **mis à jour par l'implémentation** (voir « reste ouvert » et CLAUDE.md) : `@2` est finalement **dérivé à la lecture puis agrégé en mémoire** (`summarize`/`bucketize`), **sans rollup SQL matérialisé** — choix plus simple, cohérent aux révisions par construction. *(L'intention initiale de matérialiser `@2` dans les vues de rollup a été abandonnée.)*
- Le poller **ingère aussi** le contexte d'import ENTSO-E (flux + intensités voisines) dans un store dédié, **aligné au pas quart d'heure** du mix.

### 7. Surface API

- **`?methodology=`** sur les endpoints d'intensité (défaut `rte-direct`).
- **`GET /v1/methodologies`** — liste des méthodes disponibles + versions.
- **`GET /v1/factors`** — table des facteurs par filière et par méthode. C'est le levier de **vérifiabilité**, donc de crédibilité : la méthode est auditable, pas un chiffre opaque.

### 8. Périmètre géographique : national d'abord

`acv-ademe` est livré **national** en v1. Le régional est **reporté** : l'intensité régionale est déjà une grandeur **dérivée par modèle** (le `taux_co2` est absent du jeu régional — addendum ADR-0003), donc `acv-ademe` régional serait une *dérivation sur dérivation*, à cadrer dans un ADR ultérieur.

## Conséquences

- **Moat renforcé** : méthode *consumption-based* façon UK **et** vérifiable, là où la source officielle n'offre ni l'une ni l'autre.
- **Pas de rupture de contrat** : `rte-direct` reste le défaut ; `acv-ademe` s'ajoute via un paramètre et de nouvelles routes.
- **Domaine** : ajout de `MethodologyCalculator`, `AcvAdeme`, `CrossBorderFlows`. Toujours **zéro IO** dans `core`.
- **Infra** : nouveau crate `adapter-entsoe` + port `CrossBorderSource` ; le poller orchestre désormais **deux sources** à aligner au pas quart d'heure ; nouvelle variante de vues de rollup ; **quota et disponibilité ENTSO-E** à gérer (token, *rate limit*) — atténués par le principe « un seul composant tape la source ».
- **Gouvernance** : tout changement de facteurs ou de traitement des pertes = **bump de version + trace**.
- **Coût** : complexité d'ingestion accrue (synchronisation de deux flux), surface d'API élargie (à versionner proprement sous `/v1`).

## Alternatives envisagées

- **Lifecycle de la prod FR seule (sans imports)** : plus simple, pas d'ENTSO-E — mais s'arrête à la production et ne voit pas l'électricité réellement consommée. Écarté comme méthode *publiée* ; conservé éventuellement comme **étape de calcul interne**.
- **Facteurs d'import annuels moyens par pays** (au lieu d'ENTSO-E temps réel) : aucune source nouvelle, mais grossier (ne distingue pas un import nocturne éolien d'une pointe charbon). Écarté pour la justesse ; reste un **fallback** si ENTSO-E est indisponible.
- **Tout-stocké (lignes `acv` primaires)** : agrégats triviaux, mais **double le volume** de la table primaire et impose de **re-dériver l'`acv` à chaque upsert** de millésime (risque de drift). Écarté au profit de l'hybride.
- **Tout-lecture (rien de stocké)** : **impossible** en *consumption-based* — les intensités voisines varient dans le temps et ne peuvent être cherchées par requête utilisateur ; il faut au minimum stocker le contexte d'import. Écarté.

## Questions ouvertes (implémentation — n'impactent pas le contrat public)

- **Taux de pertes T&D** : instantané (si donnée disponible) *vs* constante documentée versionnée.
- **Forme du store d'import** : table dédiée *vs* extension du modèle de mesure existant.
- **Cold-start ENTSO-E** : backfill historique des intensités voisines, requis pour entraîner la prévision `acv-ademe` (cohérence avec la phase 3).

## Addendum (2026-09-25) — Critère de déclenchement du régional *consumption-based*

### Contexte

Le §8 a reporté l'extension géographique d'`acv-ademe@2` *consumption-based* aux 12
régions, la qualifiant de « dérivation sur dérivation » : contrairement au
national, l'intensité régionale n'est déjà elle-même qu'une valeur **dérivée**
par modèle (`acv-ademe@1`, ADR-0008 — pas de `taux_co2` régional publié,
addendum ADR-0003). Au 2026-09-25, `acv-ademe@2` reste **national uniquement**
et `acv-ademe@1` (basé production) est la seule méthode ACV servie au régional —
confirmé par le catalogue `/v1/methodologies` (`crates/sdk/tests/fixtures/methodologies.json:1` :
`acv-ademe@1` → `"scope":"national + 12 régions"`, `acv-ademe@2` →
`"scope":"national"`). L'itération I7 (`docs/plan-iterations.md`, ligne I7,
« Critère de déclenchement d'un `acv-ademe` régional ») demande un critère
explicite et vérifiable. Cet addendum le fixe ; il ne change **rien** à ce qui
est servi aujourd'hui.

### Fait vérifié : la donnée bilatérale manque aujourd'hui

Un régional *consumption-based* demanderait, par analogie avec le national
(§5 : port `CrossBorderSource`, value object `CrossBorderFlow` — flux **signé
par voisin nommé** + intensité de ce voisin, `crates/core/src/domain/cross_border.rs:57-66`),
un flux **bilatéral** entre chaque région française et chacune de ses régions
limitrophes (+ l'étranger), et pas seulement un solde agrégé. Cette donnée
**n'existe dans aucune source actuellement branchée ou recensée** :

1. **ODRÉ `eco2mix-regional-tr`** ne publie qu'un solde net agrégé,
   `ech_physiques`, sans détail par région voisine. Confirmé par la
   documentation du jeu (« the balance of physical exchanges with neighboring
   regions », <https://odre.opendatasoft.com/explore/dataset/eco2mix-regional-tr/>,
   consulté le 2026-09-25) et par le code déjà en place : `RegionalRecord`
   (`crates/adapter-odre/src/dto.rs:159-169`) ne décode qu'**un seul** champ
   `ech_physiques: Option<f64>` (ligne 168), mappé tel quel sur
   `GenerationMix.echanges` (ligne 195) — un scalaire, jamais une liste par
   voisin.
2. La page RTE dédiée à ce jeu (« Eco2mix – Consumption, Generation and
   Inter-Regional Flows », <https://www.rte-france.com/en/data-publications/eco2mix/regional-data>,
   consulté le 2026-09-25) confirme la même granularité : « the balance of
   power flows between regions » — un solde par région, pas une matrice
   région↔région.
3. L'API dédiée de RTE aux flux physiques (« Physical Flow »,
   `data.rte-france.com`) est **explicitement limitée aux frontières
   internationales** : « expose physical cross-border schedules detailing
   electricity flows actually transiting across the interconnection lines
   directly linking countries » (<https://data.rte-france.com/catalog/-/api/doc/user-guide/Physical+Flow/1.0>,
   consulté le 2026-09-25) — le même périmètre que l'adapter ENTSO-E déjà
   branché (§5), pas les régions françaises.
4. En interne, `GET /v1/exchanges` (ADR-0017) — le seul endpoint qui expose
   aujourd'hui un détail **par voisin nommé** plutôt qu'un solde — est
   explicitement borné aux « 6 frontières de la France » ; une éventuelle
   extension **internationale** (matrice pays↔pays) y est déjà traitée comme
   un chantier distinct non entamé, et une matrice **inter-régionale
   française** n'y est même pas évoquée (`docs/adr/0017-endpoint-echanges-transfrontaliers.md:27`).

Recherche faite par **documentation** (pages ODRÉ/RTE + code du dépôt), sans
appel à l'API ODRÉ/RTE/ENTSO-E (contrainte de session, cf. CLAUDE.md carbon-fr
§« À NE PAS faire »).

### Décision : critère de déclenchement

Rouvrir un `acv-ademe` régional *consumption-based* seulement si les **quatre
conditions** suivantes sont réunies :

1. **Donnée disponible** — RTE ou ODRÉ (ou une source tierce fiable, cohérente
   avec la contrainte de souveraineté FR/EU) publie un flux **bilatéral nommé**
   région↔région (et région↔international) analogue à `CrossBorderFlow` (§5),
   et pas seulement le solde `ech_physiques` actuel. Vérification : reproduire
   cette même recherche documentaire (catalogue ODRÉ + `data.rte-france.com/catalog`)
   — jamais par appel API — lors d'une revue de veille datée, ou dès qu'une
   annonce RTE la mentionne.
2. **Précision atteignable** — le flux, une fois trouvé, est publié à un pas
   compatible avec le mix régional existant et couvre les **12 régions sans
   trou** (sinon la méthode ne pourrait pas tenir la promesse de périmètre
   qu'elle afficherait dans `/v1/methodologies`).
3. **Cohérence vérifiable** — la somme des flux bilatéraux d'une région avec
   ses voisines + l'international doit pouvoir se recouper avec son solde
   `ech_physiques` publié : la méthode doit rester **auditable**, pas une boîte
   noire (même exigence de vérifiabilité que le levier §7 pour le national).
4. **Demande utilisateur documentée** — un besoin produit concret (comme la
   carte du dashboard qui a motivé `GET /v1/exchanges`, ADR-0017) plutôt
   qu'une extension spéculative ; à défaut, le chantier reste dans la table
   « En attente d'un déclencheur » de `docs/plan-iterations.md`, non priorisé.

### Ce qui serait requis si le critère se déclenche

- **Domaine** : un voisinage **régional français** distinct de `Neighbor`
  (`crates/core/src/domain/cross_border.rs`, aujourd'hui 6 pays fixes,
  périmètre **national**) — probablement un nouveau type plutôt qu'une
  extension de cet enum, pour ne pas mélanger les deux échelles.
- **Ingestion** : un nouveau port sortant (ou une extension du port ODRÉ) — à
  **recompter le quota avant tout branchement** : le poller régional consomme
  déjà **~80 %** du quota ODRÉ de 50 000 appels/mois avec le seul `acv-ademe@1`
  (addendum ADR-0003, 2026-09-23) ; une ingestion bilatérale par région
  rapprocherait la marge de zéro, voire la dépasserait selon la source retenue.
- **Gouvernance** : le précédent posé par l'ADR-0008 (extension du national au
  régional pour `acv-ademe@1`, **même version**, simple addendum du
  2026-06-20 — pas de bump pour une extension de **périmètre géographique** à
  formule inchangée) suggère qu'étendre `acv-ademe@2` au régional ne
  nécessiterait **pas** forcément une nouvelle version (`@3`) si la formule et
  les facteurs restent identiques — mais **au minimum un addendum** à cet ADR
  (un ADR dédié si le modèle de voisinage régional diverge substantiellement
  du national, p. ex. gestion des régions frontalières multi-pays) ; jamais
  une modification silencieuse (gouvernance ADR-0005).
- **Backtest** : revalider la plausibilité de la méthode au régional (comme
  §5/§ État d'implémentation l'a fait au national contre l'API ENTSO-E live le
  2026-06-16) avant de servir, en particulier pour les régions cumulant import
  inter-régional **et** international (ex. Grand Est, Hauts-de-France,
  Nouvelle-Aquitaine).

### Pourquoi le régional actuel reste correct entre-temps

`acv-ademe@1` **basé production** continue d'être servi et reste une méthode
**correcte et documentée** pour son périmètre déclaré :

- C'est une méthode **acceptée et versionnée à part entière** (ADR-0008), pas
  un pis-aller informel : elle répond à une promesse de périmètre (national
  **+ régional**, ADR-0003) que `rte-direct` ne peut pas tenir faute de
  `taux_co2` régional publié.
- Sa limite est **documentée, pas cachée** : « Basée production : pour une
  région importatrice, reflète la production locale, pas la conso (imports =
  `acv-ademe@2`) » (`CLAUDE.md:67`) ; le catalogue `GET /v1/methodologies`
  (§7) expose le `scope` exact par version, donc un client sait précisément
  ce qu'il lit.
- Le national sert **les deux** versions (`acv-ademe@1` *et* `acv-ademe@2`,
  `crates/sdk/tests/fixtures/methodologies.json:1`), ce qui donne un repère de
  l'écart production/consommation qu'un client peut appliquer avec prudence au
  régional en attendant.
- Gouvernance ADR-0005 : une méthode publiée ne se modifie jamais
  silencieusement — le statu quo régional n'est donc pas une improvisation à
  corriger en urgence, c'est l'état stable attendu tant que le critère
  ci-dessus n'est pas rempli.

> **Points à confirmer par Morgan** (aucune urgence — le régional actuel reste
> correct, cf. ci-dessus) :
> 1. **Veille RTE/ODRÉ** — ajouter ce critère à une revue périodique (comme la
>    veille hydrogène mensuelle, `docs/plan-iterations.md` §Échéances) ou le
>    laisser purement réactif (revérifié seulement si Morgan tombe sur une
>    annonce) ? *Recommandation : réactif* — le quota est déjà tendu (~80 %,
>    ADR-0003) et rien n'indique un chantier RTE en cours sur une matrice
>    inter-régionale ; une veille dédiée ajouterait du travail récurrent pour
>    un signal qui n'est pas attendu à court terme.
> 2. **Versionnement** — si la donnée apparaît un jour, confirmer le choix
>    « même version `@2`, addendum » (précédent ADR-0008) plutôt qu'une
>    version régionale dédiée (`acv-ademe-regional@1` ou similaire) ?
>    *Recommandation : suivre le précédent ADR-0008* (formule et facteurs
>    inchangés, seul le périmètre géographique s'étend) — cohérent avec la
>    gouvernance ADR-0005/ADR-0019 (versionner un **changement de méthode**,
>    pas une extension de couverture).
