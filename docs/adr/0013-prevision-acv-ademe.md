# ADR-0013 — Prévision `acv-ademe` : prévoir les entrées, puis appliquer le calculateur

- **Statut** : Accepté (mise en œuvre **engagée** — pipeline + baseline climatologique livrés ; ML & day-ahead à venir)
- **Date** : 2026-06-15
- **Consolide** : ADR-0010 (méthode `acv-ademe`) × ADR-0012 (modèle ML) ; s'appuie sur le contrat ADR-0011

## État d'implémentation (2026-06-16)

**Tranche A — pipeline « prévoir les entrées → appliquer le calculateur » +
baseline climatologique : livrée.** C'est exactement le **baseline** du §7 (mix
saisonnier + imports climatologiques), et la **preuve du pipeline** §1-2.

- fonction pure `acv_ademe_forecast` (`crates/core/src/domain/forecast_acv.rs`) :
  prévoit **chaque entrée par canal** (8 filières du mix, flux + intensité de
  chaque voisin) avec la formule de `climatology@1` (climatologie
  horaire-de-semaine + biais de persistance décroissant), réassemble
  `GenerationMix` + `CrossBorderFlows` par créneau, puis applique le **même**
  calculateur `AcvAdemeConsumption` (ADR-0010) ;
- **invariant de convergence au nowcast garanti et testé** (§1) : à horizon → 0,
  chaque canal vaut sa dernière observation → entrées prévues = entrées observées
  → prévision = nowcast ;
- intervalle : dispersion empirique par créneau de la série `@2` dérivée de
  l'historique (la calibration par quantiles de résidus par horizon, §6, viendra
  derrière le même contrat) ;
- adapter `AcvAdemeForecaster<R, C>` (lit mix `@1` via `IntensityRepository` +
  contexte d'import via `CrossBorderRepository`, délègue au calcul pur) ;
- **routage par méthode** au composition root (§5) : `ForecastState` porte un
  modèle `@2` optionnel (dispatch dynamique) ; servi via
  **`GET /v1/intensity/forecast?methodology=acv-ademe&version=2`** (national).

**Tranche B — backtest & calibration `acv-ademe@2` (§6-7) : livrée.** Cas d'usage
`BacktestConsumptionForecast` : la vérité `@2` **n'étant pas stockée**, elle est
**dérivée** de l'observé (mix `@1` + contexte d'import via
`derive_consumption_series`) une fois sur toute la fenêtre, puis comparée à la
prévision (walk-forward, anti-fuite, vs persistance). Sous-commande
**`carbonfr-server backtest-acv`** (MAE/RMSE global + par horizon). Intervalles
**calibrés par quantiles de résidus par horizon** (`calibrate_bands`) et
**auto-calibrés au démarrage** (`AcvAdemeForecaster::with_bands`,
`CARBONFR_FORECAST_CALIBRATE_WEEKS`, repli dispersion par créneau).

**À venir :** `MixForecaster` **GBDT multi-sorties** (§2) et
`CrossBorderForecastSource` **ENTSO-E day-ahead ⊕ proxy** (§3-4) — la tranche A
tient lieu de proxy (climatologie du contexte d'import stocké), pas encore de
day-ahead. Le GBDT n'est livré que s'il bat ce baseline **au `backtest-acv`**
(garde de promotion, cohérent ADR-0012).

## Contexte

L'ADR-0010 a défini `acv-ademe` comme une intensité **consumption-based**, calculée par une **stratégie de domaine pure** (`AcvAdeme`) à partir d'entrées : mix FR, flux d'import par frontière, intensité de chaque voisin. L'ADR-0012 a livré un modèle ML qui prévoit l'**intensité scalaire** pour `rte-direct`. L'ADR-0011 a fixé le contrat `ForecastPoint` (qui **porte déjà `methodology`**) et la garde de backtest.

Le **trou** : prévoir l'intensité `acv-ademe` future revient en partie à **prévoir des réseaux étrangers** (l'intensité du voisin à l'heure de l'import). Ni 0008 ni 0010 ne le traite. C'est la seule dépendance qui croise les deux axes.

Cadrage retenu : **prévoir les entrées puis appliquer le calculateur `AcvAdeme`** ; intensités voisines = **ENTSO-E day-ahead** dans l'horizon, **proxy** au-delà.

## Décision

### 1. On prévoit les entrées, le calculateur reste l'unique source de vérité

Le **même** `MethodologyCalculator::AcvAdeme` (ADR-0010) s'applique aux entrées **prévues** comme aux entrées **observées**. Il est **agnostique au régime** (observé / prévu) — c'est sa propriété clé. Conséquences :

- la prévision **hérite de la version de méthode** et reste **auditable** (pas de boîte noire) ;
- elle **converge vers le nowcast `acv-ademe`** quand l'horizon → 0 (invariant **testable**, à garantir).

### 2. Le modèle prévoit le **mix**, pas un scalaire

Le chemin `acv-ademe` requiert un **`MixForecaster`** : une variante **multi-sorties** du GBDT de l'ADR-0012 (mêmes features dont météo, **même principe anti-fuite**), prévoyant le **vecteur mix par filière**. Tout-Rust, derrière le port. La forme du pipeline :

