# ADR-0033 — Index qualitatif d'intensité carbone (`index` : very-low → very-high)

- **Statut** : Accepté le 2026-10-20 — Morgan a retenu la proposition telle quelle (« garde les seuils proposés »), les quatre points de l'encadré ci-dessous compris ; le premier commit d'implémentation n'arrive qu'après cette acceptation (principe 3 du plan I9) ; **implémenté le 2026-10-20** (PR #156)
- **Date** : 2026-10-20
- **Décideurs** : Morgan (Kovelt / carbon-fr)
- **ADR liés** : ADR-0005 (méthodologies versionnées, immuables), ADR-0007 (contrat `/v1` additif), ADR-0008 (facteurs ACV ADEME = constante de domaine versionnée), ADR-0010 §7 (vérifiabilité : `/v1/factors`), ADR-0014 (flux SSE), ADR-0019 (champ/route ajoutés = *minor* en 0.x), ADR-0025/0026 (éligibilité électrolyseur : seuil RFNBO 18 gCO₂eq/MJ), ADR-0029 (carte `/hydrogene`, classes `<25, 25-45, 45-70, 70-100, ≥100`), ADR-0030 §3 (SDK Rust : champ public ajouté = *minor*), ADR-0031/0032 (SDK)

> ### Décision de Morgan du 2026-10-20 : la proposition, telle quelle
>
> | # | Point | Retenu (proposition de cet ADR) | Variante(s) écartée(s) |
> |---|---|---|---|
> | 1 | **Seuils** des 5 bandes (gCO₂eq/kWh) | **20 · 64,8 · 100 · 200**, absolus et sourcés (§ Décision 1) | arrondir 64,8 → 65 (perd la source exacte) ; 200 → 250 ; classes de la carte `25 · 45 · 70 · 100` (non sourcées) |
> | 2 | **Libellés** JSON | `very-low`, `low`, `moderate`, `high`, `very-high` (kebab-case, comme tous les slugs de l'API) | `very low`… avec espace (parité littérale NESO) ; libellés français `tres-faible`… |
> | 3 | **Catalogue** `GET /v1/intensity/index` (bornes, sources, version) | **oui** — même rôle que `/v1/factors` pour l'ACV : la légende est auditable, pas un chiffre opaque | non : seuils seulement dans la doc |
> | 4 | **Carte `/hydrogene`** alignée sur l'index | **oui** — une seule échelle dans le produit (légende = bandes de l'index) | garder les 5 classes dataviz actuelles |
>
> Les quatre points sont tranchés dans le sens de la proposition (réponse de Morgan du 2026-10-20 : « garde les seuils proposés pour l'ADR »). Les variantes restent consignées pour mémoire ; le risque prévu par le plan — des seuils trop disputés pour servir un index — ne s'est pas réalisé.

---

## Contexte

**Le besoin (PROD-API-1, état des lieux du 2026-09-26)** : carbon-fr sert des gCO₂eq/kWh, jamais une lecture qualitative. carbonintensity.org.uk (NESO, Grande-Bretagne) expose depuis 2017 un champ `intensity.index` à cinq valeurs — *very low, low, moderate, high, very high* — qui est **le** champ repris par le grand public, les outils no-code (Home Assistant, Node-RED, IFTTT : « si l'index est *very low*, lancer la charge ») et les tableaux de bord, parce qu'un libellé se lit sans connaître l'unité ni l'ordre de grandeur. Le plan I9 en fait le premier différenciateur de l'itération, **ADR d'abord** : des seuils se discutent (neutralité perçue), ils ne se glissent pas dans une PR de code.

**Précédent NESO — ce qu'il fait exactement** (méthodologie nationale, *Carbon Intensity Forecast Methodology*, NESO, édition de mai 2024, table 2 ; PDF lié depuis carbonintensity.org.uk, relu le 2026-10-20) : les bandes sont **absolues**, en gCO₂/kWh, et **programmées année par année, en baisse linéaire jusqu'à 2030**, pour suivre la trajectoire de décarbonation du réseau britannique :

| Année | very low | low | moderate | high | very high |
|---|---|---|---|---|---|
| 2017 | 0–99 | 100–199 | 200–299 | 300–399 | 400+ |
| 2024 | 0–34 | 35–109 | 110–189 | 190–270 | 270+ |
| **2026** | **0–24** | **25–89** | **90–169** | **170–230** | **230+** |
| 2028 | 0–14 | 15–69 | 70–149 | 150–190 | 190+ |
| 2030 | 0–4 | 5–49 | 50–129 | 130–150 | 150+ |

Ce ne sont donc **ni des percentiles**, ni des seuils physiques : une convention nationale, datée, qui rend le même kWh « moderate » en 2024 et « high » en 2028. Transposer ce *calendrier* à la France n'aurait pas de sens (trajectoire différente, réseau déjà décarboné) ; en revanche l'**ordre de grandeur** des bandes 2026–2027 (very low < 20–25, low jusqu'à ~90, very high > 200–230) est un repère utile : un index français calé sur des seuils voisins restera lisible par quiconque connaît l'index britannique.

**Ce que le produit a déjà** : la carte `/hydrogene` (ADR-0029 §5) classe les régions en cinq couleurs sur `<25, 25-45, 45-70, 70-100, ≥100` — des bornes choisies pour **l'étalement visuel** de la rampe bleue sur le régional `acv-ademe`, pas pour leur signification. Elles ne sont sourcées nulle part et ne sont pas exposées par l'API. Deux échelles qualitatives dans le même produit (carte ≠ API) seraient une source de confusion.

**Contraintes de conception** :

- Le domaine (`carbonfr-core`) est pur : l'index est un **calcul additif** sur une `CarbonIntensity`, sans IO ; ses seuils sont une **constante de domaine versionnée** comme les facteurs ACV (ADR-0008) — immuable une fois publiée, toute révision = nouvelle version + nouvel ADR (ADR-0005).
- Contrat `/v1` additif (ADR-0007) : un champ **ajouté**, jamais un champ modifié. Les SDK le portent en optionnel (un serveur ≤ 0.10 ne l'envoie pas).
- `rte-direct` (direct, national) et `acv-ademe` (cycle de vie, national + 12 régions, **basé production**) sont dans la même unité ; en France l'écart entre les deux au national est de 5 à 10 g (médianes 2025 : 14 vs 20). Un index par méthodologie doublerait la surface (deux barèmes à expliquer) pour un gain nul.

### Ce que disent les données

Deux jeux locaux, reconstruits le 2026-10-20 par les sous-commandes du serveur (exports de masse ODRÉ, millésime consolidé, pas de quota prod consommé) :

- **national**, `rte-direct` et `acv-ademe@1`, pas 30 min, 2023-01-01 → 2026-07-03 (58 362 points par méthode, base de dev locale) ;
- **régional** `acv-ademe@1`, 12 régions, 2025-01-01 → 2026-06-30 (26 205 points par région, base jetable `carbonfr_adr`, `CARBONFR_BACKFILL_SCOPE=regional`).

Requêtes : `percentile_cont(p) WITHIN GROUP (ORDER BY intensity)` et `avg((intensity < s)::int)` sur `measurement`, par méthode/année/région — rejouables tel quel sur la base de prod.

**National — percentiles (gCO₂eq/kWh)** :

| Méthode | Année | p10 | p25 | **p50** | p75 | p90 | p99 | moyenne | max |
|---|---|---|---|---|---|---|---|---|---|
| rte-direct | 2023 | 12 | 18 | **26** | 36 | 56 | 79 | 29,7 | 93 |
| rte-direct | 2024 | 8 | 9 | **15** | 24 | 39 | 54 | 19,0 | 69 |
| rte-direct | 2025 | 8 | 9 | **14** | 21 | 34 | 50 | 17,3 | 58 |
| rte-direct | 2026 (→ 3/7) | 8 | 13 | **15** | 25 | 39 | 54 | 19,8 | 61 |
| acv-ademe | 2023 | 17,4 | 22,4 | **30,2** | 41,4 | 59,8 | 82,6 | 34,2 | 98,3 |
| acv-ademe | 2024 | 11,0 | 13,9 | **19,8** | 26,8 | 42,9 | 59,8 | 23,1 | 75,8 |
| acv-ademe | 2025 | 10,5 | 14,6 | **20,0** | 26,9 | 39,2 | 56,2 | 22,5 | 66,2 |
| acv-ademe | 2026 (→ 3/7) | 13,1 | 16,2 | **21,5** | 29,5 | 45,3 | 62,1 | 24,9 | 68,9 |

Lecture : hors 2023 (parc nucléaire en corrosion sous contrainte), le national vit entre 8 et 25 g les trois quarts du temps, et **ne dépasse jamais 100 g**. La saisonnalité est nette (médiane `rte-direct` 2025 : 25–29 g en janvier-février, 8–11 g d'avril à octobre, 19 g en novembre-décembre). Les quintiles 2025 de `rte-direct` sont **9 · 10 · 14 · 21** : une distribution écrasée sur quelques grammes.

**Régional `acv-ademe` (2025-01 → 2026-06), percentiles** :

| Région | p10 | p25 | **p50** | p75 | p90 | p99 | max |
|---|---|---|---|---|---|---|---|
| Centre-Val de Loire | 6,2 | 6,6 | **7,8** | 9,3 | 11,5 | 14,2 | 18,4 |
| Auvergne-Rhône-Alpes | 6,0 | 7,2 | **9,5** | 14,6 | 17,8 | 21,6 | 32,6 |
| Occitanie | 5,9 | 7,2 | **10,5** | 22,5 | 31,8 | 41,1 | 48,6 |
| Nouvelle-Aquitaine | 7,6 | 8,9 | **13,3** | 20,7 | 28,4 | 37,0 | 43,9 |
| Normandie | 10,5 | 12,5 | **14,4** | 17,6 | 21,6 | 28,5 | 36,8 |
| Grand Est | 9,6 | 11,6 | **15,0** | 26,8 | 55,3 | 80,1 | 95,0 |
| Hauts-de-France | 16,7 | 21,5 | **30,5** | 46,6 | 68,1 | 113,1 | 314,2 |
| Provence-Alpes-Côte d'Azur | 14,9 | 24,3 | **40,0** | 71,0 | 156,2 | 226,0 | 296,5 |
| Pays de la Loire | 9,2 | 20,0 | **46,2** | 95,5 | 205,4 | 337,2 | 385,9 |
| Bourgogne-Franche-Comté | 10,9 | 30,3 | **48,5** | 65,9 | 94,8 | 158,2 | 214,7 |
| Île-de-France | 19,4 | 25,6 | **54,8** | 180,0 | 282,2 | 348,3 | 380,6 |
| Bretagne | 10,1 | 24,3 | **58,0** | 140,5 | 231,9 | 341,1 | 380,1 |

Lecture : l'échelle régionale va de 6 à 386 g. Les régions importatrices (Île-de-France, Bretagne, Pays de la Loire, PACA) montent très haut parce que **`acv-ademe` est basé production** (ADR-0008) : quand elles produisent, c'est avec leurs centrales gaz/charbon de pointe (Cordemais, Montoir, Vitry) ; leur consommation réelle est majoritairement importée du nucléaire voisin. L'index régional hérite de ce caveat, déjà affiché par la carte (« couleur régionale : signal réseau, pas une éligibilité du site »).

## Décision

### 1. Un index à cinq bandes, **absolu**, un seul barème, seuils **sourcés** : `20 · 64,8 · 100 · 200`

| Bande | Intervalle (gCO₂eq/kWh) | Ancrage |
|---|---|---|
| `very-low` | **< 20** | Mix dominé par le nucléaire, l'hydraulique et l'éolien (facteurs ACV ADEME 6 / 4 / 7,3 g, ADR-0008) ; c'est l'état normal du national (médiane 2024-2025 : 14-15 g en direct, 20 g en ACV) |
| `low` | 20 ≤ x **< 64,8** | **18 gCO₂eq/MJ** : seuil d'intensité de la zone de dépôt sous lequel l'électricité prélevée sur le réseau compte comme **pleinement renouvelable** pour l'hydrogène RFNBO (règlement délégué (UE) 2023/1184, art. 4 §2) — 18 × 3,6 = 64,8 g/kWh, le seuil déjà au cœur de la couche éligibilité (ADR-0025/0026). Au-dessous, un électrolyseur raccordé au réseau produit du RFNBO sans PPA dédié |
| `moderate` | 64,8 ≤ x **< 100** | **100 gCO₂e/kWh** (cycle de vie) : seuil de contribution substantielle de la **taxonomie européenne** pour la production d'électricité (règlement délégué (UE) 2021/2139, annexe I, critères des activités 4.x d'électricité ; repris par 2022/1214 pour le gaz et le nucléaire). Entre les deux seuils réglementaires : plus « renouvelable » au sens RFNBO, encore « durable » au sens taxonomie |
| `high` | 100 ≤ x **< 200** | Au-delà de la taxonomie ; 200 g ≈ la **moitié du facteur gaz CCGT** (406 g ACV ADEME) : un mix où environ un kWh sur deux vient du gaz |
| `very-high` | **≥ 200** | Mix majoritairement fossile (gaz de pointe, charbon, fioul : 406 / 1 038 / 778 g) |

Bornes **inférieures incluses, supérieures exclues** (`[a, b[`), comme les classes de la carte. Les seuils sont **des constantes de domaine, version `index@1`** (`carbonfr_core::domain::IntensityIndex`, constante `IntensityIndex::THRESHOLDS_V1`), immuables : les faire évoluer = `index@2` + nouvel ADR, exactement comme `acv-ademe@2` a été un nouvel ADR (ADR-0010). Pas de révision annuelle programmée (contrairement à NESO) : des seuils réglementaires ne bougent qu'avec les textes qui les fondent — c'est précisément ce qui les rend **défendables** face à la question de la neutralité : aucun d'eux n'est un choix éditorial de carbon-fr.

**Ce que l'index dirait, avec ces seuils** (parts du temps, mêmes jeux que ci-dessus) :

| Périmètre | very-low | low | moderate | high | very-high |
|---|---|---|---|---|---|
| National `rte-direct` 2023 | 30,2 % | 64,5 % | 5,4 % | 0 | 0 |
| National `rte-direct` 2024 | 62,2 % | 37,8 % | 0,0 % | 0 | 0 |
| National `rte-direct` 2025 | 69,7 % | 30,3 % | 0 | 0 | 0 |
| National `acv-ademe` 2023 | 16,8 % | 75,7 % | 7,5 % | 0 | 0 |
| National `acv-ademe` 2024 | 51,2 % | 48,6 % | 0,2 % | 0 | 0 |
| National `acv-ademe` 2025 | 49,9 % | 50,1 % | 0,0 % | 0 | 0 |
| 12 régions `acv-ademe` 2025-26 (pondéré par points) | 50,7 % | 32,8 % | 5,2 % | 7,1 % | 4,2 % |
| Centre-Val de Loire | 100 % | 0 | 0 | 0 | 0 |
| Auvergne-Rhône-Alpes | 97,3 % | 2,7 % | 0 | 0 | 0 |
| Normandie | 85,1 % | 14,9 % | 0 | 0 | 0 |
| Nouvelle-Aquitaine | 73,4 % | 26,6 % | 0 | 0 | 0 |
| Occitanie | 71,3 % | 28,7 % | 0 | 0 | 0 |
| Grand Est | 65,4 % | 29,0 % | 5,6 % | 0 | 0 |
| Hauts-de-France | 19,9 % | 68,3 % | 9,7 % | 1,9 % | 0,2 % |
| Bourgogne-Franche-Comté | 18,0 % | 56,1 % | 17,5 % | 8,3 % | 0,1 % |
| Provence-Alpes-Côte d'Azur | 19,3 % | 54,3 % | 8,1 % | 15,2 % | 3,1 % |
| Pays de la Loire | 25,0 % | 42,6 % | 8,4 % | 13,6 % | 10,4 % |
| Bretagne | 21,7 % | 32,2 % | 12,0 % | 19,6 % | 14,5 % |
| Île-de-France | 11,9 % | 38,3 % | 1,2 % | 26,1 % | 22,5 % |

Ce tableau est le **test de neutralité** de la proposition : le national est `very-low` ou `low` au moins 99,8 % du temps hors 2023 — c'est vrai, et c'est ce que dit RTE de son réseau ; 2023 apparaît comme l'année dégradée qu'elle a été (un barème relatif l'aurait gommée) ; la différence direct / cycle de vie se voit (le `very-low` passe de 70 % à 50 % en ACV) sans que le barème change ; et les régions à production fossile de pointe sont nommées comme telles, avec le caveat « basé production » déjà porté par la carte. Les bandes `high`/`very-high` existent pour le régional aujourd'hui, et pour toute source future (import, comparaison européenne) — un index tronqué à trois bandes « parce que la France n'y va jamais » serait un index fait pour la France seulement.

### 2. Exposition : champ additif `index` + catalogue

- **`index` (chaîne)** ajouté, **jamais absent** sur les réponses qui portent une intensité instantanée ou ponctuelle : `GET /v1/intensity/now` et `/now/all` (chaque mesure), `/v1/intensity/date` (chaque point, et la colonne `index` en dernière position du CSV de la PR #152, après `vintage`), `/v1/intensity/forecast` (chaque point, calculé sur `expected`), **flux SSE** `/v1/intensity/stream` (chaque événement). Pas sur `/v1/intensity/stats` (des agrégats : l'index d'une moyenne n'est pas la moyenne des index) ni sur `/v1/intensity/greenest-window` (la fenêtre répond déjà à la question « quand ») — ajoutables plus tard, additivement, si un usage le réclame.
- **Libellés** : `very-low`, `low`, `moderate`, `high`, `very-high` (point 2 de l'encadré) — kebab-case comme tous les identifiants de l'API (`rte-direct`, `acv-ademe`, slugs de région) ; l'OpenAPI les déclare en `enum` (comme `region` depuis la 0.10.0). La correspondance littérale avec NESO (`very low` avec espace) tient en une ligne de doc.
- **`GET /v1/intensity/index`** (point 3) : catalogue du barème — `version` (`1`), les cinq bandes avec `label`, `min`, `max` (nullable aux extrémités), `unit`, et pour chaque borne sa `source` (texte et URL) ; `Cache-Control: public` comme les autres catalogues. Rôle identique à `/v1/factors` (ADR-0010 §7) : **la légende est auditable**, et un client (dont la carte `/hydrogene`) la lit au lieu de la coder en dur.
- **SemVer** : champ + route ajoutés → version applicative **minor** (**v0.11.0**, ADR-0019) ; **SDK TypeScript** : `index?: IntensityIndex` optionnel sur les types concernés (serveur ≤ 0.10 : absent) + méthode `intensityIndex()` → **`sdk-v0.5.0`** (la 0.4.0 est prise par la reconnexion automatique, PR #153) ; **SDK Rust** : champ public ajouté sur des types `pub` = *minor* obligatoire (ADR-0030 §3) → **`carbonfr-sdk` 0.3.0**, `IntensityIndex` en `#[non_exhaustive]` des deux côtés (une bande ajoutée un jour ne casserait pas les `match`).

### 3. Domaine (`carbonfr-core`)

```rust
/// Index qualitatif d'intensité (ADR-0033), barème `index@1`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntensityIndex { VeryLow, Low, Moderate, High, VeryHigh }

impl IntensityIndex {
    pub const VERSION: u32 = 1;
    /// Bornes supérieures exclues de `very-low`, `low`, `moderate`, `high`.
    pub const THRESHOLDS_V1: [f64; 4] = [20.0, 64.8, 100.0, 200.0];
    pub fn of(intensity: CarbonIntensity) -> Self { /* première borne > valeur */ }
    pub fn slug(self) -> &'static str { /* "very-low" … */ }
}
```

Calcul pur, testé sur chaque borne (valeur = borne → bande supérieure ; `[a, b[`), sur 0 et sur une valeur énorme ; `Measurement`, `ForecastPoint`, `IntensityUpdate` ne changent pas (l'index se **dérive** à la projection, dans les DTO de l'adapter HTTP — il n'est pas stocké : rien en base, rien à migrer, et un `index@2` se servirait sur tout l'historique sans réécriture).

### 4. Carte `/hydrogene` (point 4)

La légende de la carte adopte les cinq bandes de l'index (couleurs inchangées, bornes `20 · 64,8 · 100 · 200`, libellés affichés) et lit `/v1/intensity/index` au chargement plutôt que la constante `CLASSES`. Conséquence visuelle assumée : moins de nuances au sein des régions nucléaires (Centre-Val de Loire, ARA, Normandie : toutes `very-low`), plus de contraste pour les régions de pointe fossile — c'est l'information, pas l'esthétique, qui prime (ADR-0029 §5 révisé par cet ADR).

## Conséquences

**Positives** : un libellé lisible par tous, repris sans effort par le no-code ; parité d'usage avec l'index britannique ; des seuils qu'on peut **défendre ligne par ligne** (deux textes européens, les facteurs ADEME déjà publiés par `/v1/factors`) ; honnêteté sur le niveau français ; la même échelle sur l'API, la carte et les SDK ; aucune donnée stockée, aucune migration.

**Négatives / limites, à documenter dans le README et l'OpenAPI** :

- Un index absolu **n'aide pas à choisir l'heure** en France : le national sera `very-low` ou `low` presque toujours. Pour le *timing*, l'API a déjà `/v1/intensity/forecast`, `/greenest-window`, `/below` et `/schedule` — le README le dit explicitement, pour qu'on ne demande pas à l'index ce qu'il ne mesure pas.
- Le régional en `acv-ademe` est **basé production** : une région importatrice peut être `very-high` quand ses centrales de pointe tournent alors que sa consommation est décarbonée (ADR-0008). Le caveat de la carte passe dans la doc de l'index.
- 64,8 n'est pas rond : c'est le prix de la source exacte (18 gCO₂eq/MJ). Le catalogue l'explique ; l'arrondi à 65 est la variante du point 1.

**Engagements** (PR d'implémentation, après fusion de cet ADR) : type `IntensityIndex` + tests dans `core` ; DTO `now`/`now/all`/`date` (JSON + CSV)/`forecast`/`stream` + catalogue ; OpenAPI (`enum`, nouvelle route, snapshot) ; SDK TS 0.5.0 et Rust 0.3.0 (types ; tests : parité OpenAPI et REST côté Rust, REST et flux côté TS — la parité OpenAPI du SDK TS reste l'item DX-3 d'I10) ; Bruno (`intensity-index.bru`) ; README (« Index qualitatif ») ; carte `/hydrogene` ; CHANGELOG ; plan I9. Le `methodologies` de `/v1/methodologies` n'est pas touché : l'index n'est pas une méthodologie (il ne produit pas de gCO₂eq/kWh), c'est une **lecture** d'une intensité, quelle que soit la méthode. Tenus le 2026-10-20 (PR #156).

## Alternatives envisagées

1. **Barème relatif (percentiles de l'historique national, façon « quintiles »)** — écartée. Les quintiles 2025 de `rte-direct` sont 9 · 10 · 14 · 21 g : l'index basculerait de `very-low` à `high` pour 12 g d'écart, et dirait `high` à 21 g là où l'Allemagne est à 300 — illisible et trompeur hors de France. Il gommerait aussi les années anormales (2023 serait « normale » dans son propre barème) et imposerait une révision annuelle, donc un index non comparable d'une année à l'autre.
2. **Calendrier décroissant façon NESO** — écartée. Pertinent pour un réseau qui se décarbone de 400 à 50 g en treize ans ; sans objet pour un réseau à 15 g de médiane. Et un barème qui change chaque 1er janvier contredit « versions portées par la donnée, immuables » (ADR-0005).
3. **Classes actuelles de la carte (`25 · 45 · 70 · 100`)** — écartée comme barème d'API. Choisies pour l'étalement des couleurs sur le régional (ADR-0029 §5), elles n'ont aucune source ; publiées, elles devraient être défendues, et ne le pourraient pas. Elles restent la variante du point 1 si Morgan préfère la continuité visuelle de la carte.
4. **Un barème par méthodologie** (`rte-direct` ≠ `acv-ademe`) — écartée. Même unité, écart de 5 à 10 g au national ; deux barèmes = deux explications, deux catalogues, et des comparaisons impossibles entre `/now?methodology=rte-direct` et `acv-ademe`. Le barème unique est la règle ; les seuils réglementaires s'entendent d'ailleurs en cycle de vie, ce qui joue en faveur de `acv-ademe` sans pénaliser `rte-direct` (plus bas).
5. **Index numérique (1 à 5) ou double (chaîne + nombre)** — écartée. Un entier invite à des moyennes sans sens ; la chaîne est ce que le no-code consomme. Un client qui veut un ordre lit le catalogue (`min`/`max`).
6. **Ne pas servir d'index** (risque prévu par le plan) — reste l'issue si le point 1 est jugé indéfendable ; cet ADR passerait alors en *Déprécié* avec la raison, et l'item I9 serait clos « par l'ADR ».

## Sources

- NESO, *Carbon Intensity Forecast Methodology* (national), édition de mai 2024, table 2 « Numerical bands for the Carbon Intensity Index » — PDF lié depuis [carbonintensity.org.uk](https://carbonintensity.org.uk/) (`api.neso.energy`), relu le 2026-10-20.
- Règlement délégué (UE) 2023/1184 de la Commission (RFNBO), article 4, paragraphe 2 : seuil de 18 gCO₂eq/MJ d'intensité de la zone de dépôt.
- Règlement délégué (UE) 2021/2139 (taxonomie climat), annexe I : seuil de 100 gCO₂e/kWh (cycle de vie) pour la production d'électricité ; complété par 2022/1214.
- Facteurs ACV ADEME `acv_ademe_v1` (`crates/core/src/domain/factors.rs`, ADR-0008) : nucléaire 6, hydraulique 4, éolien 7,3, solaire 55, bioénergies 24, gaz 406, fioul 778, charbon 1 038 gCO₂eq/kWh.
- Données : exports de masse ODRÉ (éCO2mix consolidé, national 2023-01 → 2026-07 ; régional 2025-01 → 2026-06), rejoués localement le 2026-10-20 par `carbonfr-server backfill`.
