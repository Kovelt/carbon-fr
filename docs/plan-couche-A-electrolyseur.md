# PLAN FINAL — Couche A « éligibilité électrolyseur » (carbon-fr, ADR-0026)

> Lentille : pureté hexagonale (core pur, `time` jamais chrono) + exactitude réglementaire/neutralité ADR-0025 + plus petit diff sensé. Décisions figées : **Option A** (étendre `GET /v1/intensity/greenest-window` via `?eligibility=`) + **Option B** (signal prix câblé sur `SpotPriceRepository`). Branche : `feat/electrolyzer-eligibility-layer`. **Livrable = plan ; ne pas committer/pusher sans feu vert.**
> Tous les faits ci-dessous sont vérifiés contre le code réel (chemins + lignes cités).

---

## A. Décisions tranchées

| # | Question | Décision retenue | Motif |
|---|---|---|---|
| D1 | Où vit le crate domaine ? | Crate **`carbonfr-eligibility`** (dossier `crates/eligibility/`), dépend de `carbonfr-core` (sens unique), 100 % pur : deps = `carbonfr-core` + `time` **uniquement** (PAS `thiserror` — `evaluate` est total, aucun `Result`). | Évite le cycle ; règle d'or. Le constat DOMAINE proposait `thiserror` ; on le **retire** car aucun type d'erreur n'est requis (l'indétermination est portée par `EligibilitySignal::Indeterminate`). |
| D2 | Où vit l'orchestration ? | **Fonction libre privée de l'adapter HTTP** (`crates/adapter-http/src/eligibility_uc.rs`). `core` **strictement intact**. | Calqué sur `MethodologiesResponse::catalog()` (vit côté HTTP). C'est l'adapter, seule couche dépendant des deux crates, qui appelle `evaluate()`. |
| D3 | Câblage de l'état HTTP ? | Champ `Option<Arc<dyn EligibilityRepo + Send + Sync>>` sur `ForecastState` + **blanket impl** `EligibilityRepo for R: IntensityRepository + SpotPriceRepository` dans l'adapter. `greenest_window<F>` et `router<R,F>` inchangés en signature. | Suit exactement `consumption: Option<Arc<dyn ForecastModel + Send + Sync>>` (lib.rs:133-134). `+ Send + Sync` explicite **par parité** (corrige le style relevé par hexa-compil). |
| D4 | Part renouvelable sur l'horizon futur ? | **Nowcast/historique uniquement.** `renewable_share`/`national_renewable_share` du dernier mix observé pour `at ≤ now_at` ; `None` (→ `Indeterminate`) au-delà. `low-carbon` (intensité) servi sur **tout** l'horizon. | `ForecastPoint` ne porte pas le mix (vérifié forecast_point.rs:47-60). Honnête. `MixForecast` en réserve ADR-0026 (domaine prêt : consomme `Option<f64>`). |
| D5 | Endpoint catalogue ? | **OUI** : `GET /v1/eligibility/rulesets` (handler sans state, calqué sur `methodologies()`). | Vérifiabilité/neutralité. |
| D6 | Report horaire ? | **Servir uniquement le droit en vigueur** (`rfnbo:2023-1184`, `low-carbon:2025-2359` = `served`). Le report = `rfnbo:2026-revision` `status=planned`, présent au catalogue mais **jamais résolu** (400 si demandé). **Aucune date ferme** dans son `hourly_switchover` factuel : il garde `2030-01-01` (= droit en vigueur, marqué spéculatif dans `description`). | Droit non adopté ; ADR-0006/0020. **Correctif reg-neutral** : ne pas coder de date de report comme un fait. |
| D7 | Seuil low-carbon ? | **Proxy carbon-fr 60 gCO₂eq/kWh**, étiqueté `indicative` (`basis="indicative-non-regulatory"`). Conso 53 kWh/kg versionnée, overridable. Dérivation **rendue reproductible** (cf. §Valeurs). | L'acte 2025/2359 ne fixe aucun seuil **électrique** (fixe 28,2 gCO₂eq/MJ). |
| D8 | Condition EUA du surplus ? | **Non câblée en v1** (pas de flux EUA). Seul le seuil 20 €/MWh. **Documentée dans le `disclaimer`/`description`** (le surplus a 2 branches dont une non évaluée). | Pas de donnée EUA ; ne rien inventer. **Correctif reg-neutral** : mentionner la branche EUA côté API, pas seulement en TODO interne. |
| D9 | `Date` const sans `expect` ? | `Date::from_calendar_date(...).expect("date littérale valide")` dans les fabriques de constantes. PAS de feature `time/macros`. | Acceptable (équiv. `TrvReference::trv_2026()`). `macros` impacterait tout le workspace + `cargo deny`. |
| D10 | Pilier prix sous `low-carbon` ? | **Désactivé** : `surplus_price_eur_mwh = None` pour `low-carbon:2025-2359`. | Le surplus-prix est un pilier **RFNBO** de corrélation temporelle, hors champ low-carbon. L'activer ferait sortir `eligible=false` des créneaux nucléaires bas-carbone (prix indéterminé au-delà du day-ahead). |
| D11 | Sémantique de `?eligibility=` ? | **Non destructif** : la fenêtre verte classique reste inchangée ; on **ajoute** un bloc `eligibility`. | Rétro-compat stricte. |
| D12 | Erreur de l'overlay ? | **Best-effort `Option`** dans `EligibilityRepo` (jamais 500) ; si `?eligibility=` présent mais état non câblé → **503** propre (`ApiError::unavailable`). | Indétermination > échec dur. Le 503 n'arrive qu'en mauvaise config (jamais en prod). |
| D13 | `evaluate()` signature ? | `evaluate(slots: &[SlotInput], ruleset: &EligibilityRuleset, bidding_zone: &str) -> Vec<EligibilityVerdict>` — le framework est porté par `ruleset.framework`. | Moins de paramètres, cohérence catalogue. (Le constat DOMAINE listait `framework` séparé ; on le **fusionne** dans le ruleset — `EligibilityRuleset` gagne un champ `framework`.) |
| D14 | `renewable_share` : où ? | Dans `carbonfr-eligibility` (réimplémente la convention privée `price::mix_shares`, price.rs:277). PAS ajouté à `core::GenerationMix`. | `mix_shares` est privée/non réexportée. Test golden ancré sur ≈0,2547 garde la convention synchronisée. |
| **D15** | **Ancre du mix nowcast ?** (correctif 3 critiques) | **`rte-direct`**, PAS `acv-ademe`. Méthode renommée `latest_national_mix`. | `get_price.rs:22` `ANCHOR_METHODOLOGY="rte-direct"` est la convention canonique du mix national (aligné `/v1/intensity/now` + `/v1/mix` + handlers.rs:~1284). `rte-direct` est strictement plus disponible et porte le même mix ; `acv-ademe` peut résoudre vers `@2` (consumption, mix incertain) car `latest()` filtre l'id **sans la version** (ports.rs:89). |
| **D16** | **Double `forecast()` ?** (correctif 3 critiques) | **Un seul `forecast()`**. On n'appelle PLUS `FindGreenestWindow::execute` sur ce chemin : on appelle `state.forecaster.forecast(...)` **une fois** → `Vec<ForecastPoint>`, puis (a) `core::domain::greenest_window(&points, window, estimator)` (fonction pure, window.rs:34) pour la fenêtre, (b) on réutilise les **mêmes** points pour `evaluate_eligibility`. | `FindGreenestWindow::execute` (find_greenest_window.rs:39-43) forecast en interne puis jette les points → re-forecast = coût ×2 + risque de divergence. **NB** : le handler actuel ignore déjà `state.consumption` pour greenest-window (vérifié handlers.rs:705) → on conserve ce comportement (greenest-window = chemin `forecaster` scalaire ; pas de `acv-ademe@2` ici, comme aujourd'hui). |
| **D17** | **Intervalles de confiance ?** (correctif majeur completude/brief §11) | `?estimator=` est **propagé** jusqu'à `evaluate_eligibility` : `SlotInput.intensity = expected` (central) ou `upper` (prudent). En plus, le DTO `EligibilitySlotBody` **expose `intensity_lower`/`intensity_upper`**, et `low-carbon` marque le créneau **`indeterminate`** quand le seuil tombe dans `[lower, upper]` (signal `LowCarbonIntensity` → variante indéterminée par recouvrement). | ADR-0011 pose l'incertitude comme propriété de 1er ordre. Un créneau low-carbon éligible en central peut basculer en prudent : c'est exactement le cas d'usage. |

