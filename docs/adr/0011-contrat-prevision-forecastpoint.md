# ADR-0011 — Contrat de prévision : `ForecastPoint` (intervalles) + `StatForecaster`

- **Statut** : Proposé
- **Date** : 2026-06-15
- **Raffine** : ADR-0009 (modèle climatologie livré — **re-type le contrat** du port `ForecastModel`, aujourd'hui `Vec<Measurement>`) ; roadmap §3 (phase 3 — prévision)

## Contexte

La prévision d'intensité est la **valeur créée** du service (ARCHITECTURE §2) : elle n'existe pas à la source. Le port `ForecastModel` existe, `FindGreenestWindow` le consomme, et `greenest_window()` (fonction **pure** du domaine) sélectionne le créneau.

Mais le port renvoie aujourd'hui un `Vec<Measurement>`. Or :

- un `Measurement` est une **observation révisée**, porteuse d'un `vintage` (`tr`/`consolidated`/`definitive`, ADR-0006) ; une prévision **n'a pas de millésime** — c'est une prédiction ;
- `Measurement` n'a **aucun emplacement** pour exprimer une **incertitude**.

Réutiliser `Measurement` pour la prévision détourne donc `vintage` (les tests mettent `Tr` faute de mieux) et interdit les intervalles de confiance. Comme l'incertitude est une propriété **de premier ordre** d'une prévision honnête — et que la phase 4 va **figer le contrat** dans le SDK + l'OpenAPI — il faut corriger le contrat **maintenant**, avant qu'il ne devienne public.

Décisions de cadrage (cette itération) : intervalles **dans le contrat v1** ; modèle = **`StatForecaster` amélioré** (pas de ML à ce stade) ; périmètre `rte-direct`, national.

## Décision

### 1. Un type de domaine dédié `ForecastPoint`

Distinct de `Measurement`. La forme — le **contrat**, pas l'implémentation :

```rust
struct ForecastPoint {
    at: OffsetDateTime,        // début du pas quart d'heure
    region: Region,
    expected: CarbonIntensity, // estimation centrale
    lower: CarbonIntensity,    // borne basse de l'intervalle
    upper: CarbonIntensity,    // borne haute
    methodology: Methodology,  // une prévision est faite *pour* une méthode
    model: ModelVersion,       // …et produite *par* un modèle versionné
}
```

Invariant garanti à la construction : `lower ≤ expected ≤ upper`. **Pas de `vintage`** (aucun millésime pour une prédiction), **pas de `mix`** (hors périmètre v1). On introduit un `ModelVersion { id, version }`, sur le modèle de `Methodology` : une prévision dit **quel modèle** l'a produite (reproductibilité, honnêteté).

### 2. Le port `ForecastModel` renvoie des `ForecastPoint`

`forecast(region, from, horizon) -> Result<Vec<ForecastPoint>, ForecastError>` (async inchangé). C'est un changement de signature **du port** — donc des adapters — pas des cas d'usage, au-delà du retypage.

### 3. `greenest_window()` opère sur `ForecastPoint`

Par défaut sur `expected`. **Option** (versionnée dans la signature) : optimiser sur `upper` → un créneau vert **prudent** (« au pire, l'intensité ne dépassera pas »), utile pour un engagement. Reste une fonction **pure** du domaine.

### 4. `StatForecaster` amélioré (dans un adapter, zéro IO dans `core`)

Pas de ML. Un modèle **saisonnier** exploitant la forte périodicité jour/semaine de l'intensité :

- analogues historiques par **créneau horaire × type de jour** (ouvré / week-end / férié), pondérés par récence — c'est l'usage prévu des **vues de rollup** (« pour les statistiques et le modèle », ARCHITECTURE §5) ;
- **ajustement par la consommation prévue RTE** (J-1 / J), déjà présente dans le jeu temps réel ODRÉ (ARCHITECTURE §2) → **input gratuit, sans nouvelle source** : la charge anticipée prédit le recours au gaz/charbon à la marge.

### 5. Intervalles par quantiles empiriques de résidus

Les bornes `lower`/`upper` ne viennent **pas** d'une hypothèse gaussienne, mais des **quantiles empiriques de l'erreur historique, par horizon**, mesurés en backtest. Conséquence directe et souhaitable : l'intervalle **s'élargit avec l'horizon**, sans calibrage arbitraire.

### 6. Backtesting = source des intervalles, pas un bonus

Un harnais de **validation *walk-forward*** sur l'historique (consolidé/définitif, 2012→) produit : MAE / RMSE par horizon, **taux de couverture** des intervalles, et les **quantiles de résidus** qui alimentent le point 5. Hors `core`, hors chemin chaud. La **précision mesurée est publiée** (MAE par horizon), façon `/factors` — la transparence fait la crédibilité.

### 7. Périmètre & surface API

- v1 : `rte-direct`, **national**, `StatForecaster`.
- La **prévision `acv-ademe`** (imports prévus + intensités voisines prévues) **se couple à l'axe 1** → reportée.
- Le **ML** reste un **futur adapter** derrière le même port ; le choix de *runtime* (Rust pur vs ONNX/`tract`) **n'est pas tranché ici**.
- `GET /v1/intensity/forecast` renvoie désormais `expected` + `lower`/`upper` + `model`, et expose le **niveau de confiance**.
- `GET /v1/greenest-window` gagne un sélecteur optionnel d'estimateur (central / prudent), **défaut central**.

## Conséquences

- **Timing** : ce n'est **pas** du post-phase 4 — c'est une **correction de contrat de phase 3 à boucler *avant* la phase 4**. Sinon le SDK/OpenAPI figent un `/forecast` sans incertitude, et le corriger devient un **breaking change** du `/v1` public.
- **Domaine** : ajout de `ForecastPoint` et `ModelVersion` ; `ForecastModel` retypé ; `greenest_window` / `FindGreenestWindow` retypés. Toujours **zéro IO** dans `core`.
- **Infra** : nouvel adapter `StatForecaster` lisant les rollups ; harnais de backtest (offline) produisant les quantiles ; **pas de nouvelle source** (conso RTE déjà ingérable via l'adapter ODRÉ).
- **Gouvernance** : une prévision porte `model` + `methodology` ; tout changement de modèle = **bump de `ModelVersion`**, exposé. Jamais de changement silencieux.
- **Coût** : mapping `ForecastPoint` ↔ DTO d'adapter ; infra de backtest à écrire et à rejouer (idéalement déclenchée après révision de millésime, cohérent ADR-0006).

## Alternatives envisagées

- **Garder `Measurement` pour la prévision** (statu quo) : détourne `vintage`, aucun emplacement d'intervalle. Écarté — c'est précisément le défaut qu'on corrige.
- **Intervalles paramétriques (résidus gaussiens)** : plus simples, mais mal calibrés pour une erreur asymétrique. Écarté au profit des quantiles empiriques.
- **Aller directement au ML** : prématuré. Le contrat doit être juste d'abord, et un bon modèle saisonnier + charge est une **référence difficile à battre** *et* le **benchmark obligatoire** de tout futur ML. Reporté, sans dette d'architecture (*drop-in* derrière le port).
- **Point unique sans intervalle** : écarté — l'incertitude est décidée comme exigence v1.

## Questions ouvertes (implémentation — n'impactent pas le principe du contrat)

- Niveau de confiance par défaut (80 % vs 90 %), configurable et exposé.
- Cadence de recalcul des quantiles / ré-estimation, et déclenchement après révision de millésime.
- Forme du harnais de backtest (`bin/backtest` vs *bench*) et support de publication de la précision.
- Signature exacte du sélecteur d'estimateur de `greenest_window` (central / prudent).
