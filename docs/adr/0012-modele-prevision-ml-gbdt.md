# ADR-0012 — Modèle de prévision ML : `GbdtForecaster` (tout-Rust) + features météo

- **Statut** : Accepté (mise en œuvre **engagée** — store météo + framework GBDT + `bin/train` + `GbdtForecaster` livrés ; mesuré, `gbdt@1` ne bat pas `climatology@1` → **non servi** par garde de promotion ; gain ML = itération ouverte)
- **Date** : 2026-06-15
- **Raffine** : ADR-0011 (tranche la fourche *runtime* qu'il avait laissée ouverte)

## État d'implémentation (2026-06-15)

**Tranche 1 — store de prévision météo (§5/§6) : livré.** Port
`WeatherForecastSource` + adapter `carbonfr-adapter-meteo` (Open-Meteo, sans
clé, FR/EU — vent à 100 m + irradiance, agrégés sur 7 points de métropole).
Store `WeatherRepository` (table `weather_forecast`) **daté `(run_at, valid_at)`**
pour l'**anti-fuite** : on conserve l'historique des `run_at` afin de
n'entraîner/inférer que sur la prévision *telle qu'elle était disponible*. Le
poller l'ingère à chaque cycle. Le crate `gbdt` (GBDT pur Rust, **Apache-2.0**,
compatible cargo-deny) est confirmé pour la suite. ENTSO-E (prévisions de
génération vent/solaire, souveraines mais sous token) reste l'**upgrade**
prévu, derrière le même port.

**Tranche 2a — framework GBDT : livré.** Crate `carbonfr-adapter-gbdt`
(`gbdt` pur Rust) : *feature engineering* **partagé** train/inférence
([`features`], identité garantie — ancre = dernière observation avant
l'origine, anti-fuite), `build_training_examples` + `train_model` (entraînement
offline), `GbdtForecaster` (inférence derrière `ForecastModel`, artefact
chargé par chemin), sous-commande `carbonfr-server train` (entraîne → sauve →
**compare au backtest** `gbdt@1` vs `climatology@1`). Le `core` reste pur.

**Résultat mesuré (features actuelles : calendrier + lags d'intensité, SANS
météo).** Sur novembre 2024 (entraîné juin→oct), `gbdt@1` **ne bat pas**
`climatology@1` (RMSE ≈ 15,8 vs 7,5). C'est **attendu** : la climatologie
calibrée (+ correction d'anomalie) est une référence difficile, et le levier
identifié par cet ADR — la **météo** — n'est pas encore en jeu. `climatology@1`
**reste le modèle servi** (garde de promotion : on ne sert le GBDT que s'il bat
la baseline).

**Tranche 2b — features météo + apprentissage résiduel : livrée, mais le GBDT
ne bat toujours pas `climatology@1`.** Ajouté : backfill historique des
prévisions météo (API archive Open-Meteo, `run_at = valid_at − 24 h`,
anti-fuite) ; features **vent/irradiance prévus** (météo *as-of* l'origine) et
**climatologie de créneau** (apprentissage résiduel), calculées **identiquement**
à l'entraînement et à l'inférence (fenêtre glissante). Correctif au passage :
dédup `(region, at)` dans `upsert_loads`.

Mesuré (national `rte-direct`, novembre 2024) sur plusieurs configurations —
sans météo, avec météo, et entraîné sur l'**année entière** (pour écarter
l'extrapolation hivernale, les arbres n'extrapolant pas) : `gbdt@1` reste
**~2× moins bon** que `climatology@1` (RMSE ≈ 15 vs 7,5). La climatologie +
correction d'anomalie **calibrée** est une référence difficile ; le GBDT
(`gbdt-rs`, features/hyper-paramètres actuels) ne la dépasse pas.

**Décision (garde de promotion) : `climatology@1` reste le modèle servi.** Le
framework ML, le pipeline météo et la garde sont en place et réutilisables ;
faire gagner le GBDT relève d'une **itération ML ouverte** (réglage
d'hyper-paramètres, features supplémentaires, voire une implémentation de
boosting plus riche) — non engagée tant que le bénéfice n'est pas démontré.

