//! Primitives de scheduling **carbon-aware** (ADR-0014 §1).
//!
//! Toutes des **fonctions pures** sur une prévision (`&[ForecastPoint]`),
//! réutilisant le sélecteur central/prudent ([`WindowEstimator`], ADR-0011).
//! Aucun port, aucune IO : ce sont des conseils calculés sur la prévision, **pas
//! du pilotage réseau** (non-objectif assumé, ADR-0014).

use time::{Duration, OffsetDateTime};

use crate::domain::{
    CarbonIntensity, ForecastPoint, GreenWindow, WindowEstimator, greenest_window,
};

/// Valeur d'un point selon l'estimateur (central `expected` / prudent `upper`).
fn estimate(p: &ForecastPoint, estimator: WindowEstimator) -> f64 {
    match estimator {
        WindowEstimator::Central => p.expected.value(),
        WindowEstimator::Prudent => p.upper.value(),
    }
}

/// Pas de la série (déduit des deux premiers points), ou `None` si < 2 points
/// ou pas non strictement positif.
fn step_of(points: &[ForecastPoint]) -> Option<Duration> {
    if points.len() < 2 {
        return None;
    }
    let step = points[1].at - points[0].at;
    (step > Duration::ZERO).then_some(step)
}

/// Un créneau candidat (un pas de prévision) et son intensité selon l'estimateur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScheduleSlot {
    pub at: OffsetDateTime,
    pub intensity: CarbonIntensity,
}

/// Créneau contigu le plus bas-carbone **à livrer avant `deadline`** : borne la
/// série aux créneaux dont la fin (`at + pas`) est `≤ deadline`, puis applique
/// [`greenest_window`]. Généralise `greenest_window` avec une échéance.
///
/// `None` si la série est trop courte, ou si aucun créneau de `duration` ne tient
/// avant l'échéance.
pub fn greenest_window_before(
    points: &[ForecastPoint],
    duration: Duration,
    deadline: OffsetDateTime,
    estimator: WindowEstimator,
) -> Option<GreenWindow> {
    let step = step_of(points)?;
    // On garde les points dont le créneau se termine au plus tard à l'échéance.
    let cutoff = points
        .iter()
        .take_while(|p| p.at + step <= deadline)
        .count();
    greenest_window(&points[..cutoff], duration, estimator)
}

