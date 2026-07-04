# ADR-0028 — Prévision de la part renouvelable pour l'éligibilité (`share-clim@1`)

- **Statut** : Accepté (GATE de backtest franchi le 2026-07-03 ; re-jeu du GATE de neutralité requis avant service, cf. §Conséquences)
- **Date** : 2026-07-03
- **Décideurs** : Morgan (Kovelt / carbon-fr)
- **ADR liés** : ADR-0026 (parent — amende sa décision 9), ADR-0009 (famille de modèle climatologique), ADR-0011 (intervalles), ADR-0013 (≠ `MixForecaster` GBDT, chantier distinct), ADR-0018 (précédent du gate mesuré), ADR-0019 (versionnement)

---

## Contexte

La couche A « électrolyseur » (ADR-0025/0026) sert le pilier `renewable-share` du cadre `rfnbo` **au nowcast uniquement** (décision 9 d'ADR-0026, « D4 ») : au-delà de la dernière observation, la part renouvelable est `None` → signal `Indeterminate`. La revue de neutralité du 2026-07-03 a mesuré l'ampleur réelle de cette limite : **100 % des créneaux de l'endpoint prévisionnel** sont indéterminés sur ce pilier (constat C4, réfuté comme choix documenté, mais noté « le chantier qui le lèvera est MixForecast »). C'est le chantier **H4** de la roadmap hydrogène.

Contraintes héritées : jamais d'extrapolation muette (l'indétermination est un signal explicite) ; ancre `rte-direct` nationale ; jamais d'IO par créneau (audit F05) ; un modèle ne se sert que s'il **bat une baseline au backtest** (précédents `climatology@1` servi, `gbdt@1` refusé, gate ADR-0018).

## Décision

1. **Modèle `share-clim@1`** : climatologie **horaire-de-semaine de la part renouvelable** elle-même (pas du mix par canal, pas de météo en v1) + correction d'anomalie décroissante ancrée sur le **nowcast** — la formule d'ADR-0009 appliquée au scalaire `renewable_share`, clampée `[0, 1]`. τ = 14 j, profondeur 10 semaines (valeurs `climatology@1` ; à re-caler par backtest dédié si besoin). Fonctions **pures** dans `carbonfr-eligibility` (`share_forecast.rs`) ; `core` gagne seulement la promotion `pub` de `week_slot` (primitive de bucketing partagée).

