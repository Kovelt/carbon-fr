# ADR-0009 — Modèle de prévision d'intensité (`climatology@1`)

- **Statut** : Accepté
- **Date** : 2026-06-15

## Contexte

La phase 3 ouvre la **prévision** d'intensité carbone : répondre à « quand, dans
les prochaines heures, l'électricité sera-t-elle la moins carbonée ? » (décaler
une recharge, un lave-vaisselle, un batch de calcul). L'échafaudage du domaine
existe déjà :

- port sortant `ForecastModel::forecast(region, from, horizon) -> Vec<Measurement>` ;
- cas d'usage `FindGreenestWindow` (orchestration) ;
- fonction **pure** `greenest_window(points, durée)` (choix du créneau).

Il manque **l'implémentation du modèle** derrière le port.

### Forces en présence

- **La prévision n'existe pas à la source.** éCO2mix publie une prévision de
  **consommation** (J-1 et J), mais **ni l'intensité carbone, ni le mix de
  production complet** ne sont prévus [^eco2mix]. La prévision d'intensité est
  donc, par construction, de la **valeur créée par un modèle** (ARCHITECTURE §2).
- **On dispose déjà de l'historique complet** (backfill national depuis 2012,
  ADR-0006 ; régional en `acv-ademe`). C'est l'actif à exploiter.
- **Éthos du projet** : souverain, dev-first, **explicable**, léger, sans
  dépendance opaque ni élargissement de périmètre sans ADR. Un modèle dont on ne
  peut pas expliquer chaque point serait contraire à cette ligne.
- **L'intensité carbone française est fortement saisonnière** : cycle
  journalier (pointe matin/soir), hebdomadaire (semaine vs week-end) et annuel
  (chauffage hiver). Ces régularités sont précisément ce qu'un modèle de
  **climatologie** capture.

## Décision

Implémenter **`climatology@1`** : une **climatologie horaire-de-semaine
glissante, corrigée d'un biais de persistance décroissant**. Modèle pur,
déterministe, explicable, **sans dépendance externe**, alimenté par l'historique
déjà stocké.

### Identité et versionnement

- Le modèle de prévision est un **attribut versionné**, indépendant de la
  méthodologie carbone : on prévoit *la série d'une méthodologie donnée*
  (`rte-direct` ou `acv-ademe`) *avec* un modèle (`climatology@1`). Deux axes de
  version orthogonaux.
- Comme pour la méthodologie (ADR-0005), **jamais** de modification silencieuse :
  toute évolution de la formule ou des paramètres = nouvelle version
  (`climatology@2`, ou un autre modèle) + ADR/addendum. La réponse d'API
  **expose l'identifiant de modèle** (transparence).

### Formule

Pour une cible `(region, methodology)` à l'horodatage `t`, au pas natif **15 min** :

1. **Climatologie** `C(t)` = moyenne des intensités historiques observées au
   **même créneau de la semaine** que `t` (même couple `jour-de-semaine × heure ×
   quart`, soit 7×24×4 = 672 créneaux), sur une **fenêtre glissante de `N`
   semaines** précédant `from`. Réactive à la dérive saisonnière, tout en gardant
   assez d'échantillons.
2. **Correction de persistance** : soit `o` la dernière intensité **observée** à
   `t₀` et `b = o − C(t₀)` le biais récent. On propage ce biais en le faisant
   **décroître** avec l'horizon :

   ```
   prévision(t) = max(0,  C(t) + b · exp(−(t − t₀) / τ))
   ```

   La persistance domine la première heure (l'intensité est fortement
   autocorrélée à court terme), la climatologie domine au-delà.

3. **Créneaux UTC** (cohérence avec les rollups, ADR-0004) ; chaque point porte
   `methodology` = la méthodologie prévue.

#### Paramètres (défauts **calés par backtest** — cf. addendum)