/// Les `k` créneaux **les moins intenses** de la série, **pas forcément
/// contigus**, triés par horodatage croissant.
///
/// **Hypothèse explicite (ADR-0014 §1)** : interruptibilité parfaite — les
/// créneaux sont traités comme indépendants (convient à un job divisible et
/// pausable, pas à une charge qui doit tourner d'un trait).
pub fn lowest_slots(
    points: &[ForecastPoint],
    k: usize,
    estimator: WindowEstimator,
) -> Vec<ScheduleSlot> {
    if k == 0 {
        return Vec::new();
    }
    let mut slots: Vec<ScheduleSlot> = points
        .iter()
        .filter_map(|p| {
            CarbonIntensity::new(estimate(p, estimator)).map(|intensity| ScheduleSlot {
                at: p.at,
                intensity,
            })
        })
        .collect();
    // Sélection des k plus bas, puis remise en ordre chronologique.
    slots.sort_by(|a, b| {
        a.intensity
            .value()
            .partial_cmp(&b.intensity.value())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    slots.truncate(k);
    slots.sort_by_key(|s| s.at);
    slots
}

/// Tous les créneaux d'intensité **strictement inférieure à `threshold`**
/// (gCO₂eq/kWh) sur l'horizon, triés par horodatage croissant.
pub fn slots_below(
    points: &[ForecastPoint],
    threshold: f64,
    estimator: WindowEstimator,
) -> Vec<ScheduleSlot> {
    points
        .iter()
        .filter_map(|p| {
            let value = estimate(p, estimator);
            if value < threshold {
                CarbonIntensity::new(value).map(|intensity| ScheduleSlot {
                    at: p.at,
                    intensity,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Économie carbone d'un créneau planifié **vs « maintenant »** (ADR-0014 §1).
///
/// « Maintenant » = premier point de la série. Sans énergie de job, on expose le
/// delta d'intensité et la fraction ; avec l'énergie (`kWh`), l'économie
/// **absolue** en gCO₂eq — ce qui rend l'API actionnable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Savings {
    /// Intensité « maintenant » (premier point), selon l'estimateur.
    pub now: CarbonIntensity,
    /// Intensité du créneau retenu.
    pub scheduled: CarbonIntensity,
    /// Réduction d'intensité (gCO₂eq/kWh) : `now − scheduled` (positive = gain).
    pub intensity_delta: f64,
    /// Réduction relative (fraction de `now`, ex. `0.4` = −40 %).
    pub fraction: f64,
    /// Économie **absolue** (gCO₂eq) si l'énergie du job (`kWh`) est fournie.
    pub absolute_g: Option<f64>,
}

/// Construit l'[`Savings`] d'un créneau d'intensité `scheduled` par rapport au
/// premier point de `points`. `None` si la série est vide ou `now` non défini.
pub fn savings_vs_now(
    points: &[ForecastPoint],
    scheduled: CarbonIntensity,
    energy_kwh: Option<f64>,
    estimator: WindowEstimator,
) -> Option<Savings> {
    let first = points.first()?;
    let now = CarbonIntensity::new(estimate(first, estimator))?;
    let intensity_delta = now.value() - scheduled.value();
    let fraction = if now.value() > 0.0 {
        intensity_delta / now.value()
    } else {
        0.0
    };
    let absolute_g = energy_kwh.map(|kwh| intensity_delta * kwh);
    Some(Savings {
        now,
        scheduled,
        intensity_delta,
        fraction,
        absolute_g,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Methodology, ModelVersion, Region};

    fn p(at: OffsetDateTime, g: f64) -> ForecastPoint {
        ForecastPoint::new(
            at,
            Region::National,
            CarbonIntensity::new(g).unwrap(),
            CarbonIntensity::new((g - 5.0).max(0.0)).unwrap(),
            CarbonIntensity::new(g + 5.0).unwrap(),
            Methodology::rte_direct(),
            ModelVersion::new("climatology", 1),
        )
    }

    fn series(values: &[f64]) -> Vec<ForecastPoint> {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let step = Duration::minutes(15);
        values
            .iter()
            .enumerate()
            .map(|(i, &g)| p(t0 + step * (i as i32), g))
            .collect()
    }

    #[test]
    fn deadline_excludes_later_cheaper_window() {
        // Le créneau le moins cher (10) est en fin de série, après l'échéance.
        let points = series(&[100.0, 40.0, 50.0, 10.0, 12.0]);
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let step = Duration::minutes(15);
        // Échéance = fin de l'index 2 → seuls les indices 0..=2 sont éligibles.
        let deadline = t0 + step * 3;
        let w = greenest_window_before(
            &points,
            Duration::minutes(15),
            deadline,
            WindowEstimator::Central,
        )
        .unwrap();
        // Le meilleur avant l'échéance est l'index 1 (40), pas l'index 3 (10).
        assert_eq!(w.start, t0 + step);
        assert!(w.average.value() < 50.0 && w.average.value() >= 40.0);
    }

    #[test]
    fn lowest_slots_picks_k_cheapest_in_time_order() {
        let points = series(&[100.0, 10.0, 80.0, 20.0, 90.0]);
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let step = Duration::minutes(15);
        let slots = lowest_slots(&points, 2, WindowEstimator::Central);
        assert_eq!(slots.len(), 2);
        // Les deux moins chers (10 @1, 20 @3), rendus en ordre chronologique.
        assert_eq!(slots[0].at, t0 + step);
        assert_eq!(slots[1].at, t0 + step * 3);
        assert!(slots[0].intensity.value() < slots[1].intensity.value());
    }

    #[test]
    fn lowest_slots_caps_at_series_length() {
        let points = series(&[30.0, 20.0]);
        assert_eq!(lowest_slots(&points, 10, WindowEstimator::Central).len(), 2);
        assert!(lowest_slots(&points, 0, WindowEstimator::Central).is_empty());
    }

    #[test]
    fn slots_below_threshold_keeps_time_order() {
        let points = series(&[100.0, 40.0, 55.0, 30.0]);
        let below = slots_below(&points, 50.0, WindowEstimator::Central);
        assert_eq!(below.len(), 2);
        assert_eq!(below[0].intensity.value(), 40.0);
        assert_eq!(below[1].intensity.value(), 30.0);
    }

    #[test]
    fn savings_absolute_needs_energy() {
        let points = series(&[100.0, 20.0]);
        let scheduled = CarbonIntensity::new(20.0).unwrap();

        // Sans énergie : delta + fraction, pas d'absolu.
        let relative = savings_vs_now(&points, scheduled, None, WindowEstimator::Central).unwrap();
        assert_eq!(relative.intensity_delta, 80.0);
        assert!((relative.fraction - 0.8).abs() < 1e-9);
        assert!(relative.absolute_g.is_none());

        // Avec 10 kWh : 80 gCO2/kWh × 10 = 800 gCO2 économisés.
        let absolute =
            savings_vs_now(&points, scheduled, Some(10.0), WindowEstimator::Central).unwrap();
        assert_eq!(absolute.absolute_g, Some(800.0));
    }
}
