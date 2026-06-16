//! Statistiques agrégées d'intensité carbone (résumés et rollups).

use std::collections::BTreeMap;

use time::OffsetDateTime;

use crate::domain::{CarbonIntensity, Measurement};

/// Statistiques d'intensité sur un ensemble de mesures (gCO₂eq/kWh).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntensityStats {
    pub average: CarbonIntensity,
    pub min: CarbonIntensity,
    pub max: CarbonIntensity,
    /// Nombre de mesures agrégées.
    pub count: u64,
}

/// Un seau temporel d'un rollup : son début et les statistiques associées.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RollupBucket {
    /// Début du seau (aligné sur le pas, en UTC).
    pub start: OffsetDateTime,
    pub stats: IntensityStats,
}

/// Pas d'agrégation d'un rollup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Hourly,
    Daily,
}

impl Granularity {
    /// Étiquette stable (clé d'API, libellé de réponse).
    pub fn label(self) -> &'static str {
        match self {
            Granularity::Hourly => "hour",
            Granularity::Daily => "day",
        }
    }

    /// Pas correspondant à une étiquette d'API, ou `None` si inconnue.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "hour" | "hourly" => Some(Granularity::Hourly),
            "day" | "daily" => Some(Granularity::Daily),
            _ => None,
        }
    }

    /// Pas en secondes (seau aligné sur l'epoch UTC).
    fn step_seconds(self) -> i64 {
        match self {
            Granularity::Hourly => 3_600,
            Granularity::Daily => 86_400,
        }
    }
}

/// Statistiques agrégées sur un ensemble de valeurs d'intensité. `None` si vide.
fn stats_of_values(values: &[f64]) -> Option<IntensityStats> {
    if values.is_empty() {
        return None;
    }
    let sum: f64 = values.iter().sum();
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Some(IntensityStats {
        average: CarbonIntensity::new(sum / values.len() as f64)?,
        min: CarbonIntensity::new(min)?,
        max: CarbonIntensity::new(max)?,
        count: values.len() as u64,
    })
}

/// Résumé (moyenne/min/max/effectif) d'une série de mesures, **calculé dans le
/// domaine** — utilisé pour les méthodes dérivées à la lecture (`acv-ademe@2`)
/// dont la série n'est pas stockée et ne peut donc pas venir de l'agrégat SQL.
pub fn summarize(measurements: &[Measurement]) -> Option<IntensityStats> {
    let values: Vec<f64> = measurements.iter().map(|m| m.intensity.value()).collect();
    stats_of_values(&values)
}

/// Agrège une série de mesures en seaux temporels (`granularity`), alignés sur
/// l'epoch UTC, triés par début croissant — équivalent en mémoire des rollups
/// matérialisés, pour les méthodes calculées à la lecture.
pub fn bucketize(measurements: &[Measurement], granularity: Granularity) -> Vec<RollupBucket> {
    let step = granularity.step_seconds();
    let mut groups: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    for m in measurements {
        let ts = m.at.unix_timestamp();
        let bucket = ts - ts.rem_euclid(step);
        groups.entry(bucket).or_default().push(m.intensity.value());
    }
    groups
        .into_iter()
        .filter_map(|(ts, values)| {
            let start = OffsetDateTime::from_unix_timestamp(ts).ok()?;
            stats_of_values(&values).map(|stats| RollupBucket { start, stats })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Methodology, Region, Vintage};
    use time::Duration;

    fn measure(at: OffsetDateTime, g: f64) -> Measurement {
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
    fn summarize_computes_mean_min_max() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let ms = [measure(t0, 10.0), measure(t0, 20.0), measure(t0, 60.0)];
        let s = summarize(&ms).unwrap();
        assert_eq!(s.average.value(), 30.0);
        assert_eq!(s.min.value(), 10.0);
        assert_eq!(s.max.value(), 60.0);
        assert_eq!(s.count, 3);
        assert!(summarize(&[]).is_none());
    }

    #[test]
    fn bucketize_groups_by_hour() {
        let t0 = OffsetDateTime::UNIX_EPOCH; // borne d'heure
        let ms = [
            measure(t0, 10.0),
            measure(t0 + Duration::minutes(30), 20.0),
            measure(t0 + Duration::hours(1), 60.0),
        ];
        let buckets = bucketize(&ms, Granularity::Hourly);
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].start, t0);
        assert_eq!(buckets[0].stats.average.value(), 15.0); // (10+20)/2
        assert_eq!(buckets[1].stats.average.value(), 60.0);
    }
}
