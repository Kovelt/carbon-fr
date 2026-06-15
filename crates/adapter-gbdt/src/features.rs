//! *Feature engineering* (ADR-0012) — **partagé** entre l'entraînement
//! (`bin/train`) et l'inférence (`GbdtForecaster`).
//!
//! Frontière hexagonale : la transformation en features vit dans l'adapter, pas
//! dans le `core`. La règle d'or est l'**identité** train/inférence — d'où ce
//! module unique. Toutes les features sont **connues à l'instant de la
//! prédiction** `origin` (anti-fuite) : pas de valeur observée postérieure.

use std::collections::HashMap;

use time::{Duration, OffsetDateTime, UtcOffset};

/// Nombre de features (taille du vecteur d'entrée du GBDT).
pub const FEATURE_SIZE: usize = 11;

/// Décalage d'une semaine (saisonnalité hebdomadaire).
const WEEK: Duration = Duration::weeks(1);

/// Index du créneau dans la semaine (`jour × pas`), en UTC — même découpage que
/// la climatologie du `core`. Sert à la climatologie de créneau (feature de base
/// pour l'apprentissage résiduel, ADR-0012).
pub fn week_slot(at: OffsetDateTime, step_secs: i64) -> i64 {
    let t = at.to_offset(UtcOffset::UTC);
    let weekday = t.weekday().number_days_from_monday() as i64;
    let secs_in_day = t.hour() as i64 * 3600 + t.minute() as i64 * 60 + t.second() as i64;
    let slots_per_day = if step_secs > 0 {
        86_400 / step_secs
    } else {
        96
    };
    weekday * slots_per_day + secs_in_day / step_secs.max(1)
}

/// Climatologie d'intensité par créneau de semaine (moyenne des observations).
pub fn slot_climatology(
    intensity: &HashMap<OffsetDateTime, f64>,
    step_secs: i64,
) -> HashMap<i64, f64> {
    let mut acc: HashMap<i64, (f64, u32)> = HashMap::new();
    for (&at, &v) in intensity {
        let e = acc.entry(week_slot(at, step_secs)).or_insert((0.0, 0));
        e.0 += v;
        e.1 += 1;
    }
    acc.into_iter()
        .map(|(slot, (sum, n))| (slot, sum / n as f64))
        .collect()
}

/// Construit le vecteur de features pour **prévoir l'intensité à `target`**
/// depuis l'origine `origin`. `anchor_at` est l'horodatage de la **dernière
/// observation disponible** à l'origine (l'origine elle-même n'est pas
/// nécessairement publiée) — ce qui garantit l'identité train/inférence.
/// Retourne `None` si un lag requis manque (créneau hors historique).
///
/// Features (toutes ≤ `origin`) :
/// 1. horizon (heures depuis l'origine) ;
/// 2. heure du jour ; 3. jour de semaine ; 4. week-end ; 5. mois ;
/// 6. intensité à `target − 1 sem.` (analogue saisonnier) ;
/// 7. intensité à l'ancre (persistance) ;
/// 8. anomalie récente = intensité(ancre) − intensité(ancre − 1 sem.) ;
/// 9. climatologie d'intensité au créneau de `target` (base, apprentissage résiduel) ;
/// 10. vent prévu à `target` (météo, ADR-0012 ; 0 si absent) ;
/// 11. irradiance prévue à `target` (0 si absente).
pub fn build_features(
    origin: OffsetDateTime,
    target: OffsetDateTime,
    anchor_at: OffsetDateTime,
    climo_target: f64,
    weather: Option<(f64, f64)>,
    intensity: &HashMap<OffsetDateTime, f64>,
) -> Option<Vec<f32>> {
    let t = target.to_offset(UtcOffset::UTC);

    // Lags requis (tous ≤ l'origine).
    let lag_week = *intensity.get(&(target - WEEK))?;
    let lag_anchor = *intensity.get(&anchor_at)?;
    // Anomalie récente : 0 si l'analogue de l'ancre manque.
    let anomaly = intensity
        .get(&(anchor_at - WEEK))
        .map(|prev| lag_anchor - prev)
        .unwrap_or(0.0);

    let horizon_hours = (target - origin).whole_minutes() as f32 / 60.0;
    let weekday = t.weekday().number_days_from_monday();
    let (wind, irradiance) = weather.unwrap_or((0.0, 0.0));

    Some(vec![
        horizon_hours,
        t.hour() as f32,
        weekday as f32,
        if weekday >= 5 { 1.0 } else { 0.0 },
        t.month() as u8 as f32,
        lag_week as f32,
        lag_anchor as f32,
        anomaly as f32,
        climo_target as f32,
        wind as f32,
        irradiance as f32,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_when_lags_present() {
        let origin = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let target = origin + Duration::hours(6);
        let mut intensity = HashMap::new();
        intensity.insert(origin, 50.0); // ancre = origine
        intensity.insert(origin - WEEK, 40.0);
        intensity.insert(target - WEEK, 55.0);

        let f = build_features(
            origin,
            target,
            origin,
            48.0,
            Some((30.0, 200.0)),
            &intensity,
        )
        .unwrap();
        assert_eq!(f.len(), FEATURE_SIZE);
        assert_eq!(f[0], 6.0); // horizon 6 h
        assert_eq!(f[5], 55.0); // lag semaine sur la cible
        assert_eq!(f[6], 50.0); // ancre
        assert_eq!(f[7], 10.0); // anomalie 50 − 40
        assert_eq!(f[8], 48.0); // climatologie de créneau
        assert_eq!(f[9], 30.0); // vent prévu
        assert_eq!(f[10], 200.0); // irradiance prévue
    }

    #[test]
    fn weather_absent_is_zero() {
        let origin = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let target = origin + Duration::hours(6);
        let mut intensity = HashMap::new();
        intensity.insert(origin, 50.0);
        intensity.insert(target - WEEK, 55.0);
        let f = build_features(origin, target, origin, 48.0, None, &intensity).unwrap();
        assert_eq!(f[9], 0.0);
        assert_eq!(f[10], 0.0);
    }

    #[test]
    fn none_when_required_lag_missing() {
        let origin = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let target = origin + Duration::hours(6);
        // Aucun lag → None.
        assert!(build_features(origin, target, origin, 0.0, None, &HashMap::new()).is_none());
    }
}
