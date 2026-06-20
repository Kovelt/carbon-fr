# Brief Claude Code — Couche A : overlay carbon-aware « électrolyseur » (RFNBO / bas-carbone)

> Référence de gouvernance : **ADR-0025**. Ce brief implémente la couche A décrite par l'ADR. Il ne formalise pas la paramétrisation fine des seuils réglementaires — c'est l'objet de l'**ADR-0026**, à rédiger au moment de l'implémentation, et dont les valeurs par défaut sont posées ici comme configuration provisoire versionnée.

---

## 1. Objectif

Ajouter à carbon-fr une couche de **décision** qui répond à : *« pour un électrolyseur dans la zone Z, sous le cadre d'éligibilité F, quelles fenêtres temporelles sont les plus favorables à une production conforme, et quel est le signal d'éligibilité maintenant ? »*

C'est un **support à la décision sur signaux de réseau**, pas une certification. La couche réutilise le substrat temps réel existant et la machinerie `/greenest-window`.

## 2. Périmètre — ce qui est DANS / HORS

**Dans :**
- Évaluation d'**éligibilité au niveau réseau** par créneau temporel, pour deux cadres : `rfnbo` (renouvelable) et `low-carbon` (bas-carbone inclusif nucléaire/CCS).
- Recherche de **fenêtres favorables** sur l'historique/temps réel ET sur la prévision (1-72 h, réutilise le contrat `ForecastPoint`).
- Surface API + SDK + OpenAPI 3.1.

**Hors (à ne PAS implémenter) :**
- Calcul de gCO₂eq/kgH₂ et certification d'hydrogène (exige donnée au niveau site).
- Additionnalité contractuelle / PPA spécifique à une installation (contractuel, hors données réseau).
- Couche B-light (carte fusion) — projet distinct.
- Figement des seuils réglementaires définitifs — reste paramétrable jusqu'à ADR-0026.

## 3. Placement hexagonal

Respecter strictement le principe du projet : *calcul et prédiction = types/stratégies de domaine purs ; données externes et I/O = derrière des ports.*

