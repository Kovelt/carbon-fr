//! Verdict d'éligibilité par créneau : signaux, statut, et **entrée** par créneau.

use time::OffsetDateTime;

use carbonfr_core::domain::CarbonIntensity;

use crate::ruleset::EligibilityFramework;

/// Zone de dépôt (« bidding zone ») — **PIÈGE 1** : la corrélation géographique
/// RFNBO s'évalue à la zone nationale (FR = une seule zone), **jamais** à l'une
/// des 12 sous-régions carbone. Toujours `"FR"`.
pub const FR_BIDDING_ZONE: &str = "FR";

/// Entrée d'évaluation **par créneau** : tout ce dont la stratégie pure a besoin.
///
/// `ForecastPoint` ne porte que l'intensité (pas le mix ni le prix). On enrichit
/// donc chaque créneau ici. `renewable_share`/`spot_price_eur_mwh` sont `None`
/// quand l'info n'est pas disponible (au-delà du nowcast pour le mix, au-delà du
/// day-ahead pour le prix) → signal `Indeterminate`, jamais d'extrapolation.
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
    /// Part renouvelable **instantanée** du mix national (nowcast), `[0,1]`.
    pub renewable_share: Option<f64>,
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

/// Base réglementaire d'un pilier (pour l'auditabilité, y compris `Indeterminate`).
/// Le seuil bas-carbone est un proxy → `indicative-non-regulatory` ; les autres
/// dérivent directement des textes UE → `regulatory`.
pub fn basis_of(pillar: Pillar) -> &'static str {
    match pillar {
        Pillar::LowCarbonIntensity => "indicative-non-regulatory",
        Pillar::RenewableShare | Pillar::SurplusPrice => "regulatory",
    }
}

/// Signal émis par un pilier sur un créneau.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EligibilitySignal {
    /// Part renouvelable instantanée vs seuil (proxy ; ≠ Article 4 annuel).
    RenewableShare {
        share: f64,
        threshold: f64,
        passed: bool,
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
    /// Donnée manquante : le pilier ne peut pas trancher (jamais extrapolé).
    Indeterminate { pillar: Pillar },
}

impl EligibilitySignal {
    pub fn pillar(self) -> Pillar {
        match self {
            EligibilitySignal::RenewableShare { .. } => Pillar::RenewableShare,
            EligibilitySignal::SurplusPrice { .. } => Pillar::SurplusPrice,
            EligibilitySignal::LowCarbonIntensity { .. } => Pillar::LowCarbonIntensity,
            EligibilitySignal::Indeterminate { pillar } => pillar,
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
