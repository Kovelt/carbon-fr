//! Verdict d'éligibilité par créneau : signaux, statut, et **entrée** par créneau.

use time::OffsetDateTime;

use carbonfr_core::domain::CarbonIntensity;

use crate::ruleset::EligibilityFramework;
use crate::share_forecast::ShareEstimate;

/// Zone de dépôt (« bidding zone ») — **PIÈGE 1** : la corrélation géographique
/// RFNBO s'évalue à la zone nationale (FR = une seule zone), **jamais** à l'une
/// des 12 sous-régions carbone. Toujours `"FR"`.
pub const FR_BIDDING_ZONE: &str = "FR";

/// Entrée d'évaluation **par créneau** : tout ce dont la stratégie pure a besoin.
///
/// `ForecastPoint` ne porte que l'intensité (pas le mix ni le prix). On enrichit
/// donc chaque créneau ici. `renewable_share`/`spot_price_eur_mwh` sont `None`
/// quand l'info n'est pas disponible (au-delà de l'horizon **calibré** de
/// `share-clim@1` pour le mix, au-delà du day-ahead pour le prix) → signal
/// `Indeterminate`, jamais d'extrapolation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotInput {
    pub at: OffsetDateTime,
    /// Intensité retenue pour le report et le classement (= `expected` en
    /// estimateur central, `upper` en prudent).
    pub intensity: CarbonIntensity,
    /// Bornes de l'intervalle de confiance (ADR-0011), portées pour la règle
    /// d'indétermination « seuil ∈ [lower, upper] ».
    pub intensity_lower: CarbonIntensity,
    pub intensity_upper: CarbonIntensity,
    /// Part renouvelable du mix national, `[0,1]` — **observée** (nowcast,
    /// intervalle dégénéré) ou **prévue** (`share-clim@1`, intervalle calibré —
    /// ADR-0028). La provenance et l'intervalle sont portés par l'estimation.
    pub renewable_share: Option<ShareEstimate>,
    /// Pourquoi `renewable_share` est `None`, quand il l'est (F12 — la sortie
    /// distingue « au-delà de l'horizon calibré » de « donnée manquante »).
    /// Ignoré si `renewable_share` est `Some`. Seules `MissingData` et
    /// `BeyondCalibratedHorizon` ont un sens ici.
    pub renewable_share_gap: IndeterminateReason,
    /// Prix spot day-ahead (€/MWh) au créneau, frais (filtré côté adapter).
    pub spot_price_eur_mwh: Option<f64>,
}

/// Pilier d'éligibilité (de quel test provient un signal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pillar {
    /// Part renouvelable instantanée (proxy réseau, cadre `rfnbo`).
    RenewableShare,
    /// Surplus prix day-ahead (cadre `rfnbo`).
    SurplusPrice,
    /// Seuil d'intensité bas-carbone (cadre `low-carbon`).
    LowCarbonIntensity,
}

impl Pillar {
    pub fn slug(self) -> &'static str {
        match self {
            Pillar::RenewableShare => "renewable-share",
            Pillar::SurplusPrice => "surplus-price",
            Pillar::LowCarbonIntensity => "low-carbon-intensity",
        }
    }
}

/// Base réglementaire **canonique** d'un pilier (pour l'auditabilité, y compris
/// `Indeterminate`). Le seuil bas-carbone est un proxy → `indicative-non-regulatory` ;
/// les autres dérivent directement des textes UE → `regulatory`.
///
/// ⚠️ C'est la base du *ruleset canonique* : quand l'appelant a surchargé le seuil
/// d'un pilier, la base **servie** doit venir de `EligibilityRuleset::basis_for`
/// (qui retourne alors `user-override`) — revue de neutralité ADR-0026, constat C14.
pub fn basis_of(pillar: Pillar) -> &'static str {
    match pillar {
        Pillar::LowCarbonIntensity => "indicative-non-regulatory",
        Pillar::RenewableShare | Pillar::SurplusPrice => "regulatory",
    }
}