- **Domaine (pur, sans I/O)** — crate suggéré `carbonfr-eligibility` (aligner le nom sur la convention `carbonfr-*` et le crate des primitives carbon-aware de l'ADR-0014) :
  - types d'éligibilité,
  - fonction d'évaluation pure `evaluate(slots, ruleset) -> Vec<EligibilityVerdict>`,
  - stratégie par cadre (`rfnbo`, `low-carbon`).
- **Ports (existants à réutiliser)** : le port fournissant la série temporelle régionale de mix/intensité (déjà utilisé par `/mix` et `/forecast`). **Nouveau besoin potentiel** : un port « prix day-ahead » (voir §7, décision ouverte).
- **Application** : use-case orchestrant `fetch mix/forecast (port) -> evaluate (domaine) -> fenêtres/verdict`.
- **Adapters entrants** : endpoint HTTP + SDK.

## 4. Modèle de domaine (esquisse Rust)

```rust
pub enum EligibilityFramework { Rfnbo, LowCarbon }

/// Versionné, comme les méthodologies (`served` + vintage). Valeurs par défaut provisoires (ADR-0026).
pub struct EligibilityRuleset {
    pub framework: EligibilityFramework,
    pub version: &'static str,            // ex. "rfnbo:2023-1184"
    pub temporal_granularity: TemporalGranularity, // Monthly | Hourly
    pub hourly_switchover: NaiveDate,     // défaut 2030-01-01, révisable (report 2031-2033 en débat)
    pub article4_renewable_threshold: f64, // défaut 0.90
    pub surplus_price_eur_mwh: Option<f64>, // défaut Some(20.0) ; None si pas de port prix
    pub low_carbon_intensity_threshold_g_per_kwh: Option<f64>, // pour LowCarbon ; valeur ADR-0026
}

pub struct EligibilityVerdict {
    pub timestamp: DateTime<Utc>,
    pub bidding_zone: String,             // zone de dépôt (voir §6) — PAS la sous-région
    pub eligible: bool,
    pub signals: Vec<EligibilitySignal>,  // quels piliers ont statué et comment
    pub carbon_intensity_g_per_kwh: f64,  // report informatif (méthodo orthogonale)
}

pub enum EligibilitySignal {
    Article4 { national_renewable_share: f64, passed: bool },
    PriceSurplus { price_eur_mwh: f64, passed: bool },
    RenewableCoverage { share: f64 },     // continu, pour classer les fenêtres
    LowCarbonThreshold { intensity: f64, passed: bool },
    GeographicCorrelation { passed: bool },
    TemporalCorrelation { granularity: TemporalGranularity },
}
```

## 5. Logique d'éligibilité par cadre (niveau réseau, v1 honnête)

Les deux cadres reposent sur des signaux de natures différentes — c'est le cœur de la valeur :

**`rfnbo` — basé sur la composition des sources :**
- *Article 4* : part nationale de renouvelables ≥ seuil (défaut 90 %) sur la période → l'élec réseau peut compter comme renouvelable.
- *Surplus prix* : prix day-ahead < seuil (défaut 20 €/MWh) → créneau compté comme surplus renouvelable.
- *Renewable coverage* : part renouvelable du mix au créneau — signal continu pour **classer** les fenêtres même quand aucune exception binaire ne se déclenche.

**`low-carbon` — basé sur un seuil d'intensité carbone :**
- L'acte délégué bas-carbone (adopté le 8 juillet 2025) raisonne en seuil d'émissions GHG. Or carbon-fr **dispose déjà** du gCO₂eq/kWh → un créneau est « bas-carbone qualifiant » si l'intensité ≤ seuil. Le nucléaire qualifie naturellement (déjà bas dans la donnée). Seuil précis à fixer en ADR-0026.

**Corrélation temporelle** : granularité `Hourly` → chaque heure évaluée indépendamment ; `Monthly` → agrégat mensuel. Implémenter d'abord `Hourly` (cas 2030, le plus structurant), exposer la granularité en paramètre.

## 6. ⚠️ Piège 1 — zone de dépôt ≠ sous-région carbone

La corrélation **géographique** RFNBO s'évalue à la maille de la **zone de dépôt** (« bidding zone »), qui pour la France est **nationale** (FR = une seule zone). Les 12 régions de carbon-fr servent à l'intensité carbone, **pas** au pilier géographique d'éligibilité. Ne pas confondre : l'éligibilité géographique se calcule au national ; la donnée régionale reste utile pour le classement carbone des fenêtres. Documenter ce choix dans le code.

## 7. ⚠️ Piège 2 — exception « surplus prix » = dépendance donnée

L'exception < 20 €/MWh exige une donnée de **prix day-ahead** (disponible via ENTSO-E Transparency, déjà utilisé pour les flux transfrontaliers). **Décision ouverte pour Morgan** :
- **Option A (recommandée pour v1)** : livrer sans le signal prix (`surplus_price_eur_mwh: None`), couvrir Article 4 + renewable-coverage + low-carbon-threshold, et marquer le signal prix en TODO documenté (nécessite port prix).
- **Option B** : ajouter dès v1 un port/adapter « prix day-ahead » (ENTSO-E) et activer le signal.

Ne pas coder le signal prix en dur sans port.

## 8. Surface API (proposition — à confirmer)

Deux options, **A privilégiée** :

- **Option A — extension de `/greenest-window`** : ajouter un paramètre optionnel `eligibility` (`framework` + `ruleset version` + overrides). Quand présent, les fenêtres sont **filtrées/annotées** par l'éligibilité. Élégant, réutilise la machinerie existante. Axe éligibilité **orthogonal** à la méthodologie carbone (`rte-direct`/`acv-ademe`) : ce sont deux paramètres indépendants.
- **Option B — endpoint dédié** `GET /electrolyzer/windows` et `GET /electrolyzer/eligibility/now`, si on veut une surface produit distincte.

Réponse : liste de fenêtres avec `start`, `end`, `eligible`, `signals`, `carbon_intensity`, classées par favorabilité.

## 9. SDK & OpenAPI

- `@carbon-fr/sdk` : helper typé, ex. `client.electrolyzer.windows({ framework, region, horizonHours, ruleset })`.
- OpenAPI 3.1 : décrire les nouveaux paramètres/schémas, exposés dans Swagger UI `/docs`. Énumérer `framework` et les versions de ruleset `served`.

## 10. Configuration & versionnement

- Rulesets **versionnés** (`rfnbo:2023-1184`, futur `low-carbon:2025`, etc.), à l'image des méthodologies `served`.
- Aucune valeur réglementaire codée en dur : tout passe par la config du ruleset.
- Prévoir l'ajout de nouvelles versions sans rupture (le report horaire 2030→2033 et le seuil Article 4 90 %→70-85 % sont en débat).

## 11. Tests

La logique de domaine étant pure, viser une couverture par **cas-or (golden cases)** :
- Article 4 juste au-dessus / en dessous du seuil.
- Bascule de granularité temporelle autour de `hourly_switchover`.
- `low-carbon` : créneau nucléaire qualifiant vs créneau gaz non qualifiant.
- `rfnbo` vs `low-carbon` sur le même mix (résultats divergents attendus).
- Fenêtres sur prévision (`ForecastPoint`) avec intervalles de confiance.

## 12. Definition of Done

- [ ] Crate domaine pur avec types + stratégies + évaluation, sans I/O.
- [ ] Use-case application câblé sur les ports existants.
- [ ] Surface API (option retenue) + SDK + OpenAPI à jour, visibles dans `/docs`.
- [ ] Piège 1 (zone de dépôt nationale) respecté et commenté.
- [ ] Piège 2 (prix) tranché selon §7, sans règle codée en dur.
- [ ] Golden tests verts.
- [ ] ADR-0026 ouvert pour acter les valeurs de seuils définitives.

## 13. Branche / commits suggérés

- Branche : `feat/electrolyzer-eligibility-layer`
- Commits atomiques : (1) crate domaine + types, (2) stratégies rfnbo/low-carbon + tests, (3) use-case + port(s), (4) surface API, (5) SDK, (6) OpenAPI/docs.