## Contexte

L'ADR-0011 a fixé le **contrat** de prévision (`ForecastPoint` + intervalles, port `ForecastModel` retypé) et un premier modèle **saisonnier** (`StatForecaster`). Le ML y était explicitement **reporté**, et le choix de *runtime* laissé ouvert.

Cet ADR décide le **premier modèle ML**. Il se branche derrière `ForecastModel` — c'est le *drop-in* annoncé par l'ADR-0002 — et n'a le droit d'être livré que s'il **bat le `StatForecaster`** sur le harnais de backtest (ADR-0011). Le `StatForecaster` n'est donc pas un concurrent : c'est l'**étalon** qui empêche le « ML théâtre ».

Décisions de cadrage (cette itération) : **runtime tout-Rust, rebuildable au `cargo`** ; **features incluant la météo** (nouvelle source). Périmètre `rte-direct`, national.

## Décision

### 1. Modèle : arbres de gradient boosté, tout-Rust

**Entraînement *et* inférence en Rust** (`gbdt` / `linfa`). Pas de Python, pas de dépendance C, pas d'ONNX. Le projet reste **rebuildable au `cargo` de bout en bout** et le serveur **mono-binaire** — la souveraineté est préservée *à l'exécution comme au ré-entraînement*. Les arbres boostés sont par ailleurs un choix solide sur des features tabulaires comme l'intensité.

### 2. Adapter `GbdtForecaster` derrière `ForecastModel`

Il produit des `ForecastPoint` (`expected` + `lower`/`upper` + `model` + `methodology`) : **aucun changement de contrat**, l'ADR-0011 a déjà tout posé. Il lit ses features depuis le repository / les rollups et le store météo. **Zéro IO dans `core`** — toute la *feature engineering* vit dans l'adapter.

### 3. La météo n'entre **pas** dans le domaine

Point de frontière important : `core` ne connaît que `ForecastPoint`. La météo est un **détail d'adapter de prévision**, pas un concept métier. On ne crée donc **aucun type météo dans `core`** — sinon on ferait fuiter une préoccupation d'infrastructure dans le domaine (ADR-0002).

### 4. Features

- **calendrier** : créneau horaire, type de jour (ouvré / week-end / férié), saison ;
- **lags récents** d'intensité (autorégressif) ;
- **consommation prévue RTE** (J-1 / J), déjà ingérée via ODRÉ ;
- **météo prévue** (vent / irradiance solaire) — **nouvelle source**.

### 5. Source météo = nouveau port + adapter

