# Roadmap hydrogène — extension carbon-aware (ADR-0025/0026)

- **Statut** : document vivant (mis à jour à chaque jalon ou signal réglementaire)
- **Dernière mise à jour** : 2026-07-03 (vérification sources primaires, cf. addendum ADR-0026)
- **ADR liés** : [ADR-0025](adr/0025-extension-hydrogene-carbon-aware.md) (vision, couches A/B), [ADR-0026](adr/0026-methodologie-overlays-eligibilite.md) (méthodologie des overlays)

## Position

L'hydrogène est servi comme **extension de la couche carbon-aware** (pas de produit sœur) : le seul substrat à la fois carbon-relevant et temps réel est l'intensité carbone de l'électricité alimentant l'électrolyseur — donnée que carbon-fr possède déjà. Beaucoup de choses bougeront d'ici/après 2030 (révision RFNBO, RED IV, reconnaissance du nucléaire) : cette roadmap prend de l'avance en séquençant par **déclencheurs** plutôt que par dates, et en ne codant **jamais** un paramètre réglementaire non adopté (leçon ADR-0026 D6/D8).

## État des lieux (livré)

**Couche A « électrolyseur » — livrée en v0.4.0 (2026-06-21, PR #47)** :

| Brique | Où |
|---|---|
| Crate domaine pur `carbonfr-eligibility` (37 tests, zéro IO) | `crates/eligibility/` |
| Ruleset `rfnbo:2023-1184` (servi) — disjonction part renouvelable ≥ 0,90 OU prix day-ahead ≤ 20 €/MWh | `ruleset.rs` |
| Ruleset `low-carbon:2025-2359` (servi) — intensité ≤ ~64 gCO₂eq/kWh (proxy `indicative`, dérivé) | `ruleset.rs` |
| Ruleset `rfnbo:2026-revision` (**`planned`, jamais résolu** — droit non adopté) | `ruleset.rs` |
| Overlay `?eligibility=` sur `/v1/intensity/greenest-window` (additif, mono-forecast, overrides bornés) | `adapter-http` |
| Catalogue `GET /v1/eligibility/rulesets` | `adapter-http` |
| SDK TS (`greenestWindow({eligibility})`, `eligibilityRulesets()`), Bruno (4), tests HTTP (13) | `sdk/`, `bruno/` |
| Correctifs d'audit F03 (seuil borné) + F05 (batch prix unique) | v0.4.3/0.4.4 |

**Hors périmètre définitif** (disclaimer d'API) : gCO₂eq/kgH₂, certification, additionnalité PPA niveau site (+ grandfathering), observatoire structurel autonome.

## Vérité réglementaire au 2026-07-03 (vérifiée sur textes primaires)

- **Révision ciblée RFNBO** : engagement politique seulement (AccelerateEU, COM(2026) 370 final, 22/04/2026 — « targeted review » promise T2 2026). **Aucun acte modificatif de 2023/1184 adopté/publié.** Les chiffres 2032-2033 (bascule horaire, additionnalité) et ~2040 (grandfathering) = fuites/analystes, non actionnables.
- **Annexe 2025/2359** (lue intégralement) : pas de seuil électrique explicite → notre proxy `indicative` est la bonne qualification. **Quatre méthodes** de comptabilisation de l'électricité réseau, dont deux **horaires** — (b) moyenne du mix de bidding zone prévue day-ahead par le GRT, (d) technologie marginale horaire — cf. opportunité O1 ci-dessous.
- **Consultation nucléaire** (méthodologie PPA nucléaire) : échéance 30/06/2026 = considérant non contraignant ; pas de lancement constaté ; l'obligation dure est l'évaluation du **01/07/2028** (art. 3 du 2025/2359).
- **RED IV / nouvelle stratégie H₂ UE** : consultation post-2030 close le 12/06/2026 ; proposition législative annoncée « fin 2026 » (programme de travail COM(2025) 870).
- **France** : SNH II (16/04/2025) revoit les objectifs à **4,5 GW** d'électrolyse en 2030 / **8 GW** en 2035 (l'ADR-0025 citait les 6,5 GW du projet de 2023) ; soutien 4 Md€/15 ans ; transposition RED III en retard (avis motivé complémentaire du 30/01/2026, IRICC au 01/01/2027).

## Phases

| # | Chantier | Contenu | Déclencheur / condition d'entrée | Statut |
|---|---|---|---|---|
| H0 | Hygiène doc | README racine + SDK documentent `?eligibility=` et `/v1/eligibility/rulesets` ; roadmap README corrigée | — | ✅ 2026-07-03 |
| H1 | Vérité réglementaire | Vérification sources primaires (annexe 2025/2359, statut révision RFNBO, consultation nucléaire) → addendum ADR-0026 | — | ✅ 2026-07-03 |
| H2 | **GATE de neutralité** | Revue adversariale multi-agents de la sortie réelle des deux cadres (méthode ADR-0024) : RED (3 constats majeurs C8/C9/C14) → correctifs → **GREEN** — [`docs/adr/0026-revue-neutralite.md`](adr/0026-revue-neutralite.md) | Avant tout palier payant sur cette couche (recommandé par ADR-0026) | ✅ 2026-07-03 |
| H3 | **Activer `rfnbo:2026-revision`** | Figer les dates/seuils du texte adopté, passer le ruleset en `served`, addendum ADR-0026, CHANGELOG | **Texte adopté par le Collège** (pas un draft) — voir signaux de veille | ☐ bloqué droit |
| H4 | **`MixForecast`** → livré **`share-clim@1`** ([ADR-0028](adr/0028-prevision-part-renouvelable-eligibilite.md)) | Part renouvelable **prévue** (climatologie + intervalle calibré, verdict par règle d'intervalle D17, provenance servie, horizon calibré 72 h) ; gate de backtest franchi (2 fenêtres, 0 faux verdict/450) ; GATE de neutralité **re-joué GREEN** après 4 correctifs (F1/F3/F6/F12, revue §6) ; ⚠️ toujours distinct du `MixForecaster` GBDT d'ADR-0013 ; variante météo **`share-meteo@2` mesurée le 2026-07-04** (addendum ADR-0028) : GO formel sur les 2 fenêtres (−7,7 %/−6,6 % de RMSE à h+1, parité par repli au-delà de la couverture météo) mais gain global mince (borné par la convention d'archive 24 h) → **expérience non servie** (décision du 2026-07-04 ; re-jouable via `backtest-share-meteo`, à re-mesurer sur couverture météo de service) | — | ✅ 2026-07-03 |
| H5 | Branche EUA | Exception surplus `< 0,36 × prix EUA` (flux prix ETS à ingérer) | Optionnel — valeur marginale en FR (~25 vs 20 €/MWh) ; ne se justifie que si un flux EUA sert aussi un autre usage | ☐ réserve |
| H6 | **Couche B-light** → livrée **`/hydrogene`** ([ADR-0029](adr/0029-carte-electrolyseurs-carbone-live.md)) | Page carte auto-contenue (zéro CDN/tuile/lib) : 233 électrolyseurs UE géolocalisés (EHO © Clean Hydrogen JU, instantané semestriel Dec2025) × choropleth régional `acv-ademe` live + fenêtres rfnbo/low-carbon + SSE. Licences vérifiées (GISCO écarté — clause EuroGeographics ; Vig'Hy écarté — pas de licence). **v2 en attente** : couche ADEME `hyd01-sites` après confirmation écrite de licence (`cdo@ademe.fr`) | — | ✅ 2026-07-03 |
| H7 | Bascule horaire | `hourly_switchover` déjà paramétré (`2030-01-01`), se recale via H3 ; couche B-full (observatoire) reste rejetée sauf « gap dev-first confirmé » | 2030 (ou date du texte révisé) | ☐ paramétré |

## Opportunités ouvertes (non engagées, à arbitrer)

- **O1 — Positionnement « méthode horaire » du 2025/2359** : les méthodes (b) et (d) de l'annexe reposent sur l'intensité **horaire de la bidding zone** (prévue day-ahead par le GRT, ou marginale) — exactement la famille de données que carbon-fr sert (`/v1/intensity/forecast`, technologie marginale estimée de `/v1/price`). Piste : documenter (site/docs/OpenAPI) comment la donnée carbon-fr s'aligne sur ces méthodes (conversion gCO₂eq/kWh ↔ gCO₂eq/MJ = ÷3,6), sans jamais prétendre à une valeur réglementaire (les méthodes exigent la donnée **du GRT** ou une approbation d'autorité compétente). À creuser après H2.
- **O2 — Veille automatisée** : les trois signaux ci-dessous sont vérifiables par une session périodique (mensuelle) ; à défaut, re-jouer la vérification H1 à chaque évènement UE marquant.

## Signaux de veille (chacun rouvre H3)

1. Projet de texte sur le portail **Have your say** (recherche « RFNBO » / « renewable hydrogen ») — un draft publié ⇒ commencer l'analyse, **ne rien coder** avant adoption.
2. Nouvel acte dans l'**historique EUR-Lex CELEX 32023R1184** (à ce jour, seul 2024/1408 — terminologique).
3. Lancement effectif de la **consultation nucléaire** (méthodologie PPA) et, à terme, l'évaluation du 01/07/2028 (art. 3 du 2025/2359) — impacte la formulation neutre du caveat nucléaire (`legal_basis`/`disclaimer`).
4. Proposition **RED IV** (annoncée fin 2026) — surveiller le sort du quota industriel RFNBO 42 % (art. 22a RED III) et l'ouverture « techno-neutre » au bas-carbone.

## Règles de conduite (héritées ADR-0025/0026)

- **Jamais** de paramètre réglementaire non adopté dans un ruleset `served` ; les propositions restent du texte marqué « non adopté ».
- Toute évolution de méthode = **nouvelle version de ruleset** + addendum ADR — jamais de modification silencieuse (ADR-0005/0006).
- Neutralité cardinale : binôme `rfnbo`/`low-carbon` symétrique, caveat nucléaire sans parti pris, conclusions laissées à l'utilisateur.
