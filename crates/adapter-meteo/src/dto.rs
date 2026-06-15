//! DTO de désérialisation Open-Meteo et agrégation nationale.

use carbonfr_core::domain::WeatherForecast;
use carbonfr_core::ports::SourceError;
use serde::Deserialize;
use time::{OffsetDateTime, PrimitiveDateTime, format_description::FormatItem};

/// Réponse Open-Meteo pour un point.
#[derive(Debug, Deserialize)]
pub(crate) struct OpenMeteoResponse {
    pub hourly: Hourly,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Hourly {
    pub time: Vec<String>,
    pub wind_speed_100m: Vec<Option<f64>>,
    pub shortwave_radiation: Vec<Option<f64>>,
}

/// Format des horodatages Open-Meteo (`timezone=UTC`) : `YYYY-MM-DDTHH:MM`.
const TIME_FORMAT: &[FormatItem<'_>] =
    time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]");

/// Moyenne **non-nulle** d'une colonne au pas `index`, ou `0` si tout est nul.
fn mean_at(columns: &[&[Option<f64>]], index: usize) -> f64 {
    let (sum, n) = columns
        .iter()
        .filter_map(|col| col.get(index).copied().flatten())
        .fold((0.0, 0u32), |(s, n), v| (s + v, n + 1));
    if n == 0 { 0.0 } else { sum / n as f64 }
}

/// Agrège les réponses par point en une série nationale (moyenne des points),
/// datée `(run_at, valid_at)`.
pub(crate) fn aggregate_national(
    run_at: OffsetDateTime,
    bodies: &[OpenMeteoResponse],
) -> Result<Vec<WeatherForecast>, SourceError> {
    let first = bodies
        .first()
        .ok_or_else(|| SourceError::Invalid("réponse Open-Meteo vide".into()))?;

    let winds: Vec<&[Option<f64>]> = bodies
        .iter()
        .map(|b| b.hourly.wind_speed_100m.as_slice())
        .collect();
    let irradiances: Vec<&[Option<f64>]> = bodies
        .iter()
        .map(|b| b.hourly.shortwave_radiation.as_slice())
        .collect();

    first
        .hourly
        .time
        .iter()
        .enumerate()
        .map(|(i, ts)| {
            let valid_at = PrimitiveDateTime::parse(ts, TIME_FORMAT)
                .map(PrimitiveDateTime::assume_utc)
                .map_err(|e| SourceError::Invalid(format!("horodatage météo « {ts} » : {e}")))?;
            Ok(WeatherForecast {
                run_at,
                valid_at,
                wind: mean_at(&winds, i),
                irradiance: mean_at(&irradiances, i),
            })
        })
        .collect()
}

/// Comme [`aggregate_national`], mais pour l'**archive** : chaque prévision est
/// datée d'un `run_at = valid_at − 24 h` (prévision J-1), ce qui préserve
/// l'anti-fuite au backtest (ADR-0012 §6) — on n'utilise un point que pour des
/// horizons ≥ son délai de production.
pub(crate) fn aggregate_historical(
    bodies: &[OpenMeteoResponse],
) -> Result<Vec<WeatherForecast>, SourceError> {
    let mut out = aggregate_national(time::OffsetDateTime::UNIX_EPOCH, bodies)?;
    for f in &mut out {
        f.run_at = f.valid_at - time::Duration::days(1);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
        {"hourly": {
            "time": ["2026-06-15T00:00", "2026-06-15T01:00"],
            "wind_speed_100m": [10.0, 20.0],
            "shortwave_radiation": [0.0, 100.0]
        }},
        {"hourly": {
            "time": ["2026-06-15T00:00", "2026-06-15T01:00"],
            "wind_speed_100m": [30.0, null],
            "shortwave_radiation": [0.0, 200.0]
        }}
    ]"#;

    #[test]
    fn aggregates_points_and_parses_times() {
        let bodies: Vec<OpenMeteoResponse> = serde_json::from_str(SAMPLE).unwrap();
        let run = OffsetDateTime::UNIX_EPOCH;
        let out = aggregate_national(run, &bodies).unwrap();

        assert_eq!(out.len(), 2);
        // t0 : vent (10+30)/2 = 20 ; irradiance (0+0)/2 = 0.
        assert_eq!(out[0].wind, 20.0);
        assert_eq!(out[0].irradiance, 0.0);
        assert_eq!(out[0].valid_at.hour(), 0);
        assert_eq!(out[0].valid_at.offset(), time::UtcOffset::UTC);
        // t1 : vent moyenne des non-nuls = 20 ; irradiance (100+200)/2 = 150.
        assert_eq!(out[1].wind, 20.0);
        assert_eq!(out[1].irradiance, 150.0);
        assert_eq!(out[1].run_at, run);
    }

    #[test]
    fn empty_bodies_is_invalid() {
        assert!(aggregate_national(OffsetDateTime::UNIX_EPOCH, &[]).is_err());
    }
}
