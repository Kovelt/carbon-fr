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

/// Moyenne **non-nulle** d'une colonne au pas `index`, ou `None` si aucun
/// point n'a de valeur — l'absence de donnée n'est pas un `0` physique
/// (0 = calme plat / nuit, valeurs légitimes).
fn mean_at(columns: &[&[Option<f64>]], index: usize) -> Option<f64> {
    let (sum, n) = columns
        .iter()
        .filter_map(|col| col.get(index).copied().flatten())
        .fold((0.0, 0u32), |(s, n), v| (s + v, n + 1));
    if n == 0 { None } else { Some(sum / n as f64) }
}

/// Agrège les réponses par point en une série nationale (moyenne des points),
/// datée `(run_at, valid_at)`. Un créneau dont **aucun** point n'a de valeur
/// (vent ET irradiance nuls partout) est **sauté** plutôt qu'enregistré à 0,0 :
/// l'archive Open-Meteo répond tout-`null` avant ~2017 pour ces variables, et
/// des zéros fabriqués seraient servis comme donnée (audit 2026-08). Les
/// consommateurs tolèrent la série creuse (jointures horaires, séries à trous).
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

    let mut out = Vec::with_capacity(first.hourly.time.len());
    for (i, ts) in first.hourly.time.iter().enumerate() {
        let valid_at = PrimitiveDateTime::parse(ts, TIME_FORMAT)
            .map(PrimitiveDateTime::assume_utc)
            .map_err(|e| SourceError::Invalid(format!("horodatage météo « {ts} » : {e}")))?;
        let wind = mean_at(&winds, i);
        let irradiance = mean_at(&irradiances, i);
        if wind.is_none() && irradiance.is_none() {
            // Créneau sans aucune donnée : sauté (pas de 0,0 inventé).
            continue;
        }
        out.push(WeatherForecast {
            run_at,
            valid_at,
            // Cas mixte (une seule variable absente) : 0,0 faute de champs
            // optionnels dans `WeatherForecast` — limitation documentée,
            // jamais observée sur l'API réelle (les deux variables sont
            // couvertes sur la même période).
            wind: wind.unwrap_or(0.0),
            irradiance: irradiance.unwrap_or(0.0),
        });
    }
    Ok(out)
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

    /// Créneaux tout-`null` (bord de l'archive, ex. toute 2016) : aucun
    /// `WeatherForecast` fabriqué à 0,0 (audit 2026-08).
    #[test]
    fn all_null_slots_are_skipped() {
        let bodies: Vec<OpenMeteoResponse> = serde_json::from_str(
            r#"[
                {"hourly": {
                    "time": ["2016-06-15T00:00", "2016-06-15T01:00"],
                    "wind_speed_100m": [null, null],
                    "shortwave_radiation": [null, null]
                }},
                {"hourly": {
                    "time": ["2016-06-15T00:00", "2016-06-15T01:00"],
                    "wind_speed_100m": [null, null],
                    "shortwave_radiation": [null, null]
                }}
            ]"#,
        )
        .unwrap();
        let out = aggregate_national(OffsetDateTime::UNIX_EPOCH, &bodies).unwrap();
        assert!(out.is_empty());
    }

    /// Payload partiel : le créneau tout-`null` est sauté, les créneaux avec
    /// donnée sont conservés tels quels (série creuse, pas de zéro inventé).
    #[test]
    fn partial_payload_keeps_present_values() {
        let bodies: Vec<OpenMeteoResponse> = serde_json::from_str(
            r#"[
                {"hourly": {
                    "time": ["2026-06-15T00:00", "2026-06-15T01:00", "2026-06-15T02:00"],
                    "wind_speed_100m": [null, 12.0, 8.0],
                    "shortwave_radiation": [null, null, 40.0]
                }}
            ]"#,
        )
        .unwrap();
        let out = aggregate_national(OffsetDateTime::UNIX_EPOCH, &bodies).unwrap();
        // t0 tout-null sauté ; t1 (vent seul) et t2 conservés.
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].valid_at.hour(), 1);
        assert_eq!(out[0].wind, 12.0);
        assert_eq!(out[0].irradiance, 0.0); // cas mixte : repli documenté
        assert_eq!(out[1].valid_at.hour(), 2);
        assert_eq!(out[1].wind, 8.0);
        assert_eq!(out[1].irradiance, 40.0);
    }
}
