# ADR-0026 — Méthodologie des overlays d'éligibilité électrolyseur (RFNBO / bas-carbone)

- **Statut** : Accepté
- **Date** : 2026-06-20
- **Décideurs** : Morgan (Kovelt / carbon-fr)
- **ADR liés** : ADR-0025 (parent — couche A), ADR-0014 (primitives carbon-aware), ADR-0023 (prix spot), ADR-0011 (contrat de prévision), ADR-0006 (versionnement de la donnée), ADR-0021 (Problem Details)

---

## Contexte

L'ADR-0025 acte la **couche A** « électrolyseur » : un overlay d'**éligibilité au niveau réseau** par créneau, sous deux cadres neutres (`rfnbo` renouvelable, `low-carbon` bas-carbone inclusif), au-dessus de la donnée régionale horodatée et de `/greenest-window`. Il renvoie à un ADR de méthodologie pour figer la paramétrisation. C'est cet ADR.

Forces en présence :

- **Réglementaire mouvant.** RFNBO (Règl. délégués (UE) 2023/1184 & 2023/1185) impose une corrélation temporelle se resserrant vers l'horaire (échéance **2030-01-01**, report **2031-2033** proposé mais **non adopté** à ce jour). L'acte délégué bas-carbone (UE) **2025/2359** (adopté le 8/7/2025, publié le 21/11/2025) fixe un seuil **produit** mais aucun seuil **électrique**.
- **Gap technique.** Le `ForecastPoint` (ADR-0011) ne porte que l'intensité, **pas le mix** : la part renouvelable n'est calculable qu'au nowcast/historique.
- **Pièges (ADR-0025).** Zone de dépôt = **nationale** (FR = 1 zone, ≠ 12 sous-régions carbone) ; le prix day-ahead a un horizon borné (~24-36 h).
- **Neutralité (cardinal).** carbon-fr expose l'éligibilité au regard de **chaque** cadre, sans trancher la « couleur » de l'H₂.

### Faits vs estimations