---

## B. Vue d'ensemble du flux (un seul forecast)

```
state.forecaster.forecast(region,&methodology,from,horizon) ─► Vec<ForecastPoint> (UNE fois)
        ├──► core::domain::greenest_window(&points, window, estimator) ──► GreenWindow (fenêtre verte, inchangée)
        └──► eligibility_uc::evaluate_eligibility(repo, &points, &ruleset, estimator)
                 EligibilityRepo.latest_national_mix() ► Measurement(rte-direct).mix ► renewable_share() (nowcast→Option)
                 EligibilityRepo.spot_price_at(at)     ► Option<f64> (day-ahead, filtré ≤1h → Option)
                       │ assemble Vec<SlotInput> (intensity = expected|upper selon estimator ; lower/upper portés)
                       ▼ carbonfr_eligibility::evaluate(&slots, &ruleset, "FR")
                 Vec<EligibilityVerdict>
        GreenestWindowResponse { …, eligibility: Option<EligibilityBody> }
```

---

## C. Arbre des fichiers

**Créés :**
- `crates/eligibility/Cargo.toml`
- `crates/eligibility/src/lib.rs` (façade + doc neutralité + reexports)
- `crates/eligibility/src/ruleset.rs` (`EligibilityFramework`, `TemporalGranularity`, `RulesetStatus`, `EligibilityRuleset`, fabriques, `ruleset_catalog`, `resolve_ruleset`, `with_overrides`)
- `crates/eligibility/src/verdict.rs` (`Pillar`, `EligibilitySignal`, `EligibilityVerdict`, `SlotInput`, `FR_BIDDING_ZONE`, `basis_of`)
- `crates/eligibility/src/share.rs` (`renewable_share`)
- `crates/eligibility/src/evaluate.rs` (`evaluate_slot`, `evaluate`, `score`, `rank_by_score`, `best_by_score`) — golden tests `#[cfg(test)]`
- `crates/adapter-http/src/eligibility_uc.rs` (fonction libre + tests use-case avec fake `EligibilityRepo`)
- `docs/adr/0026-methodologie-overlays-eligibilite.md`
- `bruno/greenest-window-eligibility-rfnbo.bru`, `bruno/greenest-window-eligibility-low-carbon.bru`, `bruno/eligibility-rulesets.bru`, `bruno/error-eligibility-framework-invalid.bru`

**Modifiés :**
- `Cargo.toml` (racine : `[workspace] members` + section crates internes)
- `crates/adapter-http/Cargo.toml` (**ajouter `async-trait = { workspace = true }` en `[dependencies]`** + `carbonfr-eligibility = { workspace = true }`) — **CORRECTIF BLOQUANT**
- `crates/adapter-http/src/lib.rs` (trait `EligibilityRepo` + blanket impl + champ `eligibility` + `with_eligibility` + `mod eligibility_uc;`)
- `crates/adapter-http/src/handlers.rs` (`GreenestWindowQuery` étendu + branche dans `greenest_window<F>` refactorée mono-forecast + handler `eligibility_rulesets`)
- `crates/adapter-http/src/dto.rs` (DTO enrichis + `RulesetsResponse::catalog()` + disclaimer)
- `crates/adapter-http/src/carbonfr_openapi.rs` (paths + schemas + tag + tests internes)
- `crates/adapter-http/tests/openapi.snapshot.json` (régénéré, relu en PR)
- `bin/server/src/main.rs` (`.with_eligibility(std::sync::Arc::new(repo.clone()))`)
- `sdk/typescript/src/{types.ts,client.ts}` (+ `index.ts` si besoin)
- `CLAUDE.md` racine (entrée ADR-0026 ; bascule couche A `[x]` ; statut ADR-0025 → implémenté)
- `docs/adr/README.md` (**ferme** : ligne 0026 + bascule statut 0025)
- `docs/adr/0025-extension-hydrogene-carbon-aware.md` (« Suite pressentie : ADR-0026 » → effective)

> **`core` n'est PAS modifié. `bin/server/Cargo.toml` n'est PAS modifié.**

---

## 1. Crate domaine pur `carbonfr-eligibility`

### 1.1 `crates/eligibility/Cargo.toml`
```toml
[package]
name = "carbonfr-eligibility"
version.workspace = true
description = "Couche A « électrolyseur » de carbon-fr : éligibilité réseau RFNBO / bas-carbone (domaine pur, sans IO)."
edition.workspace = true
license.workspace = true
repository.workspace = true
publish = false

[dependencies]
carbonfr-core = { workspace = true }
time = { workspace = true }
```
> `edition.workspace = true` (Cargo.toml racine sous `[workspace.package] edition = 2024` — vérifié). **Pas de `thiserror`** : `evaluate` est total (divergence assumée avec le constat DOMAINE qui le proposait — aucun `Result` produit). Pas de `serde`/`sqlx`/`axum`/`reqwest` (ADR-0002).

### 1.2 `Cargo.toml` racine (2 edits)
- `[workspace] members` : ajouter `"crates/eligibility",`.
- Section crates internes de `[workspace.dependencies]` : ajouter `carbonfr-eligibility = { path = "crates/eligibility" }`.