2. **Prévision = toujours un intervalle** (ADR-0011) : bandes de **quantiles de résidus par horizon** (`HorizonBands`, q = 0,1) calibrées par walk-forward au démarrage (`CARBONFR_SHARE_CALIBRATE_WEEKS`, défaut 8 sem. ; `0` = opt-out ; `CARBONFR_SHARE_CALIBRATE_TO` pour une calibration reproductible). **Sans bandes calibrées, on ne prévoit pas** — la part future reste `Indeterminate` (comportement d'avant cet ADR, parité self-hosting).

3. **Verdict par règle d'intervalle, symétrique de D17** : le pilier `renewable-share` tranche fermement seulement hors recouvrement du seuil — `pass` si `lower ≥ 0,90`, `fail` si `upper < 0,90`, sinon `Indeterminate`. La part **observée** (nowcast) porte un intervalle dégénéré → comportement historique strictement inchangé.

4. **Horizon borné au calibré** : au-delà de `max_horizon` (72 h, le plafond de `greenest-window`), `None` → `Indeterminate` — même discipline que le prix day-ahead (PIÈGE 2). Jamais d'extrapolation des bandes au-delà de leur calibration.

5. **Provenance servie** (auditabilité, esprit de la revue C14) : le signal porte `provenance: observed | forecast` et, pour une prévision, `value_lower`/`value_upper` ; la réponse porte `share_model: "share-clim@1"` **seulement** quand une part prévue a été servie. Champs **additifs** (aucune rupture). Le `disclaimer` explicite la double provenance.

6. **Perf** : l'historique de mix est lu en **un seul** batch (`EligibilityRepo::national_mix_range`, motif `spot_prices_range`/F05), **seulement** pour `rfnbo` avec modèle câblé (zéro coût pour `low-carbon` et sans config). La climatologie est reconstruite par requête sur ce batch (~6 700 lignes indexées, même classe de coût que le `forecast()` déjà payé) ; l'ancre d'anomalie est le `latest_national_mix` déjà lu. Coût ajouté : **+1 requête SQL** par appel `?eligibility=rfnbo`.

7. **GATE mesuré (2026-07-03, national `rte-direct` consolidé 30 min, bandes calibrées sur 8 sem. disjointes, vérité dérivée du mix — jamais stockée)** — critère : bat la **persistance** en RMSE global **ET** zéro faux `pass` ferme au seuil 0,90 :

   | Fenêtre de test | RMSE modèle | RMSE persistance | h+1 | h+6 | h+24 | h+72 | Verdicts fermes | GO |
   |---|---|---|---|---|---|---|---|---|
   | 2026-03-01 → 2026-04-27 (57 origines) | **0,0410** | 0,0435 | 0,0182/0,0208 | 0,0285/0,0349 | 0,0494/0,0491 | 0,0561/0,0592 | 228 `fail`, **0 faux** | ✅ |
   | 2025-10-01 → 2025-11-26 (56 origines) | **0,0595** | 0,0640 | 0,0181/0,0175 | 0,0334/0,0369 | 0,0690/0,0715 | 0,0897/0,0987 | 222 `fail`, **0 faux** | ✅ |

   Lecture honnête : à h+1 le modèle ≈ persistance (attendu — l'ancre d'anomalie *est* de la persistance à horizon court, un quasi-nul à h+24 sur la 1re fenêtre) ; le gain croît avec l'horizon. Le seuil 0,90 n'est jamais approché en FR (part observée ~0,15-0,45) : les verdicts fermes sont des `fail` sûrs, **aucun** faux `pass`/`fail` sur 450 évaluations. Sous-commande re-jouable : `carbonfr-server backtest-share`.

8. **Ce que ça change dans la sortie servie** : le signal `renewable-share` des créneaux futurs passe d'`Indeterminate` (100 %) à un **`fail` ferme informatif** (valeur, intervalle, seuil, provenance) dans l'immense majorité des cas — les verdicts `eligible` globaux bougent à peine (la disjonction rfnbo reste portée par le prix). La valeur est **informationnelle** (l'opérateur voit la part estimée et sa distance au seuil), pas un renversement de verdicts.

## Conséquences

**Positives** : constat C4 de la revue de neutralité levé à la racine ; le pilier `renewable-share` devient réellement étayé sur l'horizon prévisionnel ; discipline d'incertitude uniforme (D17 partout) ; opt-in/opt-out propre (bandes calibrées ou rien) ; aucun changement cassant.

**Négatives / limites (assumées)** :
- La part **prévue** reste un proxy **instantané** de l'Article 4 (moyenne annuelle légale) — inchangé, documenté.
- Modèle climatologique pur : ne voit pas la météo. La variante météo-pilotée (RenewableModel ADR-0018 sur météo prévue, plafond 48 h du store actuel) est une **itération mesurable** : elle devra battre `share-clim@1` au `backtest-share` pour être promue (`share-meteo@2` le cas échéant, jamais de mutation silencieuse).
- τ/N hérités de `climatology@1` sans re-calage dédié (le gate passe avec ; un `backtest-share-sweep` est possible si besoin).
- **Re-jeu obligatoire du GATE de neutralité** (engagement de la revue du 2026-07-03) avant mise en production : le rééquilibrage d'information (rfnbo gagne un signal étayé, low-carbon inchangé) est précisément l'angle « symétrie » à re-tester.

## Alternatives envisagées

- **Climatologie par canal du mix** (réutiliser `Channel`/`mix_channel` d'ADR-0013 puis `renewable_share(mix prévu)`) — *écartée en v1* : plus de pièces mobiles pour la même cible scalaire ; la brique reste extractible si le mix prévu complet devient nécessaire.
- **Météo-piloté d'emblée** (RenewableModel sur météo prévue + climatologie du dénominateur) — *reporté, gate d'abord* : la validation ADR-0018 est **contemporaine** (météo de l'heure → production de l'heure), pas à horizon ; le plafond du store météo est 48 h ; leçon ADR-0018 (un signal physiquement bon peut ne rien apporter à la cible réelle). À mesurer contre `share-clim@1` avant tout service.
- **Servir la prévision sans bandes** (point sec) — *rejeté* : violerait ADR-0011 (« jamais un point sans incertitude dans une décision publique ») et la discipline « jamais d'extrapolation muette ».
- **Recalcul au poller / cache global** — *rejeté en v1* : la lecture par requête (motif `ClimatologyForecaster`) est la convention établie et le coût est borné ; un cache est une optimisation future si la charge le justifie.
- **Étendre `ForecastPoint` avec le mix** — *rejeté* : contrat ADR-0011 inchangé, l'enrichissement reste une orchestration d'adapter.
- **Confondre avec le `MixForecaster` GBDT d'ADR-0013** — *rejeté* : consommateur (`acv-ademe@2`), ancre méthodologique et gate de promotion différents ; composants partagés seulement s'ils sont extraits proprement.

---

## Addendum — itération météo `share-meteo@2` mesurée (2026-07-04)

L'alternative « météo-piloté » (reportée ci-dessus, *gate d'abord*) a été **implémentée comme expérience pure et mesurée** — module `crates/eligibility/src/share_meteo.rs`, **non servi**.

**Formule** (`share-meteo@2`) : dérivation **par canal** là où la météo *as-of* l'origine couvre la cible — éolien/solaire via le `RenewableModel` (ADR-0018) **calibré par origine sur la fenêtre d'apprentissage** (anti-fuite), corrigés à l'ancre (biais additif pour l'éolien, ratio multiplicatif pour le solaire — une correction additive ancrée en journée fabriquerait du solaire la nuit) ; hydraulique/bioénergies/nucléaire/fossile en climatologie de canal + anomalie d'ancre décroissante ; part = numérateur de `renewable_share` sur le total. **Au-delà de la couverture météo : repli exact sur la formule `share-clim@1`** — zéro régression possible aux horizons non couverts, le gain ne peut venir que de la couverture. Lecture météo *as-of* stricte (`run_at < origine`, dernier run gagne).

**GATE mesuré (2026-07-04, mêmes fenêtres/origines/pas que le §7, comparaison à trois : météo vs climatologie vs persistance sur les mêmes points)** — critère : bat **`share-clim@1`** (pas seulement la persistance) en RMSE global ET zéro faux `pass` :

| Fenêtre de test | RMSE `share-meteo@2` | RMSE `share-clim@1` | h+1 | h+6 | h+24 | h+72 | Verdicts fermes | GO formel |
|---|---|---|---|---|---|---|---|---|
| 2026-03-01 → 2026-04-27 (57 origines) | **0,0407** | 0,0410 | **0,0168**/0,0182 | **0,0273**/0,0285 | 0,0494 (=) | 0,0561 (=) | 228 `fail`, **0 faux** | ✅ |
| 2025-10-01 → 2025-11-26 (56 origines) | **0,0594** | 0,0595 | **0,0169**/0,0181 | 0,0333/0,0334 | 0,0690 (=) | 0,0897 (=) | 222 `fail`, **0 faux** | ✅ |

**Lecture honnête** : le critère formel de promotion est rempli sur les deux fenêtres, mais le gain global est **mince** (−0,7 % / −0,2 % de RMSE) car il est **structurellement borné par la convention d'archive** (`run_at = valid_at − 24 h` : seuls h+1 et h+6 des checkpoints sont couverts en backtest ; h+24 tombe exactement sur la frontière stricte et h+72 est hors couverture → parité exacte par construction du repli). Là où la météo couvre, le gain est réel et systématique : **−7,7 %/−6,6 % à h+1**, −4,2 %/−0,3 % à h+6. En **service**, la couverture réelle est ~48 h avec des runs bien plus frais (1-15 h de lead) : le backtest **sous-estime** vraisemblablement le gain de service — mais c'est ce qui est mesurable aujourd'hui sans re-jouer l'ingestion.

**Décision de service : en attente (décision produit).** Deux options documentées :
1. **Promouvoir** `share-meteo@2` (câblage `eligibility_uc` + composition root + lecture météo batchée par requête, `share_model: "share-meteo@2"` servi) — impose le **re-jeu du GATE de neutralité** (chaîne servie + comportement) et une requête SQL de plus par appel `?eligibility=rfnbo`.
2. **Conserver en expérience** (état actuel) : le module et la comparaison `backtest-share` restent dans le dépôt (re-jouables), `share-clim@1` reste servi ; re-mesurer quand le store météo dépassera 48 h ou avec des leads de service réels.

La sous-commande `backtest-share` exécute désormais **toujours** la comparaison à trois quand un historique météo est disponible (sinon elle la saute en l'annonçant).