/// Signal émis par un pilier sur un créneau.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EligibilitySignal {
    /// Part renouvelable vs seuil (proxy ; ≠ Article 4 annuel). Observée
    /// (nowcast) ou prévue (`share-clim@1`) — dans le second cas le verdict est
    /// tranché sur l'**intervalle** `[lower, upper]` (ferme seulement hors
    /// recouvrement du seuil, ADR-0028).
    RenewableShare {
        share: f64,
        lower: f64,
        upper: f64,
        threshold: f64,
        passed: bool,
        /// `true` = mix observé ; `false` = part prévue (`share-clim@1`).
        observed: bool,
    },
    /// Prix day-ahead ≤ seuil de surplus.
    SurplusPrice {
        spot_price_eur_mwh: f64,
        threshold: f64,
        passed: bool,
    },
    /// Intensité carbone vs seuil bas-carbone (condition nécessaire).
    LowCarbonIntensity {
        intensity_g_per_kwh: f64,
        threshold: f64,
        indicative: bool,
        passed: bool,
    },
    /// Le pilier ne peut pas trancher — la raison est portée explicitement
    /// (revue de neutralité ADR-0028, F12 : des causes opérationnellement très
    /// différentes ne doivent pas être indiscernables). Jamais extrapolé.
    Indeterminate {
        pillar: Pillar,
        reason: IndeterminateReason,
    },
}

/// Pourquoi un pilier n'a pas pu trancher (servi avec le signal — additif).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndeterminateReason {
    /// Donnée absente (mix/prix/seuil indisponible au créneau).
    MissingData,
    /// Créneau au-delà de l'horizon **calibré** de la prévision de part
    /// renouvelable (`share-clim@1`, ADR-0028) — on ne prévoit pas au-delà.
    BeyondCalibratedHorizon,
    /// Le seuil tombe dans l'intervalle de confiance `[lower, upper]` (D17) :
    /// trancher fermement serait une sur-affirmation.
    ThresholdWithinInterval,
    /// Prix connu mais au-dessus du seuil : l'exception surplus n'est pas
    /// établie SANS être réfutée (la branche EUA n'est pas évaluée) — jamais
    /// de `fail` ferme sur ce pilier.
    SurplusNotEstablished,
}

impl IndeterminateReason {
    pub fn slug(self) -> &'static str {
        match self {
            IndeterminateReason::MissingData => "missing-data",
            IndeterminateReason::BeyondCalibratedHorizon => "beyond-calibrated-horizon",
            IndeterminateReason::ThresholdWithinInterval => "threshold-within-interval",
            IndeterminateReason::SurplusNotEstablished => "surplus-not-established",
        }
    }
}

impl EligibilitySignal {
    pub fn pillar(self) -> Pillar {
        match self {
            EligibilitySignal::RenewableShare { .. } => Pillar::RenewableShare,
            EligibilitySignal::SurplusPrice { .. } => Pillar::SurplusPrice,
            EligibilitySignal::LowCarbonIntensity { .. } => Pillar::LowCarbonIntensity,
            EligibilitySignal::Indeterminate { pillar, .. } => pillar,
        }
    }

    /// `Some(passed)` ou `None` si le pilier n'a pas pu statuer.
    pub fn passed(self) -> Option<bool> {
        match self {
            EligibilitySignal::RenewableShare { passed, .. }
            | EligibilitySignal::SurplusPrice { passed, .. }
            | EligibilitySignal::LowCarbonIntensity { passed, .. } => Some(passed),
            EligibilitySignal::Indeterminate { .. } => None,
        }
    }

    /// Valeur observée du signal (part, prix, ou intensité), `None` si indéterminé.
    pub fn value(self) -> Option<f64> {
        match self {
            EligibilitySignal::RenewableShare { share, .. } => Some(share),
            EligibilitySignal::SurplusPrice {
                spot_price_eur_mwh, ..
            } => Some(spot_price_eur_mwh),
            EligibilitySignal::LowCarbonIntensity {
                intensity_g_per_kwh,
                ..
            } => Some(intensity_g_per_kwh),
            EligibilitySignal::Indeterminate { .. } => None,
        }
    }