| Paramètre | Défaut | Rôle |
|---|---|---|
| `N` (fenêtre climatologique) | **10 semaines** | compromis réactivité / nombre d'échantillons |
| `τ` (constante de décroissance) | **2 semaines** | vitesse de retour persistance → climatologie |
| pas | 15 min | natif éCO2mix (30 min sur le jeu consolidé) |
| horizon par défaut | 24 h | usage « dans la journée » |
| horizon max | 72 h | au-delà, la correction de persistance n'apporte plus rien |

> Ces valeurs ont été **calées par backtest** (addendum ci-dessous). La valeur
> initiale de τ (6 h) sous-performait la persistance ; la calibration a montré
> qu'un τ long (correction d'anomalie quasi constante sur l'horizon) est ce qui
> fait gagner le modèle.

### Dégradation gracieuse (démarrage à froid)

Si l'historique d'une cible est insuffisant (créneau jamais observé, région
récente, < quelques semaines) :

1. élargir la fenêtre à tout l'historique disponible ;
2. à défaut, retomber sur la **persistance pure** (`prévision(t) = o`) ;
3. si aucune observation n'existe : `ForecastError` (→ `InsufficientSeries` au
   niveau du cas d'usage).

### Statut des points prévus

Une prévision **n'est pas une mesure** : elle n'est **jamais persistée** dans
`measurement` (elle ne porte pas de millésime, ADR-0006 **intacte**). Elle est
**calculée à la lecture** par l'adapter et renvoyée. La distinction
observation / prévision est portée par **l'endpoint et le DTO HTTP** (un point de
`/v1/intensity/forecast` est explicitement une prévision, avec son modèle), pas
par une mutation du domaine.

### Surface d'API (sous `/v1`, ADR-0007)

- `GET /v1/intensity/forecast?region=&methodology=&horizon=` — série prévue.
- `GET /v1/intensity/greenest-window?region=&methodology=&horizon=&window=` —
  meilleur créneau bas-carbone (cas d'usage `FindGreenestWindow`, déjà écrit).

Les deux exposent l'**identifiant de modèle** (`climatology@1`) et la
méthodologie prévue. `methodology` par défaut = `rte-direct` (national) comme sur
les endpoints existants.

## Conséquences

- **Domaine (`core`)** : la formule est une **fonction pure** (entrée : tranche
  d'historique + `from`/`horizon`/paramètres ; sortie : `Vec<Measurement>`),
  **testable sans IO** — même bénéfice hexagonal que `greenest_window`. Aucune
  dépendance nouvelle dans `core`.
- **Adapter** : un nouvel adapter sortant implémente `ForecastModel` en
  **lisant l'historique via `IntensityRepository`** (port existant `range`/
  `rollup`) puis en déléguant à la fonction pure. La *composition root* y câble le
  repo Postgres concret. Pas de nouveau port.
- **API** : deux handlers sous `/v1`, nouveaux DTO marquant les points comme
  prévisions et exposant le modèle. OpenAPI/Bruno étendus en cohérence.
- **Schéma Postgres** : **inchangé** (prévisions non persistées). Pas de
  migration.
- **Backtest** : on ajoutera une procédure de backtest (held-out) publiant une
  erreur (MAE/RMSE) — la précision annoncée sera **mesurée**, jamais supposée.
- **Chemin d'amélioration** : `climatology@2` (paramètres recalés, pondération
  saison/jour férié), ou un modèle distinct dérivant les **prévisions RTE J-1**
  (consommation + ENR → dispatch → méthodologie carbone), brançable derrière le
  **même port** sans toucher au domaine ni à l'API.

## Alternatives envisagées

- **Dérivé des prévisions RTE J-1** (consommation + ENR → modèle de dispatch →
  intensité) : plus ancré sur la source, mais exige un **modèle de dispatch non
  trivial**, plusieurs jeux de données et consomme du quota. Écarté en MVP,
  **retenu comme évolution** (`forecast@2`, même port) une fois la baseline en
  place et mesurée.
