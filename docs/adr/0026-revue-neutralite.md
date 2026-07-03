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
- **Re-jeu obligatoire** : à l'activation de `rfnbo:2026-revision` (roadmap H3), à l'arrivée de `MixForecast` (H4), ou à toute retouche des chaînes servies.