```
MixForecaster (mix FR prévu) ─────────────┐
                                          ├──> AcvAdeme (calculateur pur) ──> ForecastPoint
CrossBorderForecastSource (import prévu) ──┘        (ADR-0010)                 (ADR-0011)
```

### 3. Les entrées prévues, par source

- **Mix FR prévu** : `MixForecaster` (ci-dessus).
- **Flux d'import prévus** : **donnée** ENTSO-E day-ahead (échanges commerciaux programmés) dans l'horizon ; **proxy** (profil saisonnier de flux) au-delà.
- **Intensités voisines prévues** : ENTSO-E day-ahead (génération vent/solaire + charge **par pays** → intensité dérivée via les facteurs du pays) dans l'horizon ; **proxy climatologique** (profil-type d'intensité par pays, heure × type de jour, depuis l'historique des intensités voisines) au-delà.

### 4. Un port `CrossBorderForecastSource` (parallèle au `CrossBorderSource` observé)

De même que l'observé et le prévu ont exigé des types distincts (ADR-0011), le **contexte d'import prévu** est distinct du contexte observé (ADR-0010). On introduit un port sortant `CrossBorderForecastSource`, implémenté par un **adapter composite** : ENTSO-E day-ahead **⊕** proxy au-delà. Le proxy lit l'historique des intensités voisines (rollups). Jamais appelé par requête utilisateur — le poller ingère le day-ahead.

### 5. Routage par méthode au composition root

`ForecastPoint` porte déjà `methodology`. On câble **un `ForecastModel` par méthode** au composition root (chaque adapter reste à responsabilité unique) : `rte-direct` → le GBDT scalaire (ADR-0012) ; `acv-ademe` → le pipeline composé ci-dessus. Le cas d'usage sélectionne l'instance liée à la méthode demandée. Le **port garde sa forme** `forecast(region, from, horizon)`.

### 6. Honnêteté sur l'horizon

Au-delà du day-ahead, les entrées dégradent vers le **proxy** → intervalles `acv-ademe` plus larges que `rte-direct`. **Aucun traitement spécial** : la méthode d'intervalles (quantiles empiriques de résidus par horizon, ADR-0011) **exprime automatiquement** cette dégradation — le backtest fera apparaître la « falaise » day-ahead → au-delà sous forme de quantiles plus larges.

### 7. Garde de backtest, anti-fuite côté imports

Le pipeline complet est backtesté sur des **entrées telles que disponibles** (principe ADR-0012, étendu aux imports) : mix prévu par le modèle, contexte d'import day-ahead **tel que publié**, proxy au-delà — **jamais** le contexte d'import observé. Garde de promotion : la prévision `acv-ademe` doit battre son **baseline** (calculateur `AcvAdeme` appliqué à un mix saisonnier + imports climatologiques) sur MAE et couverture.

### 8. Périmètre

**National.** L'`acv-ademe` régional était déjà reporté (ADR-0010) ; sa prévision l'est *a fortiori*.

## Conséquences

- **Unification** : le calculateur de méthode devient le **seul** endroit où l'intensité est calculée, observée comme prévue. La prévision hérite du versionnement et de l'auditabilité — le moat tient jusque dans la prévision.
- **Domaine** : `AcvAdeme` réutilisé **tel quel** (sa propriété d'agnosticisme au régime suffit). Aucun nouveau type domaine : un « mix prévu » est un `GenerationMix` à un horodatage futur, `CrossBorderFlows` existe déjà (ADR-0010). Le cas d'usage de prévision devient **paramétré par la méthode** (routage).
- **Infra** : nouveau `MixForecaster` (GBDT multi-sorties, tout-Rust) ; nouveau port `CrossBorderForecastSource` + adapter composite (ENTSO-E day-ahead ⊕ proxy) ; le poller ingère le day-ahead ENTSO-E ; backtest étendu à `acv-ademe`.
- **Pipeline le plus lourd du projet** : prévision d'un vecteur + composition de prévisions de grilles étrangères, intervalles les plus larges, dépendance à la fraîcheur/dispo du day-ahead ENTSO-E.

## Alternatives envisagées

- **Prévoir directement le scalaire `acv-ademe`** : plus simple, mais **boîte noire** — la valeur prévue n'est pas garantie cohérente avec le calculateur, ne converge pas au nowcast, casse l'auditabilité. Écarté (la fourche).
- **Contexte d'import observé en entraînement/backtest** : fuite de données côté imports → performance surévaluée. Écarté fermement.
- **Proxy seul pour les intensités voisines** : plus simple, mais jette le meilleur signal de court terme, là où se prennent la plupart des décisions carbon-aware (prochaines 24 h). Écarté au profit de day-ahead ⊕ proxy.
- **Modèles complets par pays** : hors périmètre, très ultérieur.

## Questions ouvertes (implémentation — n'impactent pas le principe)

- **Faut-il router `rte-direct` via le `MixForecaster`** (uniformité : un seul modèle, le calculateur fait le reste) plutôt que le scalaire direct de l'ADR-0012 ? → **tranché par le backtest** (laisser la précision décider).
- Granularité et profondeur d'historique du proxy climatologique (pays × heure × type de jour).
- Latence/fraîcheur du day-ahead ENTSO-E vs cadence du poller.
- Forme de sérialisation du `MixForecaster` multi-sorties.
