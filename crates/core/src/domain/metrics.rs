//! Métriques d'erreur de prévision (gCO₂eq/kWh) — pures, sans IO.
//!
//! Sert au backtest (ADR-0009) : mesurer la précision d'un modèle plutôt que la
//! supposer. L'[`ErrorAccumulator`] agrège les erreurs au fil de l'eau (somme
//! des valeurs absolues et des carrés) sans conserver toutes les paires, ce qui
//! permet de ventiler par horizon à coût constant.

/// Métriques d'erreur agrégées : erreur absolue moyenne (MAE) et erreur
/// quadratique moyenne (RMSE), en gCO₂eq/kWh, sur `n` paires.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrorMetrics {
    pub mae: f64,
    pub rmse: f64,
    pub n: usize,
}

/// Accumulateur d'erreurs prévu/observé. Pousser chaque paire avec
/// [`observe`](ErrorAccumulator::observe), puis lire [`metrics`](ErrorAccumulator::metrics).
#[derive(Debug, Clone, Copy, Default)]
pub struct ErrorAccumulator {
    sum_abs: f64,
    sum_sq: f64,
    n: usize,
}

impl ErrorAccumulator {
    /// Intègre une paire (valeur prévue, valeur observée).
    pub fn observe(&mut self, predicted: f64, observed: f64) {
        let error = predicted - observed;
        self.sum_abs += error.abs();
        self.sum_sq += error * error;
        self.n += 1;
    }

    /// Nombre de paires accumulées.
    pub fn count(&self) -> usize {
        self.n
    }

    /// Métriques agrégées, ou `None` si aucune paire (MAE/RMSE indéfinies).
    pub fn metrics(&self) -> Option<ErrorMetrics> {
        if self.n == 0 {
            return None;
        }
        let n = self.n as f64;
        Some(ErrorMetrics {
            mae: self.sum_abs / n,
            rmse: (self.sum_sq / n).sqrt(),
            n: self.n,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_has_no_metrics() {
        assert!(ErrorAccumulator::default().metrics().is_none());
    }

    #[test]
    fn perfect_prediction_is_zero_error() {
        let mut acc = ErrorAccumulator::default();
        for v in [10.0, 20.0, 30.0] {
            acc.observe(v, v);
        }
        let m = acc.metrics().unwrap();
        assert_eq!(m.mae, 0.0);
        assert_eq!(m.rmse, 0.0);
        assert_eq!(m.n, 3);
    }

    #[test]
    fn constant_offset_gives_that_mae_and_rmse() {
        // Erreur constante de +5 sur chaque paire → MAE = RMSE = 5.
        let mut acc = ErrorAccumulator::default();
        for v in [0.0, 100.0, 50.0, 7.0] {
            acc.observe(v + 5.0, v);
        }
        let m = acc.metrics().unwrap();
        assert!((m.mae - 5.0).abs() < 1e-9);
        assert!((m.rmse - 5.0).abs() < 1e-9);
    }

    #[test]
    fn rmse_penalises_large_errors_more_than_mae() {
        // Erreurs {0, 10} → MAE = 5 ; RMSE = sqrt((0+100)/2) ≈ 7,07.
        let mut acc = ErrorAccumulator::default();
        acc.observe(10.0, 10.0); // erreur 0
        acc.observe(20.0, 10.0); // erreur 10
        let m = acc.metrics().unwrap();
        assert!((m.mae - 5.0).abs() < 1e-9);
        assert!((m.rmse - (50.0_f64).sqrt()).abs() < 1e-9);
        assert!(m.rmse > m.mae);
    }
}
