//! Bandes d'incertitude **par horizon** (ADR-0011 §5/§6).
//!
//! Calibrées à partir des **résidus empiriques** d'un backtest *walk-forward* :
//! pour chaque décalage d'horizon, on mesure la distribution de l'erreur
//! `observed − expected` et on en retient deux quantiles (ex. 10 % / 90 %).
//! Appliquées à une prévision, elles encadrent `expected` d'un intervalle qui
//! **s'élargit naturellement avec l'horizon** (l'erreur croît), sans hypothèse
//! gaussienne ni calibrage arbitraire.

use time::Duration;

/// Quantiles `(bas, haut)` de l'erreur `observed − expected`, indexés par
/// **décalage d'horizon** (multiples de `step`). `bas` est typiquement négatif
/// (l'observé tombe sous l'estimation), `haut` positif.
#[derive(Debug, Clone, PartialEq)]
pub struct HorizonBands {
    step: Duration,
    bands: Vec<(f64, f64)>,
}

impl HorizonBands {
    /// Construit les bandes depuis les résidus groupés par index d'horizon
    /// (`residuals_by_index[k]` = échantillons d'erreur à l'horizon `k·step`).
    /// Pour chaque horizon, retient les quantiles `q` et `1 − q`. Un horizon sans
    /// résidu reçoit une bande nulle `(0, 0)`.
    pub fn from_residuals(step: Duration, residuals_by_index: &[Vec<f64>], q: f64) -> Self {
        let bands = residuals_by_index
            .iter()
            .map(|errors| {
                if errors.is_empty() {
                    (0.0, 0.0)
                } else {
                    let mut sorted = errors.clone();
                    sorted.sort_by(|a, b| a.total_cmp(b));
                    (quantile(&sorted, q), quantile(&sorted, 1.0 - q))
                }
            })
            .collect();
        Self { step, bands }
    }

    pub fn step(&self) -> Duration {
        self.step
    }

    pub fn len(&self) -> usize {
        self.bands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bands.is_empty()
    }

    /// Quantiles `(bas, haut)` au décalage `dt` depuis l'origine de prévision
    /// (index = `dt / step`, borné au dernier horizon calibré). `None` si vide.
    pub fn at(&self, dt: Duration) -> Option<(f64, f64)> {
        let step_secs = self.step.whole_seconds();
        if self.bands.is_empty() || step_secs <= 0 {
            return None;
        }
        let idx = (dt.whole_seconds() / step_secs).max(0) as usize;
        Some(self.bands[idx.min(self.bands.len() - 1)])
    }
}

/// Quantile (interpolation linéaire) d'une tranche **déjà triée** et non vide.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = q.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    let frac = rank - lo as f64;
    sorted[lo] + (sorted[hi] - sorted[lo]) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_residuals_give_zero_band() {
        let b = HorizonBands::from_residuals(Duration::minutes(15), &[vec![], vec![]], 0.1);
        assert_eq!(b.at(Duration::ZERO), Some((0.0, 0.0)));
    }

    #[test]
    fn band_widens_with_horizon() {
        // Horizon 0 : erreurs serrées ; horizon 1 : erreurs larges.
        let near: Vec<f64> = (-2..=2).map(|x| x as f64).collect(); // -2..2
        let far: Vec<f64> = (-20..=20).map(|x| x as f64).collect(); // -20..20
        let b = HorizonBands::from_residuals(Duration::hours(1), &[near, far], 0.1);
        let (lo0, hi0) = b.at(Duration::ZERO).unwrap();
        let (lo1, hi1) = b.at(Duration::hours(1)).unwrap();
        assert!(hi1 - lo1 > hi0 - lo0, "h+1 doit être plus large que h+0");
    }

    #[test]
    fn index_is_clamped_to_last_horizon() {
        let b = HorizonBands::from_residuals(Duration::hours(1), &[vec![0.0], vec![5.0]], 0.1);
        // Au-delà du dernier horizon calibré → dernière bande.
        assert_eq!(b.at(Duration::hours(99)), b.at(Duration::hours(1)));
    }

    #[test]
    fn band_quantiles_bracket_the_error() {
        // Erreurs centrées : q10 négatif, q90 positif.
        let errors: Vec<f64> = (-10..=10).map(|x| x as f64).collect();
        let b = HorizonBands::from_residuals(Duration::hours(1), &[errors], 0.1);
        let (low, high) = b.at(Duration::ZERO).unwrap();
        assert!(low < 0.0 && high > 0.0, "low={low}, high={high}");
    }
}