### 1.3 `crates/eligibility/src/ruleset.rs`
```rust
use time::{Date, Month, OffsetDateTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EligibilityFramework { Rfnbo, LowCarbon }
impl EligibilityFramework {
    pub fn slug(self) -> &'static str { /* "rfnbo" | "low-carbon" */ }
    pub fn from_slug(s: &str) -> Option<Self>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalGranularity { Monthly, Hourly }
impl TemporalGranularity { pub fn slug(self) -> &'static str; } // "monthly"|"hourly"

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesetStatus { Served, Planned }
impl RulesetStatus { pub fn slug(self) -> &'static str; } // "served"|"planned"

#[derive(Debug, Clone, PartialEq)]
pub struct EligibilityRuleset {
    pub version: &'static str,                  // "rfnbo:2023-1184"
    pub framework: EligibilityFramework,        // D13 : porte le cadre
    pub status: RulesetStatus,
    pub adr: &'static str,                       // "ADR-0026"
    /// Granularité/bascule = piliers RFNBO uniquement. Sous low-carbon ces champs
    /// sont du bruit sémantique → catalogue les signale "n/a (pilier rfnbo)".
    pub granularity: TemporalGranularity,
    pub hourly_switchover: Date,
    pub article4_renewable_threshold: f64,
    pub surplus_price_eur_mwh: Option<f64>,
    pub low_carbon_intensity_threshold_g_per_kwh: Option<f64>,
    pub low_carbon_intensity_is_indicative: bool,
    pub electrolyzer_kwh_per_kg: f64,
    pub overridden: bool,
    pub legal_basis: &'static str,               // citation textuelle UE + marqueur [FAIT]/[ESTIMATION]
}

impl EligibilityRuleset {
    pub fn rfnbo_2023_1184() -> Self;       // served — voir §Valeurs
    pub fn low_carbon_2025_2359() -> Self;  // served
    pub fn rfnbo_2026_revision() -> Self;   // planned (catalogue uniquement)

    /// Granularité effective (= rfnbo ; renvoyée telle quelle pour low-carbon).
    pub fn granularity_at(&self, at: OffsetDateTime) -> TemporalGranularity {
        if at.date() >= self.hourly_switchover { TemporalGranularity::Hourly } else { self.granularity }
    }

    /// Overrides bornés. Conserve `version`, pose `overridden=true`. Ordre des
    /// paramètres FIGÉ (corrige l'incohérence §1.3/§4.2 du plan canonique).
    pub fn with_overrides(mut self,
                          surplus_price_eur_mwh: Option<f64>,
                          low_carbon_intensity_threshold_g_per_kwh: Option<f64>,
                          electrolyzer_kwh_per_kg: Option<f64>) -> Self;
}

pub fn ruleset_catalog() -> Vec<EligibilityRuleset> {
    vec![EligibilityRuleset::rfnbo_2023_1184(),
         EligibilityRuleset::low_carbon_2025_2359(),
         EligibilityRuleset::rfnbo_2026_revision()] // planned
}

/// Résout un ruleset SERVI par cadre + version optionnelle. None si inconnu OU `planned`.
pub fn resolve_ruleset(framework: EligibilityFramework, version: Option<&str>) -> Option<EligibilityRuleset>;
```
> `Date::from_calendar_date(2030, Month::January, 1).expect("date littérale valide")` dans les fabriques (seul `expect` toléré, fabrique de constante). `resolve_ruleset` ignore les `Planned` → 400 au bord si demandé (D6).

### 1.4 `crates/eligibility/src/verdict.rs`
```rust
use time::OffsetDateTime;
use carbonfr_core::domain::CarbonIntensity;
use crate::ruleset::EligibilityFramework;

pub const FR_BIDDING_ZONE: &str = "FR";   // PIÈGE 1 : FR = 1 bidding zone nationale

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotInput {
    pub at: OffsetDateTime,
    /// Intensité retenue pour le verdict (= expected en central, = upper en prudent — D17).
    pub intensity: CarbonIntensity,
    /// Bornes de l'intervalle de confiance (ADR-0011), portées pour l'auditabilité
    /// et la règle "seuil dans [lower,upper] => indéterminé" (D17).
    pub intensity_lower: CarbonIntensity,
    pub intensity_upper: CarbonIntensity,
    pub renewable_share: Option<f64>,
    pub national_renewable_share: Option<f64>,
    pub spot_price_eur_mwh: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pillar { Article4, RenewableCoverage, LowCarbonIntensity, SurplusPrice }
impl Pillar { pub fn slug(self) -> &'static str; }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EligibilitySignal {
    Article4 { renewable_share: f64, threshold: f64, passed: bool },
    RenewableCoverage { renewable_share: f64, threshold: f64, passed: bool },
    LowCarbonIntensity { intensity_g_per_kwh: f64, threshold: f64, indicative: bool, passed: bool },
    SurplusPrice { spot_price_eur_mwh: f64, threshold: f64, passed: bool },
    Indeterminate { pillar: Pillar },
}
impl EligibilitySignal {
    pub fn passed(self) -> Option<bool>;   // None si Indeterminate
    pub fn pillar(self) -> Pillar;
}

/// Base réglementaire d'un pilier (pour le DTO, y compris Indeterminate).
/// LowCarbonIntensity => "indicative-non-regulatory" ; sinon "regulatory".
/// CORRECTIF hexa-compil : le DTO requiert `basis` même pour Indeterminate.
pub fn basis_of(pillar: Pillar) -> &'static str;

#[derive(Debug, Clone, PartialEq)]
pub struct EligibilityVerdict {
    pub timestamp: OffsetDateTime,
    pub bidding_zone: String,              // toujours "FR" (PIÈGE 1)
    pub framework: EligibilityFramework,
    pub ruleset_version: &'static str,
    pub eligible: bool,
    pub signals: Vec<EligibilitySignal>,
    pub carbon_intensity: CarbonIntensity, // = SlotInput.intensity (estimateur retenu)
    pub intensity_lower: CarbonIntensity,
    pub intensity_upper: CarbonIntensity,
    pub score: f64,                        // plus bas = meilleur
}
```

### 1.5 `crates/eligibility/src/share.rs`
```rust
use carbonfr_core::domain::GenerationMix;

/// Part renouvelable [0,1], None si production nulle. Réimplémente la convention
/// privée `price::mix_shares` (price.rs:277) : pompage+echanges EXCLUS, `max(0.0)`,
/// branche `thermique=Some` (régional) sinon gaz+charbon+fioul (national).
/// Numérateur = eolien+solaire+hydraulique+bioenergies (nucléaire EXCLU — pilier rfnbo).
pub fn renewable_share(mix: &GenerationMix) -> Option<f64>;
```
> Vérifié : sur `national_mix()` (price.rs:425, nucleaire 38815 / hydraulique 8893 / eolien 2555 / solaire 1050 / bioénergies 1006 / gaz 666 / fioul 34 / charbon 0) → total 53019, renouvelable 13504 → **≈ 0,2547**. Test golden #1 ancre cette valeur.