| Valeur | Statut | Source |
|---|---|---|
| Bascule horaire RFNBO `2030-01-01` | **FAIT** | Règl. (UE) 2023/1184 (corrélation mensuelle → horaire ; option d'anticipation MS dès 2027-07-01, non exercée en FR) |
| Article 4 — seuil `0,90` (renouvelable de zone, moyenne **annuelle**) | **FAIT** | Règl. (UE) 2023/1184 art. 4 (≈ jamais atteint en FR) |
| Surplus prix `≤ 20 €/MWh` (OU `< 0,36 × prix EUA`) | **FAIT** | Règl. (UE) 2023/1184 (branche EUA **non câblée**, faute de flux EUA) |
| Seuil produit bas-carbone `28,2 gCO₂eq/MJ` (= 94 × 0,30) | **FAIT** | Règl. (UE) 2025/2359 (réduction ≥ 70 % vs comparateur fossile 94 gCO₂eq/MJ) |
| Conso électrolyseur `53 kWh/kgH₂` | **FAIT** (fourchette) | Médiane industrielle 50-55 (JRC/IEA/IRENA) |
| Seuil **électrique** bas-carbone `~64 gCO₂eq/kWh` | **ESTIMATION** | Dérivé carbon-fr (cf. décision 4) — **non réglementaire** |
| Report horaire `2031-2033` | **PROPOSITION** | Révision RFNBO attendue ~2026, **non adoptée** |

---

## Décision

1. **Crate domaine pur `carbonfr-eligibility`** au-dessus de `core` (sens unique), sans IO (`time`, jamais `chrono`). Évaluation **totale** (pas de `Result` ; l'indétermination est portée par un signal `Indeterminate`). `core` **intact** ; l'orchestration (mix nowcast + prix) vit côté adapter HTTP.

2. **Deux cadres neutres, séparés** (binôme étiqueté comme `rte-direct`/`acv-ademe`) :
   - **`rfnbo`** — *disjonction* des exceptions **réseau** : éligible si la part renouvelable instantanée ≥ seuil **OU** le prix day-ahead ≤ seuil de surplus. (Ce ne sont pas des conditions cumulatives.)
   - **`low-carbon`** — *condition nécessaire* unique : intensité ≤ seuil dérivé. **Pas** de pilier prix ni renouvelable.

3. **Seuils servis [FAIT], versionnés** dans le ruleset `rfnbo:2023-1184` : bascule `2030-01-01`, Article 4 `0,90`, surplus `20 €/MWh`. La **branche EUA** du surplus est documentée mais **non câblée** (pas de flux EUA). L'**additionnalité PPA** (route principale RFNBO, niveau site) est **hors périmètre** — exposée dans le `disclaimer`.

4. **Seuil low-carbon = proxy carbon-fr `~64 gCO₂eq/kWh` [ESTIMATION], dérivé et reproductible** :
   `94 gCO₂eq/MJ × (1 − 0,70) = 28,2 gCO₂eq/MJ` ; `× 120 MJ/kg (PCI) = 3384 gCO₂eq/kgH₂` ; `÷ 53 kWh/kg = 63,8 ⇒ round = 64 gCO₂eq/kWh`. **Sémantique = condition nécessaire** (« drapeau rouge ») : au-delà, l'électricité **à elle seule** crève le budget GHG produit → H₂ bas-carbone **impossible** ; en deçà, **candidat** (dépend des autres postes : compression, eau, auxiliaires). Étiqueté `indicative-non-regulatory`. La conso (`53 kWh/kg`) est **overridable** et **recale** le seuil (`?low_carbon_threshold_g_per_kwh=` ou via la conso). L'acte 2025/2359 ne fixe **aucun** seuil électrique.

5. **Caveat nucléaire neutre.** La reconnaissance pleine de l'électricité bas-carbone d'origine nucléaire est **en cours** côté UE (consultation d'ici 2026-06-30, évaluation d'ici 2028-07). Formulé sans parti pris dans `legal_basis`/`description`/`disclaimer`.

6. **PIÈGE 1 — bidding zone nationale.** `bidding_zone` toujours `FR` ; la part renouvelable lue est **nationale** quelle que soit la région demandée pour l'intensité. Le pilier géographique RFNBO est trivialement satisfait au national → non matérialisé en signal.

7. **PIÈGE 2 — prix borné.** Le prix day-ahead est filtré sur sa fraîcheur (≤ 1 h) ; au-delà du day-ahead, `None` → signal `Indeterminate{surplus-price}`. **Aucune extrapolation.**

8. **Report = `rfnbo:2026-revision`, statut `planned`** (D6) : présent au catalogue (transparence) mais **jamais résolu** (`400` si demandé). **Aucune date de report figée comme un fait** (son `hourly_switchover` reste `2030-01-01`, en vigueur) ; le report (2031-2033) n'apparaît qu'en texte, marqué « propositions, non adopté ».

9. **Part renouvelable future = nowcast/historique uniquement** (D4) : la part renouvelable observée n'est attribuée qu'aux créneaux `at ≤ now` (ancre **`rte-direct`**, convention canonique du mix national — pas `acv-ademe`, qui pourrait résoudre vers `@2`). Au-delà : `Indeterminate`. `MixForecast` (mix prévu) est une évolution **réservée** (le domaine est prêt : il consomme `Option<f64>`). En FR, où l'Article 4 n'est ≈ jamais atteint, la perte pratique est faible.

10. **Intervalles ADR-0011 exploités** (D17) : l'estimateur (`central`/`prudent`) est propagé (intensité reportée et classement = `expected` ou `upper`) ; pour `low-carbon`, le verdict est **`indeterminate`** quand le seuil tombe dans `[lower, upper]` (éligible si `upper ≤ seuil`, non éligible si `lower > seuil`).

11. **Mono-forecast** (D16) : un **seul** appel `forecast()` alimente la fenêtre verte ET l'éligibilité (plus de re-prévision interne via `FindGreenestWindow`).

12. **API = extension `?eligibility=`** sur `GET /v1/intensity/greenest-window` (axe **orthogonal** à `methodology`, réponse rétro-compatible) + **catalogue** `GET /v1/eligibility/rulesets`. **Pas** d'endpoint `/electrolyzer/*`. Overrides bornés exposés : `eligibility_version`, `surplus_price_eur_mwh` (≥ 0), `low_carbon_threshold_g_per_kwh` (]0, 1000]), `electrolyzer_kwh_per_kg` (]0, 200], **recale** le seuil dérivé — D4).

13. **Hors périmètre confirmé** (donnée niveau site absente) : gCO₂eq/kgH₂, certification, additionnalité PPA (et son *grandfathering* < 2028-01-01) — exposé dans le `disclaimer`.

---

## Conséquences

**Positives**
- Neutre, vérifiable, versionné ; chaque seuil porte sa base légale et son marqueur FAIT/ESTIMATION.
- Domaine 100 % pur et testable (golden tests), `core` intact, contrat API **additif**.
- En FR, `low-carbon` est réellement discriminant (base nucléaire ~30-40 qualifie ; pointes gaz ~80-150 non).
- Mono-forecast (pas de coût ×2) ; intervalles d'incertitude respectés.

**Négatives / limites (assumées)**
- Article 4 ≈ jamais déclenché en FR ; le signal `renewable-share` est **instantané** (proxy), pas l'Article 4 **annuel** légal — documenté.
- `rfnbo` futur partiellement `Indeterminate` (pas de mix prévu) ; `MixForecast` + branche EUA en réserve.
- Seuil low-carbon = **proxy** (condition nécessaire), pas un seuil réglementaire.
- Une réponse ne porte **qu'un cadre** : la neutralité repose sur la **symétrie d'accès** (catalogue + paramètre). Une **revue de neutralité adversariale** (type ADR-0024) est recommandée avant tout palier payant sur cette couche. ✅ **Conduite le 2026-07-03, verdict GREEN** après 3 correctifs (C8/C9/C14) : [`0026-revue-neutralite.md`](0026-revue-neutralite.md) — re-jouable à toute modification des rulesets servis.

---

## Alternatives envisagées

- **Endpoint dédié `/electrolyzer/*`** — *écarté* (Option A retenue : extension de `/greenest-window`).
- **Seuil low-carbon réglementaire** — *écarté* : l'acte 2025/2359 ne fixe pas de seuil électrique → proxy `indicative`.
- **`eligible (rfnbo) = tous les piliers`** — *écarté* : les exceptions réseau sont **disjonctives** (Article 4 OU surplus), pas cumulatives.
- **Article 4 instantané** (part instantanée ≥ 90 %) — *écarté* : l'Article 4 légal est une moyenne **annuelle** ; le signal instantané est servi comme proxy explicitement étiqueté, pas comme l'Article 4.
- **Ancre mix `acv-ademe`** — *écarté* : `rte-direct` est la convention canonique et plus disponible (D15).
- **Double forecast** — *écarté* (D16, mono-forecast).
- **`ForecastState` → `AppState<R>` / état fusionné** — *écarté* : contaminerait les 5 handlers de prévision ; un trait objet-safe + wrapper suffit.
- **Use-case dans `core`** — *écarté* : créerait un cycle (l'orchestration mix+prix vit côté adapter).
- **`MixForecast` immédiat / branche EUA / servir `rfnbo:2026-revision`** — *reportés* (réserve, droit non adopté).
- **Ignorer les intervalles ADR-0011** — *écarté* (D17 : l'incertitude est de premier ordre).

---

## Addendum — vérification sur sources primaires (2026-07-03)

Vérification conduite sur les textes officiels (annexe intégrale du Règl. (UE) 2025/2359 lue sur le PDF du JOUE L du 21/11/2025 ; communication AccelerateEU COM(2026) 370 final du 22/04/2026 ; recherche EUR-Lex des actes modifiant 2023/1184). Résultats :

1. **Décision 4 confirmée au verbatim** : l'annexe du 2025/2359 ne fixe **aucun seuil d'intensité électrique**. Le chiffre « 18 gCO₂eq/MJ » qui circule en source secondaire **n'apparaît nulle part** dans le texte — confusion probable avec la valeur **France** de la Table 5 (18,8 en 2019 / 18,3 en 2021, intensités moyennes nationales 2019-2023, source JRC/Eurostat) ou avec le `183 gCO₂eq/MJ` de la méthode (c). L'étiquetage `indicative-non-regulatory` du seuil dérivé `~64` est donc **maintenu**.
2. **Précision nouvelle** — l'annexe (Partie A, point 6) définit **quatre méthodes alternatives** de comptabilisation de l'électricité réseau non-100 %-renouvelable : **(a)** moyennes **annuelles** nationales (Table 5, l'une des 5 dernières années au choix) ; **(b)** moyenne **horaire** du mix de la bidding zone **prévue day-ahead par le GRT** (H-2 avant gate closure) ; **(c)** heures pleine charge → binaire `0` ou `183 gCO₂eq/MJ` (selon les heures où le prix marginal était fixé par renouvelable/nucléaire) ; **(d)** intensité **horaire de la technologie marginale** fixant le prix, si publiée par le GRT. Notre seuil dérivé correspond implicitement à une lecture de type **(a)** ; les méthodes **(b)/(d)** sont horaires — c'est exactement la famille de signaux que carbon-fr produit (cf. roadmap hydrogène).
3. **Précision d'attribution** : le comparateur fossile `94 gCO₂eq/MJ` n'est pas répété dans 2025/2359 — il y est incorporé **par renvoi au Règl. (UE) 2023/1185** ; seul le seuil de 70 % y est littéral.
4. **Décision 8 confirmée** : au 03/07/2026, **aucun acte modificatif** de 2023/1184 n'est adopté ni publié (seul 2024/1408, purement terminologique, existe). La « révision ciblée » RFNBO n'est qu'un engagement politique (AccelerateEU, T2 2026, non délivré à ce jour) ; les chiffres 2032-2033 / grandfathering ~2040 sont des fuites/notes d'analystes, **non actionnables**. `rfnbo:2026-revision` reste `planned`.
5. **Décision 5 affinée** : l'échéance « consultation nucléaire d'ici 2026-06-30 » vient d'un **considérant** (« should », non contraignant) ; l'obligation de l'**article 3** est l'évaluation au **2028-07-01**. Aucun signe de lancement effectif de la consultation au 03/07/2026.

**Signaux de veille** (chacun justifierait d'ouvrir le chantier `rfnbo:2026-revision`, mais seulement sur texte stable — idéalement adopté par le Collège) : (a) projet de texte sur le portail *Have your say* (« RFNBO » / « renewable hydrogen ») ; (b) nouvel acte dans l'historique EUR-Lex CELEX 32023R1184 ; (c) lancement effectif de la consultation nucléaire. Détail et séquencement : [`docs/roadmap-hydrogene.md`](../roadmap-hydrogene.md).

---

## Addendum — décision 9 amendée par l'ADR-0028 (2026-07-03)

La réserve « `MixForecast` » de la décision 9 est levée par l'[ADR-0028](0028-prevision-part-renouvelable-eligibilite.md) (`share-clim@1`, GATE de backtest franchi sur deux fenêtres indépendantes). La décision 9 se lit désormais :

> Part renouvelable **observée** attribuée aux créneaux `at ≤ now` (ancre `rte-direct`, inchangé, intervalle dégénéré). **Au-delà** : part **prévue** par `share-clim@1` **si** les bandes sont calibrées (sinon `Indeterminate`, comme avant), avec verdict par règle d'intervalle (ferme seulement hors recouvrement du seuil — symétrique de la décision 10/D17), provenance `forecast` servie, et **jamais** au-delà de l'horizon calibré (72 h).

Le principe cardinal est inchangé : l'indétermination reste un signal explicite, rien n'est extrapolé en silence — la prévision porte son incertitude au lieu de se taire. La revue de neutralité doit être **re-jouée** avec ce nouveau comportement (engagement du §5 de la revue).
