//! Recherche du créneau le plus bas-carbone (« greenest window »).

use time::{Duration, OffsetDateTime};

use crate::domain::{CarbonIntensity, Measurement};

/// Un créneau temporel et son intensité carbone moyenne.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GreenWindow {
    pub start: OffsetDateTime,
    pub end: OffsetDateTime,
    pub average: CarbonIntensity,
}

/// Trouve, dans une série de mesures **triées chronologiquement et à pas
/// régulier**, le créneau contigu couvrant au moins `duration` qui minimise
/// l'intensité carbone moyenne.
///
/// Retourne `None` si la série compte moins de deux points, si le pas est nul,
/// ou si elle est trop courte pour couvrir `duration`.
///
/// Hypothèse : `points` est trié et régulièrement espacé (garanti par
/// l'adapter de prévision). Le pas est déduit des deux premiers points.
pub fn greenest_window(points: &[Measurement], duration: Duration) -> Option<GreenWindow> {
    if points.len() < 2 || duration <= Duration::ZERO {
        return None;
    }

    let step = points[1].at - points[0].at;
    if step <= Duration::ZERO {
        return None;
    }

    // Nombre de pas pour couvrir `duration` (arrondi au supérieur).
    let slots = (duration.whole_seconds() as f64 / step.whole_seconds() as f64).ceil() as usize;
    if slots == 0 || points.len() < slots {
        return None;
    }

    // Somme glissante des intensités.
    let mut window_sum: f64 = points[..slots].iter().map(|m| m.intensity.value()).sum();
    let mut best_sum = window_sum;
    let mut best_start = 0usize;

    for i in slots..points.len() {
        window_sum += points[i].intensity.value() - points[i - slots].intensity.value();
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
    use crate::domain::{Methodology, Region, Vintage};

    fn m(at: OffsetDateTime, g: f64) -> Measurement {
        Measurement {
            at,
            region: Region::National,
            intensity: CarbonIntensity::new(g).unwrap(),
            methodology: Methodology::rte_direct(),
            vintage: Vintage::Tr,
            mix: None,
        }
    }

    #[test]
    fn finds_lowest_average_window() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let step = Duration::minutes(15);
        let values = [100.0, 90.0, 10.0, 12.0, 80.0];
        let points: Vec<Measurement> = values
            .iter()
            .enumerate()
            .map(|(i, &g)| m(t0 + step * (i as i32), g))
            .collect();

        // 30 min = 2 pas ; le meilleur créneau est aux indices 2 et 3.
        let w = greenest_window(&points, Duration::minutes(30)).unwrap();
        assert_eq!(w.start, t0 + step * 2);
        assert!(w.average.value() < 12.0);
    }

    #[test]
    fn none_when_series_too_short() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let points = [m(t0, 50.0)];
        assert!(greenest_window(&points, Duration::minutes(30)).is_none());
    }
}