### 1.6 `crates/eligibility/src/evaluate.rs`
```rust
pub fn evaluate_slot(slot: &SlotInput, ruleset: &EligibilityRuleset, bidding_zone: &str) -> EligibilityVerdict;
pub fn evaluate(slots: &[SlotInput], ruleset: &EligibilityRuleset, bidding_zone: &str) -> Vec<EligibilityVerdict>;
fn score(slot: &SlotInput, ruleset: &EligibilityRuleset) -> f64;
pub fn rank_by_score(verdicts: Vec<EligibilityVerdict>) -> Vec<EligibilityVerdict>; // NaN en queue
/// CORRECTIF hexa-compil : variante empruntée pour `from_verdicts(&[...])` sans clone/collect.
pub fn best_by_score(verdicts: &[EligibilityVerdict]) -> Option<&EligibilityVerdict>;
```
Stratégie par `ruleset.framework` :
- **Rfnbo** : `Article4` (`national_renewable_share` vs `article4_renewable_threshold` ; `None`→`Indeterminate{Article4}`) + `RenewableCoverage` (`renewable_share` ; `None`→`Indeterminate{RenewableCoverage}`) + `SurplusPrice` si `surplus_price_eur_mwh=Some` (`None`→`Indeterminate{SurplusPrice}`, **aucune extrapolation**).
- **LowCarbon** : `LowCarbonIntensity` avec **règle d'intervalle (D17)** :
  - `slot.intensity_upper.value() <= threshold` → `passed:true` ;
  - `slot.intensity_lower.value() > threshold` → `passed:false` ;
  - sinon (seuil ∈ ]lower, upper]) → `EligibilitySignal::Indeterminate{LowCarbonIntensity}` (le verdict ne peut pas trancher dans l'incertitude). `indicative = ruleset.low_carbon_intensity_is_indicative`. **Pas** de pilier prix (D10).
  - NB : en estimateur **central**, `intensity = expected` ; la comparaison « recouvrement » utilise `lower`/`upper` portés → un créneau central-éligible mais dont l'intervalle franchit le seuil est honnêtement indéterminé.
- `eligible = !signals.is_empty() && signals.iter().all(|s| matches!(s.passed(), Some(true)))` — tout `Indeterminate` ⇒ `eligible=false`.
- `score` : low-carbon → `slot.intensity.value()` (homogène greenest-window) ; rfnbo → `renew_gap*100 + price_gap*100 + intensity*1e-3` (indétermination = pénalité maximale, NaN-safe).

### 1.7 `crates/eligibility/src/lib.rs`
Doc de neutralité (ADR-0025) en tête + `pub use ruleset::*; pub use verdict::*; pub use share::renewable_share; pub use evaluate::*;`.

---

## 2. Use-case d'assemblage — `crates/adapter-http/src/eligibility_uc.rs`

`core` intact (D2). Fonction libre privée, opérant sur `&dyn EligibilityRepo`, best-effort `Option`, **estimateur propagé (D17)** :
```rust
use carbonfr_core::domain::{ForecastPoint, WindowEstimator};
use carbonfr_eligibility::{EligibilityRuleset, EligibilityVerdict, SlotInput, evaluate,
                           renewable_share, FR_BIDDING_ZONE};

pub(crate) async fn evaluate_eligibility(
    repo: &dyn crate::EligibilityRepo,
    points: &[ForecastPoint],
    ruleset: &EligibilityRuleset,
    estimator: WindowEstimator,           // D17 : central=expected, prudent=upper
) -> Vec<EligibilityVerdict> {
    // 1. Mix nowcast NATIONAL ancré rte-direct (D15) → part renouvelable + borne de fraîcheur.
    let latest = repo.latest_national_mix().await;              // Option<Measurement>
    let now_at = latest.as_ref().map(|m| m.at);
    let now_share = latest.as_ref().and_then(|m| m.mix.as_ref()).and_then(renewable_share);
    // 2. Assemblage par créneau (un seul forecast en amont — D16).
    let mut slots = Vec::with_capacity(points.len());
    for p in points {
        let spot = repo.spot_price_at(p.at).await;             // Option<f64>, déjà filtré ≤1h (PIÈGE 2)
        let is_nowcast = now_at.map(|t| p.at <= t).unwrap_or(false);  // D4 : nowcast only
        let (rs, nrs) = if is_nowcast { (now_share, now_share) } else { (None, None) };
        let intensity = match estimator {
            WindowEstimator::Central => p.expected,
            WindowEstimator::Prudent => p.upper,
        };
        slots.push(SlotInput {
            at: p.at, intensity,
            intensity_lower: p.lower, intensity_upper: p.upper, // D17
            renewable_share: rs, national_renewable_share: nrs,
            spot_price_eur_mwh: spot,
        });
    }
    evaluate(&slots, ruleset, FR_BIDDING_ZONE)
}
```
> Enregistrer `mod eligibility_uc;` dans `lib.rs`.

---

## 3. Câblage HTTP — `crates/adapter-http/src/lib.rs`

**CORRECTIF BLOQUANT** : `async-trait` ajouté en `[dependencies]` (cf. §C). Trait objet-safe (pas un port core) + blanket impl + champ d'état :
```rust
use carbonfr_core::domain::{Measurement, Region};
use carbonfr_core::ports::{IntensityRepository, SpotPriceRepository};

/// Accès minimal de l'overlay d'éligibilité (mix nowcast + prix spot). Trait
/// objet-safe (dispatch dynamique) pour ne pas contaminer le `F` générique du
/// chemin de prévision — même motif que `consumption: Arc<dyn ForecastModel + Send + Sync>`.
#[async_trait::async_trait]
pub trait EligibilityRepo: Send + Sync {
    async fn latest_national_mix(&self) -> Option<Measurement>;   // D15 : rte-direct
    async fn spot_price_at(&self, at: time::OffsetDateTime) -> Option<f64>;
}

#[async_trait::async_trait]
impl<R: IntensityRepository + SpotPriceRepository> EligibilityRepo for R {
    async fn latest_national_mix(&self) -> Option<Measurement> {
        // D15 : ancre canonique du mix national (get_price.rs:22 ANCHOR_METHODOLOGY).
        self.latest(Region::National, "rte-direct").await.ok().flatten()
    }
    async fn spot_price_at(&self, at: time::OffsetDateTime) -> Option<f64> {
        // PIÈGE 2 : price_at renvoie le prix au plus proche <= at. On REFUSE un prix
        // périmé de plus d'1h pour ne pas propager le dernier day-ahead sur le futur.
        self.price_at(at).await.ok().flatten()
            .filter(|p| (at - p.at).whole_hours().abs() <= 1)
            .map(|p| p.eur_per_mwh)
    }
}
```
Sur `ForecastState<F>` : `pub(crate) eligibility: Option<std::sync::Arc<dyn EligibilityRepo + Send + Sync>>` (init `None` dans `new`) + builder `with_eligibility(mut self, repo) -> Self`. `router<R,F>` et `greenest_window<F>` **inchangés en signature**. `Arc<PgIntensityRepository>` satisfait le blanket impl (unsize coercion). Vérifier au `cargo check` du commit 2 l'absence de conflit de cohérence.

---

## 4. Handler `greenest_window` (mono-forecast) + query + DTO

### 4.1 `GreenestWindowQuery` (4 champs ajoutés, axe orthogonal à `methodology`)
```rust
/// Cadre d'éligibilité : `rfnbo` ou `low-carbon` (ADR-0025/0026). Absent => réponse historique inchangée.
eligibility: Option<String>,
/// Version de ruleset (ex. `2023-1184`). Défaut = ruleset servi du cadre.
eligibility_version: Option<String>,
/// Override du seuil de surplus prix (€/MWh).
surplus_price_eur_mwh: Option<f64>,
/// Override du seuil d'intensité bas-carbone (g/kWh, indicatif).
low_carbon_threshold_g_per_kwh: Option<f64>,
```

### 4.2 Logique refactorée — UN SEUL forecast (D16), estimateur propagé (D17)
```rust
// ... validations existantes (region, methodology, from, horizon, window_minutes) ...
let estimator = resolve_estimator(&query.estimator)?;

// D16 : un seul appel forecast(), réutilisé pour la fenêtre ET l'éligibilité.
let points = state.forecaster
    .forecast(region, &methodology, from, Duration::hours(horizon_hours as i64))
    .await?;
let window = greenest_window(&points, Duration::minutes(window_minutes as i64), estimator)
    .ok_or_else(|| ApiError::not_found("série insuffisante pour déterminer un créneau"))?;

let eligibility = match &query.eligibility {
    None => None,
    Some(slug) => {
        let framework = EligibilityFramework::from_slug(slug)
            .ok_or_else(|| ApiError::bad_request(format!(
                "`eligibility` doit valoir `rfnbo` ou `low-carbon` (reçu : {slug})")))?;
        // bornes overrides : prix fini >= 0 ; seuil g/kWh ∈ ]0,1000] (sinon 400).
        validate_eligibility_overrides(query.surplus_price_eur_mwh, query.low_carbon_threshold_g_per_kwh)?;
        let ruleset = resolve_ruleset(framework, query.eligibility_version.as_deref())
            .ok_or_else(|| ApiError::bad_request(
                "version de ruleset inconnue ou planifiée (non servie) pour ce cadre"))?
            .with_overrides(query.surplus_price_eur_mwh, query.low_carbon_threshold_g_per_kwh, None);
        let repo = state.eligibility.as_ref()
            .ok_or_else(|| ApiError::unavailable("overlay d'éligibilité non câblé (prix spot requis)"))?; // D12 → 503
        let verdicts = eligibility_uc::evaluate_eligibility(repo.as_ref(), &points, &ruleset, estimator).await;
        Some(EligibilityBody::from_verdicts(framework, &ruleset, &window, &verdicts))
    }
};
Ok(Json(GreenestWindowResponse::new(region.slug(), &methodology, &state.model, &window, eligibility)?))
```
> `FindGreenestWindow::execute` n'est plus appelé sur ce chemin (documenter le contournement). Comportement `acv-ademe@2` inchangé : greenest-window n'a jamais consulté `state.consumption` (handlers.rs:705) → on conserve. `ApiError::unavailable` = 503 (à vérifier/ajouter dans `error.rs` si absent ; sinon `ApiError::service_unavailable`).

### 4.3 DTO (`dto.rs`) — `eligibility` optionnel `skip_serializing_if` (rétro-compat byte-identique)
```rust
#[derive(Serialize, ToSchema)]
pub(crate) struct GreenestWindowResponse {
    region: String, methodology: String, model: String,
    start: String, end: String, unit: &'static str, average_intensity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    eligibility: Option<EligibilityBody>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct EligibilityBody {
    framework: &'static str,            // "rfnbo" | "low-carbon"
    ruleset_version: &'static str,
    ruleset_status: &'static str,       // "served"
    overridden: bool,
    bidding_zone: &'static str,         // "FR" (PIÈGE 1, jamais une région)
    disclaimer: &'static str,           // note neutre obligatoire (ADR-0025)
    window_eligible: bool,              // verdict du créneau retenu ∈ [start,end)
    best_eligible: Option<EligibleSlotBody>,
    count_eligible: usize,
    count_indeterminate: usize,
    slots: Vec<EligibilitySlotBody>,
}
#[derive(Serialize, ToSchema)] struct EligibleSlotBody { timestamp: String, intensity: f64, intensity_lower: f64, intensity_upper: f64, score: f64 }
#[derive(Serialize, ToSchema)]
struct EligibilitySlotBody {
    timestamp: String, eligible: bool,
    intensity: f64, intensity_lower: f64, intensity_upper: f64, // D17 : intervalle exposé
    score: f64, signals: Vec<EligibilitySignalBody>,
}
#[derive(Serialize, ToSchema)]
struct EligibilitySignalBody {
    pillar: &'static str,               // Pillar::slug()
    verdict: &'static str,              // "pass" | "fail" | "indeterminate" (PIÈGE 2 + D17)
    #[serde(skip_serializing_if="Option::is_none")] value: Option<f64>,
    #[serde(skip_serializing_if="Option::is_none")] threshold: Option<f64>,
    basis: &'static str,                // basis_of(pillar) — toujours rempli, y compris Indeterminate
}
impl GreenestWindowResponse {
    pub(crate) fn new(region: &str, methodology: &str, model: &str, window: &GreenWindow,
                      eligibility: Option<EligibilityBody>) -> Result<Self, time::error::Format>;
}
impl EligibilityBody {
    pub(crate) fn from_verdicts(framework: EligibilityFramework, ruleset: &EligibilityRuleset,
                                window: &GreenWindow, verdicts: &[EligibilityVerdict]) -> Self {
        // best_eligible = best_by_score(verdicts éligibles) — CORRECTIF hexa : empruntée, pas de Vec consommé.
        // window_eligible : le verdict dont timestamp ∈ [window.start, window.end) ; si AUCUN verdict ne
        // tombe dans la fenêtre OU s'il est indéterminé => window_eligible=false (règle explicite, MANQUE hexa).
        // basis : EligibilitySignalBody.basis = basis_of(signal.pillar()) pour TOUTES les variantes.
    }
}
```
> **`GreenestWindowResponse::new` change de signature** (ajout `eligibility`) — vérifié : actuel `new(region, methodology, model: &str, window) -> Result<_, time::error::Format>` (dto.rs:393-409). Mettre à jour l'unique appelant (handler greenest_window). `disclaimer` (constante neutre, inclut : maille bidding zone nationale FR ; seuil low-carbon = proxy non réglementaire ; **branche EUA du surplus non évaluée (D8)** ; **additionnalité RFNBO hors périmètre (correctif reg-neutral)** ; reconnaissance nucléaire en cours ; un appel = un cadre, neutralité par symétrie d'accès via le catalogue).