- **Apprentissage automatique** (gradient boosting / réseau sur le backfill) :
  précision potentiellement supérieure, mais **opaque**, infra d'entraînement,
  poids — contraire à l'éthos explicable/léger. Réservé à un éventuel
  `forecast@3` si la baseline plafonne.
- **Persistance pure** (`prévision = dernière valeur`) : trop faible au-delà de
  ~1 h (ignore le cycle journalier) — conservée seulement comme **filet de
  démarrage à froid**.
- **Climatologie sans correction de persistance** : ignore l'état récent du
  système (météo, indispo) ; la correction décroissante la rattrape à faible coût.
- **Persister les prévisions** (avec un millésime « forecast ») : polluerait la
  série observée et la logique d'upsert/millésime (ADR-0006) sans bénéfice —
  écarté au profit du calcul à la lecture.

## Addendum — calibration mesurée (2026-06-15)

- **Statut** : Accepté
- **Méthode** : backtest *walk-forward* (`carbonfr-server backtest-sweep`) sur la
  donnée **réelle** ODRÉ consolidée/définitive (`eco2mix-national-cons-def`,
  national, `rte-direct`). ⚠️ Ce jeu est au **pas de 30 min** (le temps réel est
  à 15 min) ; le backtest a été lancé à 30 min pour aligner prévision et
  observé. Deux fenêtres mensuelles **indépendantes** (origines : 1/jour) :
  novembre 2024 et septembre 2024 (validation hors échantillon). Grille
  `N ∈ {8,10,12,14}` × `τ ∈ {3 h … 504 h}`. Métrique : RMSE global (gCO₂eq/kWh),
  comparé à une **référence de persistance**.

- **Résultat** :

  | Configuration | RMSE nov. | RMSE sept. |
  |---|---|---|
  | `τ = 6 h` (valeur initiale, N=8) | 15,3 | 6,0 |
  | **persistance nue** (référence) | 8,3 | 5,2 |
  | `N = 10`, `τ = 2 sem.` (calé) | **7,6** | **5,0** |

  La valeur initiale `τ = 6 h` **sous-performait la persistance** sur les deux
  mois. La calibration montre que l'erreur **décroît de façon monotone quand τ
  augmente** et se stabilise vers `τ ≈ 2 semaines` : sur un horizon de 24 h, la
  correction d'anomalie ne doit **quasiment pas se décroître**. Le modèle
  devient alors une **climatologie corrigée de l'anomalie courante**, qui bat la
  persistance à **tous** les horizons (h+1 : −33 % ; h+6 : −14 % ; h+24 : −10 %
  de RMSE en novembre). `N` est plat sur 8–12 semaines (< 2 % d'écart).

- **Décision** : défauts `N = 10 semaines`, `τ = 2 semaines` (valeurs rondes, au
  centre de l'optimum, non surajustées à un mois). Le pas reste 15 min en temps
  réel ; il est configurable pour le backtest (`CARBONFR_BACKTEST_STEP_MINUTES`).

- **Portée & limites** : calé sur **national `rte-direct`, 2024**. Le régional,
  `acv-ademe`, et d'autres saisons peuvent différer ; l'outil `backtest-sweep`
  permet de recaler sans changer le modèle. Reste **`climatology@1`** (mêmes
  formule et garanties) : un changement de **paramètres par défaut** n'est pas un
  changement de méthode publiée — la formule et le contrat d'API sont inchangés.

## Sources

- [^eco2mix]: RTE / ODRÉ — éCO2mix national temps réel : la prévision publiée
  porte sur la **consommation** (J-1 et J) ; l'intensité carbone (`taux_co2`) et
  le mix sont **réalisés**, non prévus.
  <https://odre.opendatasoft.com/explore/dataset/eco2mix-national-tr/>
- [^seasonal]: R. J. Hyndman & G. Athanasopoulos, *Forecasting: Principles and
  Practice* (3ᵉ éd.) — méthodes de référence « seasonal naïve » et persistance,
  socle des modèles de climatologie. <https://otexts.com/fpp3/>