    /// Provenance de la valeur du signal, pour l'auditabilité : `observed` /
    /// `forecast` sur le pilier part renouvelable (le seul dont la provenance
    /// varie créneau par créneau, ADR-0028). `None` ailleurs — **pas** parce que
    /// les autres piliers seraient de l'observé (l'intensité de l'overlay
    /// prévisionnel est TOUJOURS une prévision, le prix day-ahead est une donnée
    /// publiée) : leur provenance est constante et connue de l'adapter, qui la
    /// sert lui-même (revue de neutralité ADR-0028, F1 — parité de divulgation).
    pub fn provenance(self) -> Option<&'static str> {
        match self {
            EligibilitySignal::RenewableShare { observed, .. } => {
                Some(if observed { "observed" } else { "forecast" })
            }
            _ => None,
        }
    }

    /// Raison d'indétermination, `None` si le signal a tranché.
    pub fn indeterminate_reason(self) -> Option<IndeterminateReason> {
        match self {
            EligibilitySignal::Indeterminate { reason, .. } => Some(reason),
            _ => None,
        }
    }

    /// Bornes de l'intervalle de la valeur, quand le signal est une **prévision**
    /// (part renouvelable `share-clim@1`) — `None` pour l'observé (dégénéré) et
    /// les autres piliers.
    pub fn value_bounds(self) -> Option<(f64, f64)> {
        match self {
            EligibilitySignal::RenewableShare {
                lower,
                upper,
                observed: false,
                ..
            } => Some((lower, upper)),
            _ => None,
        }
    }

    /// Seuil appliqué, `None` si indéterminé.
    pub fn threshold(self) -> Option<f64> {
        match self {
            EligibilitySignal::RenewableShare { threshold, .. }
            | EligibilitySignal::SurplusPrice { threshold, .. }
            | EligibilitySignal::LowCarbonIntensity { threshold, .. } => Some(threshold),
            EligibilitySignal::Indeterminate { .. } => None,
        }
    }
}

/// Verdict d'éligibilité d'un créneau au regard d'un cadre + ruleset.
#[derive(Debug, Clone, PartialEq)]
pub struct EligibilityVerdict {
    pub timestamp: OffsetDateTime,
    /// Toujours [`FR_BIDDING_ZONE`] (PIÈGE 1).
    pub bidding_zone: &'static str,
    pub framework: EligibilityFramework,
    pub ruleset_version: &'static str,
    /// `true` **uniquement** si l'éligibilité est certaine (au moins un pilier
    /// passe). Voir [`is_indeterminate`](EligibilityVerdict::is_indeterminate).
    pub eligible: bool,
    pub signals: Vec<EligibilitySignal>,
    pub carbon_intensity: CarbonIntensity,
    pub intensity_lower: CarbonIntensity,
    pub intensity_upper: CarbonIntensity,
    /// Score de classement (plus bas = meilleur), jamais `NaN`.
    pub score: f64,
}

impl EligibilityVerdict {
    /// `true` si non éligible **mais** un pilier reste indéterminé : le verdict
    /// pourrait basculer si la donnée manquante (ou une voie non évaluée) devenait
    /// disponible. Pour `rfnbo`, le pilier surplus n'émet jamais d'échec ferme (la
    /// branche EUA n'est pas câblée) → un créneau non éligible y est typiquement
    /// indéterminé plutôt que « définitivement non éligible » (on ne sur-affirme
    /// pas un négatif). Le cas certain-non-éligible n'existe que si tous les piliers
    /// du cadre ont émis un échec ferme.
    pub fn is_indeterminate(&self) -> bool {
        !self.eligible
            && self
                .signals
                .iter()
                .any(|s| matches!(s, EligibilitySignal::Indeterminate { .. }))
    }
}
