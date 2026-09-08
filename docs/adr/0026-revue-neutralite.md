# Revue de neutralité — Couche A « éligibilité électrolyseur » (ADR-0026)

- **Date :** 2026-07-03
- **Objet :** GATE de neutralité de l'overlay d'éligibilité (`?eligibility=` sur `GET /v1/intensity/greenest-window` + `GET /v1/eligibility/rulesets`), recommandé par l'ADR-0026 avant tout palier payant sur cette couche
- **Évalué :** la **sortie réellement servie** — captures de la prod du 2026-07-03 (v0.4.2 ; seul écart avec `main` v0.4.4 = plafond de sûreté F03 et batch prix F05, sans effet de wording) : catalogue, fenêtres `rfnbo`/`low-carbon` (nominal, `prudent`, overrides), cas d'erreur — plus les chaînes servies (`crates/eligibility/src/ruleset.rs`, `crates/adapter-http/src/dto.rs`)
- **Méthode :** évaluation adversariale multi-agents — critique militant de **chaque bord** (pro- et anti-nucléaire) + 4 auditeurs (symétrie structurelle, provenance juridique, mélecture « journaliste/dev naïf », impact décisionnel « opérateur d'électrolyseur ») → consolidation (15 constats canoniques) → **contre-instruction** : 3 réfutateurs par constat (lentilles conception / exactitude / matérialité, constat tué si ≥ 2 réfutent) → correctifs → passage 2 (re-test par constat sur le code corrigé)
- **Statut :** **GREEN** (neutralité confirmée après 3 correctifs + 2 résidus levés au passage 2)

> Artefact exigé par l'ADR-0026 (« une revue de neutralité adversariale type ADR-0024 est recommandée avant tout palier payant »). **Re-jouable** : toute modification des rulesets servis (seuils, `legal_basis`, `description`, `disclaimer`), de la logique de verdict ou de la surface API impose de rejouer ce GATE. L'addendum de vérification sources primaires du 2026-07-03 (ADR-0026) fait foi pour les questions de droit.

---

## 1. Déroulé

**Passage 1 (RED étroit).** Les six critiques ont remonté 15 constats consolidés, dont 3 « critical ». La contre-instruction en a tué 12 — **tous les critical sont tombés**, la plupart sur des erreurs factuelles ou parce que la réponse servie contient déjà la divulgation demandée (disclaimer, `signals` détaillés, marqueurs [FAIT]/[ESTIMATION]). Comme pour l'ADR-0024, la *mécanique* est saine : les charges des deux militants ne mordent pas sur la conception. Survivent 3 constats **majeurs** — confirmés à l'unanimité sur l'exactitude et la conception, réfutés seulement sur la matérialité — c'est-à-dire des défauts réels de documentation et de provenance, réparables sans toucher au fond méthodologique :

- **C8 (symétrie)** : le champ `score` porte la même clé JSON pour deux grandeurs incommensurables (`low-carbon` = intensité brute ; `rfnbo` = heuristique composite, échelle ~10×) sans documentation dans le contrat OpenAPI.
- **C9 (provenance)** : le `legal_basis` de `low-carbon:2025-2359` attribuait le comparateur 94 gCO₂eq/MJ directement à l'acte 2025/2359 (il y est incorporé **par renvoi** au Règl. 2023/1185 — le ruleset rfnbo citait correctement les deux textes) et servait « consultation d'ici 30/06/2026 » sans marqueur de réserve, date échue et considérant non contraignant.
- **C14 (provenance)** : un signal dont le seuil venait d'être **écrasé par l'appelant** (`?surplus_price_eur_mwh=100`) restait étiqueté `basis:"regulatory"` — `basis_of` ignorait l'état `overridden`.

**Correctifs (P1–P3, 2026-07-03).** Voir §4.

**Passage 2 (re-test sur le code corrigé).** Un vérificateur par constat : C14 levé et complet (tous les chemins d'override vérifiés ligne à ligne, aucun producteur de signal ne contourne `basis_for`) ; C9 levé et complet ; C8 levé sur la surface principale (DTO + OpenAPI) mais **incomplet** — le SDK TypeScript, qui recopie à la main les rustdoc sur ces interfaces, n'avait pas reçu celle de `score`. Deux résidus corrigés dans la foulée : JSDoc `score` ajoutée aux deux interfaces du SDK ; « (PPA) » retiré du `legal_basis` (sigle absent du texte officiel de la consultation nucléaire — champ étiqueté [FAIT], provenance stricte exigée).

---

## 2. Tableau pass/fail par bloc (état après correctifs)

| Bloc | Verdict | Mord sur la conception |
|---|---|---|
| Bloc 1 — Symétrie des cadres | **PASS** (après P2/C8) | non |
| Bloc 2 — Non-verdict / non-certification | **PASS** | non |
| Bloc 3 — Provenance & marqueurs FAIT/ESTIMATION | **PASS** (après P1/C14 et P3/C9) | non |
| Bloc 4 — Test adverse (pro-nucléaire) | **PASS** | non |
| Bloc 4 — Test adverse (anti-nucléaire) | **PASS** | non |
| Bloc 4 — Mélecture (journaliste/dev) & usage (opérateur) | **PASS** | non |

---

## 3. Les deux charges les plus fortes (test adverse) et leur réfutation

### Charge pro-nucléaire (la plus mordante)

« Le champ `basis` inverse la hiérarchie qu'il suggère : les piliers rfnbo sont tamponnés `regulatory` alors qu'ils ne peuvent jamais asséner de rejet ferme, pendant que le pilier du nucléaire (`low-carbon`, tamponné `indicative`) est le seul à pouvoir dire « non éligible » avec certitude. Vous avez blindé le récit renouvelable contre le mot « non » et laissé le nucléaire à découvert — en rappelant à chaque réponse que sa reconnaissance est « en cours », sans jamais rappeler que le cadre rfnbo est lui-même sous révision. »

**Réfutation (3/3).** (1) La prémisse est **factuellement fausse** : le pilier `RenewableShare` **peut** émettre un échec ferme (`passed:false` dès que la part connue est < 0,90 — c'est le setup même du test cité par la charge) ; seul `SurplusPrice` n'assène jamais de `fail`, parce que la branche EUA non câblée pourrait encore valider l'exception — et cette raison est **divulguée verbatim dans chaque réponse** (« la branche surplus EUA (<0,36×prix EUA) n'est pas évaluée »). (2) `basis` a une sémantique documentée et **orthogonale à la certitude du verdict** : il dit si le *seuil numérique* est écrit dans un texte contraignant (90 %, 20 €/MWh le sont) ou dérivé par carbon-fr (64 ne l'est pas). Étiqueter le proxy `regulatory` pour « rééquilibrer » serait précisément la faute de provenance que le GATE interdit. (3) Sur les chiffres servis, `low-carbon` marque 67/96 créneaux éligibles contre 39/96 pour `rfnbo` sur la même fenêtre — l'outcome ne désavantage pas la lecture pro-nucléaire.

### Charge anti-nucléaire (la plus mordante)

« Votre « vue renouvelable » ne vérifie jamais la moindre part renouvelable : sur 120 créneaux capturés, `renewable-share` est indéterminé à 100 %, et un créneau de base nucléaire à 12,99 gCO₂eq/kWh décroche `eligible:true` sous l'étiquette rfnbo uniquement parce que le prix était négatif. Le nucléaire rentre par la porte RFNBO sans un mégawattheure d'éolien vérifié. »

**Réfutation (2/3).** (1) L'exception de surplus (prix ≤ 20 €/MWh) **est le droit en vigueur** (Règl. 2023/1184, art. 6) — la servir n'est pas un biais de carbon-fr, et la refuser reviendrait à durcir le cadre réglementaire dans un sens militant. (2) La transparence est totale : chaque créneau expose ses `signals` complets (`renewable-share: indeterminate`, `surplus-price: pass`, valeurs et seuils) — le lecteur voit exactement *par quoi* l'éligibilité est portée. (3) Le nowcast-only de la part renouvelable est un choix documenté (ADR-0026 D4) dont l'effet pratique est faible : rfnbo étant une **disjonction** et l'Article 4 (≥ 90 %) n'étant ≈ jamais atteint en France, un mix prévu ne changerait quasiment aucun verdict. Le vote « conception » a toutefois retenu que l'ADR sous-estimait l'ampleur empirique (« partiellement » vs 100 % observé sur cet endpoint) — noté en veille, §5.

---

## 4. Correctifs appliqués (2026-07-03)

| # | Constat | Correctif | Où |
|---|---|---|---|
| P1 | C14 | `basis` d'un pilier **surchargé** passe à `user-override` : flags granulaires `surplus_price_overridden` / `low_carbon_threshold_overridden` posés par `with_overrides`, méthode `EligibilityRuleset::basis_for`, DTO branché dessus ; 4 tests unitaires + 1 test HTTP (pilier surchargé **et** pilier intact) | `ruleset.rs`, `verdict.rs`, `dto.rs`, `api.rs` |
| P2 | C8 | `score` documenté dans le contrat (rustdoc → OpenAPI, les **deux** schémas) comme **interne au cadre**, avec renvoi vers `intensity` pour toute comparaison inter-cadres ; JSDoc miroir dans le SDK TS (résidu du passage 2) | `dto.rs`, `openapi.snapshot.json`, `types.ts` |
| P3 | C9 | `legal_basis` low-carbon réécrit : 94 gCO₂eq/MJ attribué au **renvoi vers 2023/1185** (aligné sur rfnbo) ; clause calendaire requalifiée « considérant non contraignant, échéance 30/06/2026 — lancement non constaté au 2026-07-03 ; évaluation contraignante d'ici 07/2028 (art. 3) » ; « (PPA) » retiré (résidu du passage 2, provenance stricte) | `ruleset.rs` |

Tous additifs : aucune rupture de contrat (`basis` reste `string` côté SDK ; le seul test figeant `basis` porte sur le cas sans override, inchangé). `cargo fmt`/`clippy -D warnings`/tests workspace/`tsc --noEmit` verts.

---

## 5. Constats réfutés notables, limites et veille

- **C4 (critical, réfuté 2/3)** — « `renewable-share` indéterminé à 100 % sur l'endpoint prévisionnel » : fait exact, mais c'est le choix **documenté** D4 (part renouvelable nowcast-only, jamais extrapolée), divulgué dans le disclaimer, et d'effet quasi nul sur les verdicts (disjonction + Article 4 ≈ jamais atteint). Le chantier qui le lèvera vraiment est **`MixForecast`** (roadmap H4) — cette revue en renforce la justification.
- **C6 (critical, réfuté 3/3)** — « pas de champ `indeterminate` par créneau » : l'information est déjà servie sans perte (`signals[].verdict`, `count_indeterminate`) ; un booléen de plus serait redondant.
- **Datation figée** « au 2026-07-03 » dans le `legal_basis` : choix assumé (fait daté vérifiable) mais **dette documentaire consciente** — si la consultation nucléaire est lancée, le texte doit être mis à jour. Couvert par le signal de veille n°3 de [`docs/roadmap-hydrogene.md`](../roadmap-hydrogene.md).
- **Re-jeu obligatoire** : à l'activation de `rfnbo:2026-revision` (roadmap H3), à l'arrivée de `MixForecast` (H4 — ✅ re-joué, cf. §6), ou à toute retouche des chaînes servies.

---

## 6. Re-jeu du 2026-07-03 — arrivée de `share-clim@1` (ADR-0028, roadmap H4)

**Évalué** : la sortie **réellement servie** par le code candidat (serveur réel + base réelle, calibration reproductible `CARBONFR_SHARE_CALIBRATE_TO`) — parts renouvelables **prévues** sur l'horizon, provenance, intervalles, `share_model` — comparée au comportement précédent. **Même méthode** (6 critiques → consolidation → 3 réfutateurs par constat).

**Passage 1 (RED étroit)** : 12 constats consolidés, 8 tués en contre-instruction — dont les charges les plus fortes des deux bords : la charge anti-nucléaire (« 0/96 éligible rfnbo vs 96/96 low-carbon = biais structurel ») réfutée car ce contraste est le mix électrique français lui-même, servi en transparence totale, et les verdicts globaux ne bougent pas du fait de `share-clim@1` (qui n'émet jamais de `pass` en pratique) ; la charge « fail ferme d'une prévision = sur-affirmation » réfutée car c'est la discipline D17 déjà servie par low-carbon (mêmes bandes q=0,1). Survivent **4 constats**, tous confirmés à l'unanimité ou à 2/3 :

- **F1 (critical, symétrie/provenance)** — la divulgation de provenance n'existait que sur `renewable-share`, avec un commentaire **factuellement faux** (« les autres piliers ne servent que de l'observé ») alors que l'intensité du pilier low-carbon est aussi une prévision (`ForecastPoint`) sur tout créneau futur. rfnbo paraissait « scientifiquement audité », low-carbon « affirmation brute » — inversion de rigueur perçue.
- **F3 (critical, méthodologique)** — la comparaison avant/après citée au dossier du re-jeu (39→0 éligibles) était **confondue** par la dérive du pilier prix entre les deux captures (la capture « avant » venait de la prod avec données ENTSO-E ; la capture « après » d'un serveur local sans prix spot). Le delta ne prouve **rien** sur `share-clim@1`. Le vrai A/B isolé est le test d'intégration `greenest_window_eligibility_without_share_model_is_unchanged` (même instant, avec/sans modèle) : les verdicts globaux sont identiques, seul le signal `renewable-share` gagne de l'information.
- **F6 (major, symétrie)** — le disclaimer accordait à `renewable-share` une garantie (« jamais extrapolé au-delà de l'horizon calibré ») que l'intensité n'a pas (repli silencieux sur dispersion non calibrée).
- **F12 (minor, provenance)** — `Indeterminate` sans code de raison : hors-horizon, donnée manquante et prix-au-dessus-du-seuil indiscernables.

**Correctifs (P4–P7, 2026-07-03)** :

| # | Constat | Correctif |
|---|---|---|
| P4 | F1 | Parité de divulgation : `provenance` servie sur **tous** les piliers tranchés (`low-carbon-intensity` → `forecast` — l'intensité vient toujours du modèle de la réponse ; `surplus-price` → `observed` — day-ahead publié) ; commentaires sur-affirmants corrigés (`verdict.rs::provenance`, doc DTO) |
| P5 | F6 | Disclaimer réécrit : provenance de **chaque** pilier explicitée, intervalle d'intensité qualifié (« bandes calibrées, repli dispersion par créneau à froid »), garantie d'horizon calibré scopée à `share-clim@1` |
| P6 | F12 | `IndeterminateReason` versionné servi avec chaque signal indéterminé (`missing-data` / `beyond-calibrated-horizon` / `threshold-within-interval` / `surplus-not-established`) — champ additif |
| P7 | F3 | La présente section neutralise la comparaison confondue : ne **jamais** citer le delta 39→0 comme effet d'ADR-0028 ; la preuve d'invariance des verdicts est le test A/B au même instant |

**Passage 2** : re-test par constat sur le code corrigé et la sortie re-servie (`reason: beyond-calibrated-horizon` observé à la frontière 72 h, `provenance: forecast` sur le pilier intensité, disclaimer conforme). **Verdict : GREEN.**

Constats réfutés notables du re-jeu : « `surplus-price` ne fail jamais = asymétrie intra-rfnbo » (choix documenté EUA, désormais explicité par `surplus-not-established`) ; « le 0-faux-verdict du gate est vacuement vrai » (exact — le seuil n'est jamais approché en FR — mais l'ADR-0028 §7 le dit lui-même : la revendication est correctement qualifiée).

---

## 7. Re-jeu du 2026-07-04 — addendum O1 « méthodes horaires » (wording, texte servi)

**Évalué** : le diff complet du chantier O1 (roadmap hydrogène) **avant fusion** — addendum « méthodes horaires » de l'ADR-0026, enrichissement du `legal_basis` **servi** du ruleset `low-carbon:2025-2359` (mention des 4 méthodes de l'annexe, seuil ~64 qualifié de proxy hors annexe), descriptions OpenAPI (`/v1/intensity/forecast`, `/v1/intensity/greenest-window`, technologie marginale de `/v1/price`), README, roadmap, CHANGELOG. Déclencheur : la règle de re-jeu du présent document (« toute retouche des chaînes servies »).

**Méthode** : 5 critiques à lentilles distinctes (militant pro-nucléaire, militant anti-nucléaire, précision réglementaire, lecteur pressé/intégrateur API, cohérence interne) → **contre-instruction** : 3 réfutateurs par constat (lentilles exactitude factuelle / neutralité effective / proportionnalité, constat tué si ≥ 2 réfutent) → correctifs → passage 2. 53 agents au total.

**Passage 1** : 16 constats bruts, 4 tués en contre-instruction, **12 confirmés** — qui se réduisent à **trois causes racines** :

- **O1-C1 (critical, ×9 constats — provenance de processus)** — le diff soumis affirmait au passé, dans trois fichiers (addendum ADR, CHANGELOG, roadmap), que « la passe de neutralité a été re-jouée (revue, §7) » alors que la présente section **n'existait pas encore** au moment de la critique : l'affirmation de conformité au processus était elle-même invérifiable — précisément le type d'assertion que ce GATE interdit. (Le re-jeu était en cours d'exécution au moment de la lecture du diff ; il n'en reste pas moins que le texte anticipait un fait non constaté — le constat est retenu tel quel.)
- **O1-C2 (major, ×2 — précision réglementaire)** — la phrase du README généralisait « (elles exigent une donnée publiée par le GRT) » aux **quatre** méthodes, alors que l'addendum lui-même ne l'établit que pour les méthodes horaires (b)/(d) — (a) repose sur la Table 5, (c) sur un comptage d'heures pleine charge.
- **O1-C3 (minor — exactitude de référence)** — le tableau de l'addendum citait `PriceContext.marginal` (identifiant du domaine interne) au lieu du champ **réellement servi** `marginal_technology`.

**Correctifs (P8–P10, 2026-07-04)** :

| # | Constat | Correctif |
|---|---|---|
| P8 | O1-C1 | La présente section : le re-jeu est désormais **documenté et vérifiable** — les mentions « revue §7 » des trois fichiers deviennent exactes à la fusion (le diff est fusionné en un seul lot avec cette section) |
| P9 | O1-C2 | README nuancé méthode par méthode : « les méthodes horaires (b)/(d) exigent une donnée publiée par le GRT, (a) repose sur la Table 5 officielle, (c) sur un comptage d'heures pleine charge non implémenté » |
| P10 | O1-C3 | Tableau de l'addendum corrigé : « le champ `marginal_technology` du contexte de `GET /v1/price` » |

**Constats réfutés notables** : « RTE ne publie ni prévision day-ahead d'intensité de bidding zone ni marginale horaire — affirmation sans source » (réfuté sur proportionnalité : le fait est défendable et la charge de la preuve inversée serait celle d'une donnée qui existerait) ; « le `legal_basis` servi passe de 570 à 920 caractères, duplication » (réfuté : chiffre exact mais chaîne documentaire dont c'est le rôle, pas de duplication mot à mot) ; « la proximité numérique Table 5 (≈66-68) vs proxy ~64 est présentée comme validation » (réfuté : le texte met explicitement en garde contre cette lecture — « coïncidence d'ordre de grandeur, pas une équivalence méthodologique ») ; « le titre "alignement" contredit le corps "n'implémente aucune méthode" » (réfuté : le corps lève l'ambiguïté dès le premier paragraphe).

**Passage 2** : re-vérification par constat sur le texte corrigé (les trois correctifs sont textuels et immédiatement vérifiables dans le même diff). **Verdict : GREEN.**
