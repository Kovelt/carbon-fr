# ADR-0010 — Méthodologie `acv-ademe` (cycle de vie + imports, *consumption-based*)

- **Statut** : Accepté (mise en œuvre **engagée** — domaine pur + endpoints de vérifiabilité livrés ; source d'import ENTSO-E à brancher)
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

⚠️ `TD_LOSS_FACTOR_V1 = 0,072` est un **ordre de grandeur à sourcer précisément**
(Bilan électrique RTE / Base Carbone ADEME) avant publication de `@2` ; tout
changement = bump de version.

**À venir (tranche B) :** adapter `adapter-entsoe` + port `CrossBorderSource` +
store du contexte d'import + ingestion par le poller + service effectif de
`acv-ademe@2` (statut `planned` dans `/v1/methodologies` tant que la source
n'est pas branchée). Le défaut de l'API **reste `rte-direct`**.

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

**Incluses**, cohérent avec un périmètre consommation et le modèle UK, via un **facteur versionné**. Le raffinement (taux instantané si la donnée est disponible *vs* constante documentée) est tranché à l'implémentation et **porté par la version** de la méthode.

### 4. Calcul : une *stratégie de domaine* pure

Un trait `MethodologyCalculator` dans `core`, avec deux implémentations : `RteDirect` et `AcvAdeme`. Une méthodologie est une **fonction pure** :

> `(mix, contexte d'import, facteurs) → CarbonIntensity`

Aucune IO, **testable avec des fakes en mémoire**, sélectionnée **par requête**. Le domaine introduit un *value object* `CrossBorderFlows` (MW **signés par voisin**) porté à côté du mix pour le chemin `acv-ademe` ; le `echanges` net existant reste pour `rte-direct` et l'affichage.

### 5. Source des imports : ENTSO-E (nouvel adapter)

Un **nouveau port sortant** `CrossBorderSource` (flux par frontière **+** intensité du voisin, au pas quart d'heure), implémenté par un crate **`adapter-entsoe`** (ENTSO-E Transparency Platform). Cela s'inscrit exactement dans l'ADR-0002 : une source additionnelle = un adapter derrière un port, sans toucher au domaine. ENTSO-E est un organisme **européen** → cohérent avec la contrainte de souveraineté. **Jamais appelé par requête utilisateur** : le **poller** l'ingère dans la base, comme pour ODRÉ.

### 6. Stockage : hybride (lecture + rollups)

- **Lectures point** (`/intensity/now`, date unique) → **calcul à la lecture**, depuis le **meilleur millésime** du mix + le contexte d'import stocké. Cohérence automatique aux révisions (`tr → consolidated → definitive`), **aucune ligne `acv` dans la table primaire** (pas de doublement de volume, pas de drift).
- **Lectures agrégées** (stats, `greenest-window`, entraînement du modèle de prévision) → `acv-ademe` **matérialisé dans les vues de rollup** (variante de méthode), rafraîchies par le poller → cohérence aux révisions **sans re-dérivation manuelle**.
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