---

## 5. Catalogue rulesets + endpoint (D5)

`handlers.rs` :
```rust
#[utoipa::path(get, path = "/v1/eligibility/rulesets",
  responses((status=200, description="Rulesets d'éligibilité disponibles", body=RulesetsResponse)),
  tag="éligibilité")]
pub(crate) async fn eligibility_rulesets() -> Json<RulesetsResponse> { Json(RulesetsResponse::catalog()) }
```
`lib.rs` (sous-routeur `core`, sans state, après `/v1/methodologies`) : `.route("/v1/eligibility/rulesets", get(handlers::eligibility_rulesets))`.

`dto.rs` (calqué sur `MethodologiesResponse::catalog()` ; **source unique = `ruleset_catalog()`**) :
```rust
#[derive(Serialize, ToSchema)]
pub(crate) struct RulesetInfo {
    framework: &'static str, version: &'static str, status: &'static str, adr: &'static str,
    /// rfnbo uniquement ; pour low-carbon => "n/a (pilier rfnbo)" (correctif hexa-compil mineur).
    granularity: &'static str, hourly_switchover: Option<&'static str>,
    article4_renewable_threshold: f64,
    surplus_price_eur_mwh: Option<f64>,
    low_carbon_intensity_threshold_g_per_kwh: Option<f64>,
    low_carbon_intensity_is_indicative: bool,
    electrolyzer_kwh_per_kg: f64,
    legal_basis: &'static str, description: &'static str,  // sources + caveat nucléaire + branche EUA + additionnalité hors périmètre
}
#[derive(Serialize, ToSchema)]
pub(crate) struct RulesetsResponse { rulesets: Vec<RulesetInfo>, disclaimer: &'static str }
impl RulesetsResponse { pub(crate) fn catalog() -> Self; } // 3 entrées: 2 served + 1 planned
```

---

## 6. Pièges 1 & 2

- **PIÈGE 1** : `bidding_zone` toujours `FR_BIDDING_ZONE = "FR"`, jamais `region.slug()` ; `national_renewable_share` lu du mix **national**. Pilier géographique RFNBO trivialement satisfait au national FR → **non matérialisé** en signal. Doc dans crate + DTO + disclaimer + ADR + test `bidding_zone_is_fr_not_region`.
- **PIÈGE 2** : `spot_price_at` filtre les prix périmés (> 1h) → `None` au-delà du day-ahead → `Indeterminate{SurplusPrice}` → `verdict:"indeterminate"`. **Aucune extrapolation.** `eligible=false` si pilier requis indéterminé ; `score` reste calculable. Tests `surplus_price_indeterminate_beyond_day_ahead` + `spot_price_stale_beyond_one_hour_is_indeterminate`.

---

## 7. Part renouvelable sur l'horizon (D4)

Nowcast/historique only via `Measurement.mix` (ancre **rte-direct**, D15) pour `at ≤ now_at`, `None` au-delà → `Indeterminate`. `low-carbon` servi sur tout l'horizon (intensité dans `ForecastPoint`) ; `rfnbo` surplus-prix sur ~24-36h. Évolution `MixForecast` (exposer `Vec<(ForecastPoint, GenerationMix)>` ou nouveau port) documentée en réserve ADR-0026 — domaine déjà prêt (`Option<f64>`).

---

## 8. Composition root — `bin/server/src/main.rs`

