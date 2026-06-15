//! Recherche du créneau le plus bas-carbone (« greenest window »).

use time::{Duration, OffsetDateTime};

use crate::domain::{CarbonIntensity, ForecastPoint};

/// Estimateur sur lequel optimiser le créneau (ADR-0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEstimator {
    /// Estimation centrale (`expected`) — le cas par défaut.
    Central,
    /// Borne haute (`upper`) — créneau **prudent** (« au pire, l'intensité ne
    /// dépassera pas »), utile pour un engagement.
    Prudent,
}

/// Un créneau temporel et son intensité carbone moyenne (selon l'estimateur).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GreenWindow {
    pub start: OffsetDateTime,
    pub end: OffsetDateTime,
    pub average: CarbonIntensity,
}

/// Trouve, dans une série de **prévisions triées et à pas régulier**, le créneau
/// contigu couvrant au moins `duration` qui minimise l'intensité moyenne — sur
/// l'estimateur central (`expected`) ou prudent (`upper`) selon `estimator`.
///
/// Retourne `None` si la série compte moins de deux points, si le pas est nul,
/// ou si elle est trop courte pour couvrir `duration`.
///
/// Hypothèse : `points` est trié et régulièrement espacé (garanti par
/// l'adapter de prévision). Le pas est déduit des deux premiers points.
pub fn greenest_window(
    points: &[ForecastPoint],
    duration: Duration,
    estimator: WindowEstimator,
) -> Option<GreenWindow> {
    if points.len() < 2 || duration <= Duration::ZERO {
        return None;
    }

    let step = points[1].at - points[0].at;
    if step <= Duration::ZERO {
        return None;
    }

    let value = |p: &ForecastPoint| match estimator {
        WindowEstimator::Central => p.expected.value(),
        WindowEstimator::Prudent => p.upper.value(),
    };

    // Nombre de pas pour couvrir `duration` (arrondi au supérieur).
    let slots = (duration.whole_seconds() as f64 / step.whole_seconds() as f64).ceil() as usize;
    if slots == 0 || points.len() < slots {
        return None;
    }

    // Somme glissante des intensités (sur l'estimateur choisi).
    let mut window_sum: f64 = points[..slots].iter().map(value).sum();
    let mut best_sum = window_sum;
    let mut best_start = 0usize;

    for i in slots..points.len() {
        window_sum += value(&points[i]) - value(&points[i - slots]);
        if window_sum < best_sum {
            best_sum = window_sum;
            best_start = i - slots + 1;
        }
    }

    let average = CarbonIntensity::new(best_sum / slots as f64)?;
    let start = points[best_start].at;
    let end = points[best_start + slots - 1].at + step;
    Some(GreenWindow {
        start,
        end,
        average,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Methodology, ModelVersion, Region};

    /// Point de prévision : bande symétrique de demi-largeur `half` autour de `g`.
    fn p(at: OffsetDateTime, g: f64, half: f64) -> ForecastPoint {
        ForecastPoint::new(
            at,
            Region::National,
            CarbonIntensity::new(g).unwrap(),
            CarbonIntensity::new((g - half).max(0.0)).unwrap(),
            CarbonIntensity::new(g + half).unwrap(),
            Methodology::rte_direct(),
            ModelVersion::new("climatology", 1),
        )
    }

    #[test]
    fn finds_lowest_average_window_on_expected() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let step = Duration::minutes(15);
        let values = [100.0, 90.0, 10.0, 12.0, 80.0];
        let points: Vec<ForecastPoint> = values
            .iter()
            .enumerate()
            .map(|(i, &g)| p(t0 + step * (i as i32), g, 5.0))
            .collect();

        // 30 min = 2 pas ; le meilleur créneau est aux indices 2 et 3.
        let w = greenest_window(&points, Duration::minutes(30), WindowEstimator::Central).unwrap();
        assert_eq!(w.start, t0 + step * 2);
        assert!(w.average.value() < 12.0);
    }

    #[test]
    fn prudent_estimator_optimises_upper_bound() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let step = Duration::minutes(15);
        // Index 0 : expected bas (10) mais très incertain (bande ±50 → upper 60).
        // Index 1 : expected plus haut (30) mais sûr (bande ±2 → upper 32).
        let points = vec![
            p(t0, 10.0, 50.0),
            p(t0 + step, 30.0, 2.0),
            p(t0 + step * 2, 80.0, 2.0),
        ];
        // Créneau d'un pas : en central, le meilleur est l'index 0 (10) ;
        // en prudent, c'est l'index 1 (upper 32 < 60).
        let central =
            greenest_window(&points, Duration::minutes(15), WindowEstimator::Central).unwrap();
        assert_eq!(central.start, t0);
        let prudent =
            greenest_window(&points, Duration::minutes(15), WindowEstimator::Prudent).unwrap();
        assert_eq!(prudent.start, t0 + step);
    }

    #[test]
    fn none_when_series_too_short() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let points = [p(t0, 50.0, 5.0)];
        assert!(
            greenest_window(&points, Duration::minutes(30), WindowEstimator::Central).is_none()
        );
    }
}