Un port sortant `WeatherForecastSource` (prévisions vent / irradiance, agrégées au niveau national, au pas compatible quart d'heure ou interpolées), implémenté par un adapter. Source **FR/EU** privilégiée pour la souveraineté : Météo-France open data (AROME/ARPEGE) ou les **prévisions de génération éolien/solaire d'ENTSO-E** ; Open-Meteo en repli. Comme pour ODRÉ et ENTSO-E : **jamais appelée par requête utilisateur** — le **poller** l'ingère.

### 6. Anti-fuite : on entraîne sur la météo **prévue**, pas observée

C'est le piège central. Une prévision d'intensité à 24 h ne disposera, en production, que d'une **prévision météo**, pas de l'observation. Entraîner sur la météo *observée* surévaluerait la performance (fuite de données) et donnerait un modèle décevant en prod. Le store de features météo est donc daté par **`(run_time, valid_time)`**, et l'entraînement n'utilise que la prévision **telle qu'elle était disponible** à l'instant de la prédiction simulée.

### 7. Cycle de vie (MLOps) tout-Rust

- **`bin/train`** : binaire d'entraînement **offline** qui lit l'historique (consolidé/définitif, 2012→) + les features, entraîne le GBDT, et produit un **artefact versionné** (`ModelVersion`).
- **Livraison** : l'artefact est **chargé depuis un chemin** au composition root (pas embarqué dans le binaire serveur) → republier un modèle = déposer un fichier + bump `ModelVersion`, **sans recompiler** le serveur.
- **Garde de promotion** : réutilise le harnais de backtest (ADR-0011). Un nouveau modèle ne remplace l'actuel que s'il bat **(a)** le `StatForecaster` *et* **(b)** le modèle en place, sur MAE **et** taux de couverture, en *walk-forward*. Sinon : pas de livraison.
- **Intervalles** : **mêmes quantiles empiriques de résidus par horizon** que l'ADR-0011, calculés sur le backtest du modèle ML — méthode d'incertitude inchangée, juste un meilleur estimateur central.
- **Ré-entraînement** : périodique (dérive du réseau), idéalement déclenché aussi après révision de millésime (ADR-0006).

### 8. Périmètre

`rte-direct`, **national**. La prévision **`acv-ademe`** (imports prévus + intensités voisines prévues) reste **couplée à l'axe 1** → reportée.

## Conséquences

- **Souveraineté préservée** : tout rebuildable au `cargo`, entraînement + inférence Rust, serveur mono-binaire (+ `bin/train` offline).
- **Domaine inchangé** : le contrat de l'ADR-0011 suffit ; la météo ne le touche pas. La frontière hexagonale tient.
- **Infra** : nouveau port `WeatherForecastSource` + adapter ; **store de features météo prévisionnelles** (daté `run/valid`) ; `bin/train` ; le poller ingère désormais aussi la météo. Le harnais de backtest devient **critique** (garde de livraison).
- **Réversibilité** : si `gbdt` plafonne en qualité, l'option « entraînement offline + chargement d'artefact » reste un *drop-in* **derrière le même port**, sans dette d'architecture.
- **Coût assumé** : `gbdt`/`linfa` offrent un outillage de *tuning* plus maigre que LightGBM/XGBoost (gestion des catégorielles, régularisation, vitesse). C'est le prix de la souveraineté ; le baseline saisonnier + la garde backtest sont le filet.

## Alternatives envisagées

- **Entraînement offline Python (LightGBM/XGBoost) + ONNX** : meilleur outillage, mais Python dans la boucle de ré-entraînement + soit dépendance C (`ort`), soit `ai.onnx.ml` (arbres) mal couvert par `tract`. Écarté pour cette itération (souveraineté), **gardé en réserve** (réversible derrière le port).
- **Réseau de neurones (`tract`)** : `tract` excelle sur le NN, mais c'est de la sur-ingénierie pour ce volume tabulaire face à un GBDT, et l'entraînement NN en Rust pur est moins mûr. Écarté.
- **Entraîner sur la météo observée** : pipeline plus simple, mais **fuite de données** → performance surévaluée. Écarté fermement (voir §6).
- **Pas de météo (features déjà ingérées seulement)** : plus simple, aucune nouvelle source — mais c'est précisément le levier qui fait gagner le ML sur la saisonnalité. Sans lui, le ML peine à justifier son existence face au `StatForecaster`. Écarté (décision : météo incluse).
- **Artefact embarqué dans le binaire** : repro maximale, mais impose de recompiler/redéployer le serveur pour republier un modèle. Écarté au profit du chargement par chemin.

## Questions ouvertes (implémentation — n'impactent pas le contrat)

- Source météo exacte (Météo-France AROME/ARPEGE vs prévisions de génération ENTSO-E vs Open-Meteo) et granularité spatiale (maille → agrégation nationale).
- Cadence de ré-entraînement et **seuil de gain minimal** pour promouvoir un modèle.
- Format de sérialisation de l'artefact GBDT (format du crate vs format maison versionné).
- Alignement temporel `run_time` / `valid_time` de la météo dans le store de features.