Une ligne, après la construction de `forecast_state` :
```rust
let forecast_state = ForecastState::new(forecaster, model)
    .with_consumption(std::sync::Arc::new(acv_forecaster), acv_model)
    .with_eligibility(std::sync::Arc::new(repo.clone())); // overlay électrolyseur (ADR-0026)
```
> `repo` (`PgIntensityRepository`) satisfait `IntensityRepository + SpotPriceRepository` → blanket `EligibilityRepo`. `bin/server/Cargo.toml` **inchangé** (l'adapter-http encapsule `carbonfr-eligibility`).

---

## 9. SDK TypeScript

`types.ts` : `export type EligibilityFramework = "rfnbo" | "low-carbon";` + `EligibilitySignal` (`pillar`, `verdict: "pass"|"fail"|"indeterminate"`, `value?`, `threshold?`, `basis`), `EligibilitySlot` (avec `intensity`, `intensity_lower`, `intensity_upper`), `EligibleSlot`, `EligibilityBody`, `RulesetInfo`, `RulesetsResponse` (snake_case, miroir §4.3/§5) ; étendre `GreenestWindowResponse` avec `eligibility?: EligibilityBody;`.
`client.ts` : étendre `greenestWindow(opts)` avec `eligibility?`, `eligibilityVersion?`, `surplusPriceEurMwh?`, `lowCarbonThresholdGPerKwh?` (camelCase→snake_case) ; nouvelle méthode `eligibilityRulesets()` → `this.get<RulesetsResponse>("/v1/eligibility/rulesets", {})`.
`index.ts` : `export *` couvre déjà ; vérifier. CI typecheck/build verte, zéro dépendance runtime.

---

## 10. OpenAPI + Bruno

**OpenAPI** (`carbonfr_openapi.rs`) : `paths(...)` += `crate::handlers::eligibility_rulesets` ; `components(schemas(...))` += `EligibilityBody`, `EligibilitySlotBody`, `EligibilitySignalBody`, `EligibleSlotBody`, `RulesetsResponse`, `RulesetInfo` ; `tags(...)` += `(name="éligibilité", description="Éligibilité électrolyseur RFNBO / bas-carbone (ADR-0025/0026, neutre)")` ; tests internes `document_lists_all_paths` += `"/v1/eligibility/rulesets"`, `document_lists_schemas` += `"EligibilityBody"`, `"RulesetsResponse"`. **Régénérer** : `UPDATE_OPENAPI_SNAPSHOT=1 cargo test -p carbonfr-adapter-http openapi_contract_snapshot`, puis **relire le diff** de `tests/openapi.snapshot.json` (purement additif : 4 query params optionnels + champ `eligibility?` + nouveau path/schemas).

**Bruno** (nouveaux fichiers — convention **post-ADR-0021** : asserter `res.body.code`, jamais `res.body.error`) :
- `greenest-window-eligibility-rfnbo.bru` (`?eligibility=rfnbo&horizon_hours=24` ; asserts `framework eq rfnbo`, `bidding_zone eq FR`, `ruleset_version eq rfnbo:2023-1184`, `disclaimer isString`, présence signal `surplus-price`).
- `greenest-window-eligibility-low-carbon.bru` (`?eligibility=low-carbon` ; asserts `framework eq low-carbon`, signal `low-carbon-intensity` avec `basis eq indicative-non-regulatory`, présence `intensity_upper`).
- `eligibility-rulesets.bru` (asserts `rulesets isArray`, entrée `served` + entrée `planned`, `low_carbon_intensity_is_indicative eq true`).
- `error-eligibility-framework-invalid.bru` (`?eligibility=foobar` → `status eq 400`, **`res.body.code eq bad_request`**).
> **Dette signalée (non corrigée dans ce lot, hors périmètre)** : `bruno/error-region-invalid.bru:19` et `bruno/error-region-no-rte-direct.bru:24` assertent encore `res.body.error` (pré-ADR-0021). À aligner dans un nettoyage séparé.

---

## 11. Tests golden (purs sauf indiqué)

**`crates/eligibility` (`#[cfg(test)]` par module, sans IO) :**
- `share.rs` : 1 `renewable_share_national_matches_sum_of_renewable_mix_shares` (≈0,2547) · 2 `_excludes_pompage_and_echanges` · 3 `_clamps_negative_production_to_zero` · 4 `_regional_uses_thermique_aggregate` · 5 `_none_on_zero_production` · 6 `_one_when_only_renewables`.
- `evaluate.rs` : 7 `low_carbon_eligible_below_threshold` · 8 `low_carbon_ineligible_above_threshold` · 9 `low_carbon_servable_without_mix_or_price` · 10 `low_carbon_signal_marks_indicative` · 11 `rfnbo_article4_passes_above_threshold` · 12 `rfnbo_article4_fails_below_threshold` · 13 `rfnbo_indeterminate_when_mix_absent` · 14 `rfnbo_surplus_price_passes_below_threshold` · 15 `rfnbo_surplus_price_indeterminate_beyond_day_ahead` (PIÈGE 2) · 16 `eligible_requires_all_pillars_pass_no_indeterminate` · 17 `low_carbon_has_no_price_pillar` (D10) · **35 `low_carbon_indeterminate_when_threshold_within_confidence_interval`** (D17) · **36 `low_carbon_prudent_estimator_flips_eligibility`** (D17, `intensity=upper`) · **37 `basis_filled_for_indeterminate_signal`** (correctif hexa).
- `ruleset.rs` : 18 `granularity_switches_to_hourly_after_switchover` · 19 `ruleset_version_is_carried_into_verdict` · 20 `thresholds_are_not_hardcoded` · 21 `with_overrides_changes_thresholds_keeps_version_sets_overridden` · 22 `catalog_serves_only_in_force_law` (`2026-revision`=planned) · 23 `resolve_ruleset_rejects_planned_and_unknown` · **38 `with_overrides_parameter_order_is_surplus_then_intensity_then_kwh`** (verrouille l'ordre, corrige l'incohérence relevée).
- score/neutralité : 24 `low_carbon_score_equals_intensity` · 25 `rank_by_score_orders_lowest_first` (NaN en queue) · 26 `rfnbo_score_penalises_missing_data` · 27 `bidding_zone_is_fr_not_region` (PIÈGE 1) · 28 `framework_slugs_are_neutral_labels` · **39 `best_by_score_borrows_without_consuming`**.

**`crates/adapter-http` (fake `EligibilityRepo` + projection DTO) :**
29 `nowcast_fills_renewable_share_future_leaves_none` (D4) · 30 `spot_price_stale_beyond_one_hour_is_indeterminate` (PIÈGE 2) · 31 `evaluate_eligibility_handles_missing_repo_data` (tout `None` → Indeterminate, jamais d'erreur) · 32 `greenest_window_without_eligibility_is_backward_compatible` (champ omis) · 33 `greenest_window_invalid_eligibility_is_400` · **40 `single_forecast_call_shared_between_window_and_eligibility`** (D16 — fake forecaster comptant ses appels = 1) · **41 `eligibility_uses_rte_direct_anchor_for_mix`** (D15 — fake repo enregistrant le `methodology_id` lu = `rte-direct`) · **42 `prudent_estimator_propagates_to_slot_intensity`** (D17).
**OpenAPI** : 34 `openapi_contract_snapshot` (régénéré) + `document_lists_all_paths`/`document_lists_schemas` mis à jour.

---

## 12. ADR-0026 — `docs/adr/0026-methodologie-overlays-eligibilite.md`

En-tête calqué sur ADR-0025 :
```
# ADR-0026 — Méthodologie des overlays d'éligibilité électrolyseur (RFNBO / bas-carbone)
- **Statut** : Accepté
- **Date** : 2026-06-20
- **Décideurs** : Morgan (Kovelt / carbon-fr)
- **ADR liés** : ADR-0025 (parent), ADR-0014, ADR-0023, ADR-0011, ADR-0006, ADR-0021
```
**## Contexte** : rappel ADR-0025 (couche A, neutralité, hors périmètre gCO₂eq/kgH₂) + synthèse réglementaire (RFNBO 2023/1184 piliers + 2023/1185 GHG ; low-carbon 2025/2359 du 8/7/2025) + GAP technique (`ForecastPoint` sans mix) + pièges 1 & 2 + table **FAIT vs ESTIMATION**.
**## Décision** (numérotée) : (1) crate domaine pur `carbonfr-eligibility` au-dessus de `core`, `evaluate` pur total (pas de `thiserror`), `SlotInput`, `core` intact. (2) Deux cadres neutres séparés (rfnbo n'utilise jamais le seuil intensité ; low-carbon n'utilise ni Article4/coverage ni prix — D10). (3) Seuils servis [FAIT] versionnés : bascule horaire 2030-01-01, Article 4 0,90, surplus prix 20 €/MWh ; **branche EUA documentée mais non câblée** (D8). (4) Seuil low-carbon = proxy carbon-fr 60 g/kWh [ESTIMATION], **dérivation reproductible** (28,2 gCO₂eq/MJ × 120 MJ/kg = 3384 g/kg ; ÷ 53 kWh/kg = 63,8 g/kWh borne haute ; **60 = défaut prudent CHOISI sous la borne**, étiqueté `indicative`), conso 53 kWh/kg versionnée + overridable (D7). (5) Caveat nucléaire (consultation **30/06/2026**, éval. **07/2028**) neutre. (6) PIÈGE 1 (bidding zone FR, pilier géo non matérialisé). (7) PIÈGE 2 (prix Indeterminate au-delà du day-ahead, filtre ≤1h). (8) Report = `rfnbo:2026-revision` `planned`, non servi (D6) ; **aucune date de report figée** comme un fait (ADR-0025 cite « 2031-2033 » comme propositions). (9) Part renouvelable future = nowcast-only (D4), ancre **rte-direct** (D15), `MixForecast` en réserve. (10) **Intervalles ADR-0011 exploités** : estimateur propagé, low-carbon indéterminé quand le seuil ∈ [lower,upper] (D17). (11) **Mono-forecast** : un seul `forecast()` partagé fenêtre/éligibilité (D16). (12) API = extension `?eligibility=` + catalogue `/v1/eligibility/rulesets` ; pas de `/electrolyzer/*`. (13) Hors périmètre confirmé : gCO₂eq/kgH₂, certification, **additionnalité PPA** (donnée niveau site absente — exposée aussi dans le disclaimer).
**## Conséquences** : positives (neutre/vérifiable/versionné ; domaine testable ; `core` intact ; contrat additif ; FR low-carbon discriminant ; mono-forecast ; intervalles respectés) ; limites (Article 4 ≈ jamais en FR — assumé ; rfnbo futur partiellement Indeterminate ; seuil low-carbon = proxy ; EUA + MixForecast en réserve ; un appel = un cadre, neutralité par symétrie d'accès ; revue de neutralité adversariale type ADR-0024 recommandée avant tout palier payant).
**## Alternatives considérées** : endpoint dédié `/electrolyzer/*` (rejeté — Option A) ; seuil low-carbon réglementaire (rejeté — `indicative`) ; servir `rfnbo:2026-revision` (rejeté) ; ancre mix `acv-ademe` (rejeté — divergence convention `rte-direct`, D15) ; double-forecast (rejeté — D16) ; `ForecastState`→`AppState<R>`/`<F,R>` (rejeté — contamine 5 handlers) ; use-case dans `core` (rejeté — cycle) ; MixForecast immédiat (reporté) ; pilier prix low-carbon (rejeté — D10) ; EUA câblé (reporté) ; ignorer les intervalles ADR-0011 (rejeté — D17).
Maj : `CLAUDE.md` racine + `docs/adr/README.md` (ferme) + ADR-0025.

---

## 13. Découpage en commits atomiques (`feat/electrolyzer-eligibility-layer`)

Chaque commit : `cargo fmt --all` + `cargo clippy --all-targets -- -D warnings` + `cargo test --workspace` + `cargo deny check` verts. Messages en français, trailer `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

1. **`feat(eligibility): crate domaine pur carbonfr-eligibility`** — `crates/eligibility/` (Cargo + lib/ruleset/verdict/share/evaluate) + workspace `Cargo.toml` + **golden tests purs (1-28, 35-39)**. Aucun autre crate touché.
2. **`feat(http): trait EligibilityRepo + état + use-case d'assemblage`** — `adapter-http/Cargo.toml` (**`async-trait` en `[dependencies]`** + `carbonfr-eligibility`), `lib.rs` (trait + blanket impl + champ + builder + `mod eligibility_uc;`), `eligibility_uc.rs` + tests use-case (29-31, 40-42). Pas encore en route.
3. **`feat(http): greenest-window mono-forecast + ?eligibility + DTO`** — `handlers.rs` (query + branche refactorée + `validate_eligibility_overrides`) + `dto.rs` (DTO + disclaimer + `GreenestWindowResponse::new` resignée) + tests handler (32-33).
4. **`feat(http): catalogue GET /v1/eligibility/rulesets`** — `dto.rs` (`RulesetsResponse::catalog`) + `handlers.rs` (handler) + `lib.rs` (route).
5. **`docs(openapi): paths/schemas éligibilité + snapshot régénéré`** — `carbonfr_openapi.rs` + `tests/openapi.snapshot.json` (relu en PR).
6. **`feat(server): câble l'overlay d'éligibilité (composition root)`** — `bin/server/src/main.rs`.
7. **`feat(sdk): éligibilité dans greenestWindow + eligibilityRulesets`** — `sdk/typescript/src/{types.ts,client.ts}` (+ `index.ts`).
8. **`test(bruno): collections d'exemple éligibilité`** — 4 fichiers `bruno/*.bru` (convention `res.body.code`).
9. **`docs(adr-0026): méthodologie des overlays d'éligibilité électrolyseur`** — `docs/adr/0026-*.md` + `CLAUDE.md` + `docs/adr/README.md` (ferme : ligne 0026 + bascule statut 0025) + ADR-0025.

Chaîne de compilation : 1→2→3→4→5→6 ; 7/8/9 indépendants. Validation finale : `cargo check --workspace`, `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all`, `cargo deny check`, + typecheck/build SDK. **Ne committer/pusher que sur feu vert explicite.**

---

## D. Valeurs par défaut du ruleset v1 (chiffres réglementaires sourcés)

**`rfnbo:2023-1184` — `status=Served`, `framework=Rfnbo`** (Règl. délégués UE 2023/1184 & 2023/1185) :
- `granularity = Monthly` ; `hourly_switchover = 2030-01-01` — **[FAIT]** corrélation mensuelle jusqu'au 31/12/2029, horaire dès le 01/01/2030 (option d'anticipation MS dès 01/07/2027 ; FR n'a pas anticipé). `legal_basis` = « Règl. délégué (UE) 2023/1184, corrélation temporelle [FAIT] ».
- `article4_renewable_threshold = 0.90` — **[FAIT]** exception « réseau ≥ 90 % renouvelable » (année civile précédente, bidding zone, vaut 5 ans). ≈ jamais atteint en FR. `legal_basis` = « 2023/1184 art. 4 [FAIT] ».
- `surplus_price_eur_mwh = Some(20.0)` — **[FAIT]** corrélation temporelle réputée satisfaite si prix day-ahead ≤ 20 €/MWh **OU < 0,36 × prix EUA** (branche EUA **non câblée**, D8, documentée dans `description`). Horizon ≤ ~24-36h → au-delà Indeterminate (PIÈGE 2).
- `low_carbon_intensity_threshold_g_per_kwh = None` ; `low_carbon_intensity_is_indicative = false` ; `electrolyzer_kwh_per_kg = 53.0`.
- `description` mentionne : additionnalité (règle des 36 mois, grandfathering < 01/01/2028) **HORS périmètre** (donnée niveau site absente).

**`low-carbon:2025-2359` — `status=Served`, `framework=LowCarbon`** (Règl. délégué UE 2025/2359, adopté 8/7/2025, publié 21/11/2025, en vigueur ~11/12/2025) :
- `granularity = Hourly` ; `hourly_switchover = 2025-12-11` — **catalogue : marqués « n/a (pilier rfnbo) »** (correctif hexa-compil ; ces champs ne s'appliquent pas à low-carbon).
- `low_carbon_intensity_threshold_g_per_kwh = Some(60.0)` — **[ESTIMATION — proxy carbon-fr, NON réglementaire]**. Dérivation reproductible : seuil produit légal **28,2 gCO₂eq/MJ** (= 94 × 0,30) × 120 MJ/kg = **3384 gCO₂eq/kgH₂** ; ÷ **53 kWh/kgH₂** = **63,8 g/kWh** (borne haute, tout le budget GHG attribué à l'élec) ; **60 = défaut prudent CHOISI sous 63,8** (marge conversion/compression). `legal_basis`/`description` documentent explicitement que 60 n'est PAS un calcul direct mais un choix prudent sous la borne 63,8.
- `low_carbon_intensity_is_indicative = true` (**étiquetage obligatoire**) ; `electrolyzer_kwh_per_kg = 53.0` (médiane fourchette industrielle 50-55, overridable).
- `article4_renewable_threshold = 0.0` (non pertinent) ; `surplus_price_eur_mwh = None` (**D10** : pas de pilier prix).
- Caveat nucléaire **[FAIT]** : reconnaissance de l'électricité bas-carbone d'origine nucléaire **en cours** (consultation Commission d'ici **30/06/2026**, évaluation d'ici **07/2028**) → formulé neutre dans `description`/`disclaimer`.

**`rfnbo:2026-revision` — `status=Planned` (catalogue uniquement, jamais résolu, D6)** : `hourly_switchover` garde **2030-01-01** (droit en vigueur) ; `description` = « Report attendu de la bascule horaire (propositions **2031-2033** selon les sources, échéance NON figée) + additionnalité repoussée — **droit NON en vigueur, NON adopté** » [propositions]. **CORRECTIF reg-neutral** : aucune date de report n'est codée comme un fait ; on s'aligne sur la fourchette d'ADR-0025 (« 2031-2033 ») uniquement en texte.

Comparateur fossile commun **[FAIT]** : 94 gCO₂eq/MJ, réduction ≥ 70 % (RFNBO via 2023/1185 et low-carbon).

---

## E. Ordre des commits

1. `feat(eligibility): crate domaine pur carbonfr-eligibility` (domaine + tests 1-28, 35-39)
2. `feat(http): trait EligibilityRepo + état + use-case d'assemblage` (+ async-trait en deps ; tests 29-31, 40-42)
3. `feat(http): greenest-window mono-forecast + ?eligibility + DTO` (tests 32-33)
4. `feat(http): catalogue GET /v1/eligibility/rulesets`
5. `docs(openapi): paths/schemas éligibilité + snapshot régénéré` (test 34)
6. `feat(server): câble l'overlay d'éligibilité (composition root)`
7. `feat(sdk): éligibilité dans greenestWindow + eligibilityRulesets`
8. `test(bruno): collections d'exemple éligibilité`
9. `docs(adr-0026): méthodologie des overlays d'éligibilité électrolyseur`

Dépendances : 1→2→3→4→5→6 ; 7/8/9 indépendants. **Pas de commit/push sans feu vert.**

---

## F. Risques résiduels

1. **Cohérence du blanket impl** (`EligibilityRepo for R: IntensityRepository + SpotPriceRepository`) : à valider au `cargo check` du commit 2 qu'aucun conflit de trait coherence n'apparaît avec d'éventuels impls existants. Repli : impl explicite pour `PgIntensityRepository` — mais l'orphan rule l'interdit hors adapter-postgres ; le blanket dans adapter-http reste la voie propre. **Faible.**
2. **`ApiError::unavailable` (503)** : vérifier qu'une variante 503 existe dans `error.rs` (sinon l'ajouter avec `code` stable RFC 9457, ADR-0021). N'arrive qu'en mauvaise config (jamais en prod, câblé au composition root). **Faible.**
3. **Appariement prix ↔ créneau** : le day-ahead est au pas 15 min (MTU 15 min depuis 2026-06-20, get_price.rs) alors que `ForecastPoint` peut être horaire ; le filtre `≤ 1h` de `spot_price_at` couvre l'écart, mais valider que `price_at(at)` (au plus proche ≤ at) renvoie bien le créneau pertinent par point. **Faible.**
4. **`renewable_share` synchronisée avec `mix_shares`** : la convention est dupliquée (privée non réexportée). Le test golden #1 (≈0,2547) détecte une dérive, mais un changement de `mix_shares` côté core ne casse pas mécaniquement ce crate. **Moyen** — mitigé par le test golden ; à re-vérifier si `price.rs` évolue.
5. **Snapshot OpenAPI** : le diff doit être purement additif ; toute modification d'un champ existant en PR = régression de contrat à investiguer. **Faible** (gardé par le snapshot + relecture).
6. **Neutralité « un appel = un cadre »** : la réponse n'expose qu'un cadre à la fois ; la symétrie repose sur le catalogue `/rulesets` + deux appels. Limite assumée et documentée (disclaimer + ADR). Revue adversariale type ADR-0024 recommandée avant tout palier payant. **Moyen (gouvernance, pas technique).**
7. **`MixForecast` absent** : `rfnbo` Article4/coverage reste Indeterminate sur l'horizon futur ; en FR (Article 4 ≈ jamais déclenché) la perte pratique est faible, mais le verdict rfnbo futur est honnêtement incomplet. Réserve documentée. **Faible.**
8. **Dette Bruno pré-ADR-0021** (`res.body.error` dans `error-region-*.bru`) : non corrigée dans ce lot (hors périmètre), signalée pour un nettoyage séparé. **Cosmétique.**

---

## G. Critiques infondées / écartées (notées pour traçabilité)

- **hexa-compil [mineur] style `Arc<dyn EligibilityRepo>` sans `+ Send + Sync`** : techniquement non bloquant (supertraits `Send + Sync` sur `EligibilityRepo`), mais **intégré quand même** par parité de style avec `consumption` (D3).
- **reg-neutral : symétrie de visibilité des deux cadres** : la suggestion d'une réponse jointe est écartée (un appel = un cadre, plus petit diff) ; la neutralité est garantie par la **symétrie d'accès** (catalogue + paramètre), documentée comme limite assumée — pas un défaut.
- **constat DOMAINE proposait `thiserror` + `framework` séparé dans `evaluate`** : écartés (évaluation totale sans `Result` ; framework porté par le ruleset, D13) — divergences assumées et signalées.