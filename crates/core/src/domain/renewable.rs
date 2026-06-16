//! Dérivation **renouvelable** : météo → production éolien/solaire estimée
//! (ADR-0018). Couche métier **explicable et calée sur la donnée FR** — le moat.
//!
//! Pur, sans IO : une fonction `(vent, irradiance) → (éolien MW, solaire MW)`
//! paramétrée par un modèle **versionné**, dont les capacités effectives sont
//! **calibrées par moindres carrés** sur l'historique (production réelle RTE).
//! La qualité est **mesurée par backtest** (jamais supposée), comme la prévision
//! (ADR-0009).
//!
//! - **Éolien** : courbe de puissance agrégée de la flotte, sigmoïde du vent à
//!   100 m (les seuils cut-in/cut-out individuels se lissent à l'échelle nationale).
//! - **Solaire** : linéaire en irradiance (réf. STC 1000 W/m²), nul la nuit.
//!
//! Les paramètres de courbe sont des **constantes documentées** (à raffiner par
//! backtest) ; les capacités effectives (`*_capacity_mw`) sont **calibrées** car
//! le parc installé croît dans le temps.

/// Modèle de dérivation renouvelable **versionné** (ADR-0018).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenewableModel {
    /// Capacité éolienne **effective** (MW) — facteur d'échelle calibré.
    pub wind_capacity_mw: f64,
    /// Capacité solaire **effective** (MW) — facteur d'échelle calibré.
    pub solar_capacity_mw: f64,
    /// Point médian de la sigmoïde éolienne (vent à 100 m, km/h).
    pub wind_midpoint_kmh: f64,
    /// Raideur de la sigmoïde éolienne (1/(km/h)).
    pub wind_steepness: f64,
}

/// Irradiance de référence (conditions standard de test, W/m²).
const STC_IRRADIANCE: f64 = 1000.0;

impl RenewableModel {
    /// Paramètres de courbe par défaut (v1, à raffiner par backtest) : médiane
    /// ~30 km/h (~8 m/s), raideur 0,12. Capacités à 0 → à calibrer.
    pub const fn v1_uncalibrated() -> Self {
        Self {
            wind_capacity_mw: 0.0,
            solar_capacity_mw: 0.0,
            wind_midpoint_kmh: 30.0,
            wind_steepness: 0.12,
        }
    }

    /// Facteur de charge éolien ∈ [0, 1] pour un vent (km/h) — courbe de
    /// puissance **agrégée** de la flotte (sigmoïde).
    pub fn wind_capacity_factor(&self, wind_kmh: f64) -> f64 {
        1.0 / (1.0 + (-self.wind_steepness * (wind_kmh - self.wind_midpoint_kmh)).exp())
    }

    /// Éolien estimé (MW) : capacité effective × facteur de charge.
    pub fn estimate_wind_mw(&self, wind_kmh: f64) -> f64 {
        self.wind_capacity_mw * self.wind_capacity_factor(wind_kmh)
    }

    /// Solaire estimé (MW) : linéaire en irradiance (réf. STC), borné à ≥ 0.
    pub fn estimate_solar_mw(&self, irradiance_wm2: f64) -> f64 {
        self.solar_capacity_mw * (irradiance_wm2 / STC_IRRADIANCE).max(0.0)
    }
}

/// Échantillon de calibration : météo observée + production réelle (MW).
#[derive(Debug, Clone, Copy)]
pub struct RenewableSample {
    pub wind_kmh: f64,
    pub irradiance_wm2: f64,
    pub eolien_mw: f64,
    pub solaire_mw: f64,
}

/// Cale les **capacités effectives** (MW) par moindres carrés **à l'origine**, à
/// partir d'échantillons (météo, production réelle), en gardant fixes les
/// paramètres de courbe. `None` si l'assise est dégénérée (aucun signal).
///
/// Pour l'éolien : `cap = Σ(cf·éolien) / Σ(cf²)` ; pour le solaire, idem avec
/// `x = irradiance / STC`. Régression linéaire sans constante = on ne fabrique
/// pas de production sans ressource (vent/soleil nuls → estimation nulle).
pub fn calibrate(samples: &[RenewableSample], curve: RenewableModel) -> Option<RenewableModel> {
    let (mut sw_xy, mut sw_xx, mut ss_xy, mut ss_xx) = (0.0, 0.0, 0.0, 0.0);
    for s in samples {
        let cf = curve.wind_capacity_factor(s.wind_kmh);
        sw_xy += cf * s.eolien_mw;
        sw_xx += cf * cf;
        let x = (s.irradiance_wm2 / STC_IRRADIANCE).max(0.0);
        ss_xy += x * s.solaire_mw;
        ss_xx += x * x;
    }
    if sw_xx <= f64::EPSILON || ss_xx <= f64::EPSILON {
        return None;
    }
    Some(RenewableModel {
        wind_capacity_mw: sw_xy / sw_xx,
        solar_capacity_mw: ss_xy / ss_xx,
        ..curve
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_factor_is_monotonic_and_bounded() {
        let m = RenewableModel::v1_uncalibrated();
        let (low, mid, high) = (
            m.wind_capacity_factor(5.0),
            m.wind_capacity_factor(30.0),
            m.wind_capacity_factor(80.0),
        );
        assert!((0.0..=1.0).contains(&low));
        assert!(low < mid && mid < high);
        assert!((mid - 0.5).abs() < 1e-9, "médiane = 0,5 au point milieu");
    }

    #[test]
    fn solar_is_linear_in_irradiance_and_zero_at_night() {
        let m = RenewableModel {
            solar_capacity_mw: 10_000.0,
            ..RenewableModel::v1_uncalibrated()
        };
        assert_eq!(m.estimate_solar_mw(0.0), 0.0);
        assert_eq!(m.estimate_solar_mw(1000.0), 10_000.0);
        assert!((m.estimate_solar_mw(500.0) - 5_000.0).abs() < 1e-9);
    }

    #[test]
    fn calibration_recovers_known_capacities() {
        // Données synthétiques générées par un modèle connu → la calibration
        // doit retrouver ses capacités (régression exacte sans bruit).
        let truth = RenewableModel {
            wind_capacity_mw: 24_000.0,
            solar_capacity_mw: 18_000.0,
            ..RenewableModel::v1_uncalibrated()
        };
        let samples: Vec<RenewableSample> = [(10.0, 200.0), (35.0, 600.0), (60.0, 950.0)]
            .iter()
            .map(|&(wind, irr)| RenewableSample {
                wind_kmh: wind,
                irradiance_wm2: irr,
                eolien_mw: truth.estimate_wind_mw(wind),
                solaire_mw: truth.estimate_solar_mw(irr),
            })
            .collect();
        let fit = calibrate(&samples, RenewableModel::v1_uncalibrated()).unwrap();
        assert!((fit.wind_capacity_mw - 24_000.0).abs() < 1e-6);
        assert!((fit.solar_capacity_mw - 18_000.0).abs() < 1e-6);
    }
}
