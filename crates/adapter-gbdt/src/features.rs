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
pub const FEATURE_SIZE: usize = 8;

/// Décalage d'une semaine (saisonnalité hebdomadaire).
const WEEK: Duration = Duration::weeks(1);

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
/// 8. anomalie récente = intensité(ancre) − intensité(ancre − 1 sem.).
pub fn build_features(
    origin: OffsetDateTime,
    target: OffsetDateTime,
    anchor_at: OffsetDateTime,
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

    Some(vec![
        horizon_hours,
        t.hour() as f32,
        weekday as f32,
        if weekday >= 5 { 1.0 } else { 0.0 },
        t.month() as u8 as f32,
        lag_week as f32,
        lag_anchor as f32,
        anomaly as f32,
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

        let f = build_features(origin, target, origin, &intensity).unwrap();
        assert_eq!(f.len(), FEATURE_SIZE);
        assert_eq!(f[0], 6.0); // horizon 6 h
        assert_eq!(f[5], 55.0); // lag semaine sur la cible
        assert_eq!(f[6], 50.0); // ancre
        assert_eq!(f[7], 10.0); // anomalie 50 − 40
    }

    #[test]
    fn none_when_required_lag_missing() {
        let origin = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let target = origin + Duration::hours(6);
        // Aucun lag → None.
        assert!(build_features(origin, target, origin, &HashMap::new()).is_none());
    }
}
