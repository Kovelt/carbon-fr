//! Prévision d'intensité carbone par **climatologie** (`climatology@1`,
//! ADR-0009).
//!
//! Modèle pur, déterministe et explicable, sans dépendance externe : il ne
//! consomme que l'historique observé (fourni par l'adapter, lu via
//! `IntensityRepository`) et en extrapole une série future. Aucune IO ici — le
//! calcul est testable en mémoire, comme [`greenest_window`](super::greenest_window).
//!
//! Formule (ADR-0009) : pour une cible à l'horodatage `t`,
//!
//! ```text
//! prévision(t) = max(0,  C(t) + b · exp(−|t − t₀| / τ))
//! ```
//!
//! où `C(t)` est la **climatologie horaire-de-semaine** (moyenne des intensités
//! observées au même créneau `jour-de-semaine × heure × quart`), `t₀`/`o` la
//! dernière observation, et `b = o − C(t₀)` le **biais de persistance** propagé
//! en décroissant avec l'horizon (constante `τ`).

use std::collections::HashMap;

use time::{Duration, OffsetDateTime, UtcOffset};

use crate::domain::{CarbonIntensity, Measurement, Vintage};

/// Identité **versionnée** du modèle de prévision (ADR-0009), exposée par l'API.
/// Comme la méthodologie, elle ne change jamais en silence : une évolution de la
/// formule ou des paramètres = nouvelle version + ADR.
pub const CLIMATOLOGY_ID: &str = "climatology";
pub const CLIMATOLOGY_VERSION: u32 = 1;

/// Paramètres du modèle `climatology@1`.
///
/// `weeks` (la profondeur d'historique) n'en fait **pas** partie : c'est une
/// préoccupation d'adapter (combien de passé aller chercher). La fonction pure
/// travaille sur l'historique qu'on lui donne.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimatologyParams {
    /// Pas natif de la série (15 min sur éCO2mix).
    pub step: Duration,
    /// Constante de décroissance de la correction de persistance (ADR-0009).
    pub tau: Duration,
}

impl Default for ClimatologyParams {
    fn default() -> Self {
        Self {
            step: Duration::minutes(15),
            tau: Duration::hours(6),
        }
    }
}

/// Garde-fou : refuse un horizon absurde au regard du pas (évite d'allouer une
/// série démesurée si l'adapter est mal câblé). 100 000 pas = ~2,8 ans à 15 min.
const MAX_STEPS: i64 = 100_000;

/// Prévoit la série d'intensité carbone sur `[from, from + horizon)` au pas
/// `params.step`, par climatologie horaire-de-semaine corrigée d'un biais de
/// persistance décroissant (ADR-0009).
///
/// `history` : observations **passées** d'une même cible `(region,
/// methodology)`, idéalement triées et au pas natif ; seul son contenu compte
/// (l'ordre n'est pas requis pour la climatologie, mais la dernière par
/// horodatage sert d'ancre de persistance). Les points prévus reprennent la
/// `region` et la `methodology` de l'historique ; ils portent `Vintage::Tr` —
/// rang le moins autoritaire — car une prévision **n'est pas une mesure** et
/// n'est jamais persistée (ADR-0009 ; ADR-0006 intacte).
///
/// Retourne `None` si l'historique est vide ou si les paramètres/horizon sont
/// invalides (pas/τ/horizon ≤ 0, ou horizon démesuré).
pub fn climatology_forecast(
    history: &[Measurement],
    from: OffsetDateTime,
    horizon: Duration,
    params: ClimatologyParams,
) -> Option<Vec<Measurement>> {
    let step_secs = params.step.whole_seconds();
    let tau_secs = params.tau.whole_seconds() as f64;
    if history.is_empty()
        || step_secs <= 0
        || tau_secs <= 0.0
        || horizon <= Duration::ZERO
        || horizon.whole_seconds() / step_secs > MAX_STEPS
    {
        return None;
    }

    // Ancre de persistance : l'observation la plus récente.
    let anchor = history.iter().max_by_key(|m| m.at)?;
    let region = anchor.region;
    let methodology = anchor.methodology.clone();
    let t0 = anchor.at;
    let o = anchor.intensity.value();

    // Climatologie : moyenne par créneau de la semaine, + moyenne globale en
    // repli (créneau jamais observé, démarrage à froid — ADR-0009).
    let mut slots: HashMap<i64, (f64, u32)> = HashMap::new();
    let mut total = 0.0;
    for m in history {
        let entry = slots.entry(week_slot(m.at, step_secs)).or_insert((0.0, 0));
        entry.0 += m.intensity.value();
        entry.1 += 1;
        total += m.intensity.value();
    }
    let overall_mean = total / history.len() as f64;
    let climatology = |t: OffsetDateTime| -> f64 {
        match slots.get(&week_slot(t, step_secs)) {
            Some(&(sum, n)) if n > 0 => sum / n as f64,
            _ => overall_mean,
        }
    };

    let bias = o - climatology(t0);

    let end = from + horizon;
    let mut points = Vec::new();
    let mut t = from;
    while t < end {
        // |t − t₀| : la correction décroît en s'éloignant de l'ancre, dans les
        // deux sens (l'adapter prévoit normalement vers le futur, t ≥ t₀).
        let dt = (t - t0).abs().whole_seconds() as f64;
        let value = (climatology(t) + bias * (-dt / tau_secs).exp()).max(0.0);
        // value ≥ 0 par construction → `new` ne peut échouer (sauf NaN, exclu
        // car toutes les entrées sont finies).
        if let Some(intensity) = CarbonIntensity::new(value) {
            points.push(Measurement {
                at: t,
                region,
                intensity,
                methodology: methodology.clone(),
                vintage: Vintage::Tr,
                mix: None,
            });
        }
        t += params.step;
    }
    Some(points)
}

/// Index du créneau dans la semaine (`jour-de-semaine × pas`), en UTC pour un
/// découpage déterministe indépendant du fuseau (cohérent avec les rollups,
/// ADR-0004). Ex. à 15 min : 7 × 96 = 672 créneaux.
fn week_slot(t: OffsetDateTime, step_secs: i64) -> i64 {
    let t = t.to_offset(UtcOffset::UTC);
    let weekday = t.weekday().number_days_from_monday() as i64;
    let secs_in_day = t.hour() as i64 * 3600 + t.minute() as i64 * 60 + t.second() as i64;
    let slots_per_day = 86_400 / step_secs;
    weekday * slots_per_day + secs_in_day / step_secs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Methodology, Region};

    /// Construit un historique au pas `step`, intensité donnée par `value(t)`,
    /// sur `count` points finissant juste avant `end`.
    fn history(
        end: OffsetDateTime,
        step: Duration,
        count: usize,
        value: impl Fn(OffsetDateTime) -> f64,
    ) -> Vec<Measurement> {
        (0..count)
            .map(|i| {
                let at = end - step * ((count - i) as i32);
                Measurement {
                    at,
                    region: Region::National,
                    intensity: CarbonIntensity::new(value(at)).unwrap(),
                    methodology: Methodology::rte_direct(),
                    vintage: Vintage::Tr,
                    mix: None,
                }
            })
            .collect()
    }

    /// Motif horaire : creux la nuit, pointe l'après-midi (mêmes valeurs chaque
    /// jour) — la base que la climatologie doit retrouver.
    fn hourly_pattern(t: OffsetDateTime) -> f64 {
        match t.hour() {
            0..=5 => 20.0,
            12..=17 => 80.0,
            _ => 50.0,
        }
    }

    #[test]
    fn empty_history_returns_none() {
        let from = OffsetDateTime::UNIX_EPOCH;
        assert!(
            climatology_forecast(&[], from, Duration::hours(24), ClimatologyParams::default())
                .is_none()
        );
    }

    #[test]
    fn invalid_params_return_none() {
        let from = OffsetDateTime::UNIX_EPOCH;
        let h = history(from, Duration::hours(1), 24, |_| 50.0);
        // Horizon nul.
        assert!(
            climatology_forecast(&h, from, Duration::ZERO, ClimatologyParams::default()).is_none()
        );
        // Pas nul.
        let bad = ClimatologyParams {
            step: Duration::ZERO,
            tau: Duration::hours(6),
        };
        assert!(climatology_forecast(&h, from, Duration::hours(24), bad).is_none());
    }

    #[test]
    fn count_matches_horizon_over_step() {
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let step = Duration::hours(1);
        let h = history(from, step, 14 * 24, hourly_pattern);
        let out = climatology_forecast(
            &h,
            from,
            Duration::hours(24),
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        )
        .unwrap();
        assert_eq!(out.len(), 24);
    }

    #[test]
    fn forecast_reproduces_weekly_pattern() {
        // Deux semaines d'historique au motif horaire constant : l'ancre coïncide
        // avec sa climatologie (biais ≈ 0), donc la prévision = le motif.
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let step = Duration::hours(1);
        let h = history(from, step, 14 * 24, hourly_pattern);
        let out = climatology_forecast(
            &h,
            from,
            Duration::hours(24),
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        )
        .unwrap();

        // Un point de nuit doit être nettement sous un point d'après-midi.
        let night = out
            .iter()
            .find(|m| m.at.hour() == 3)
            .unwrap()
            .intensity
            .value();
        let day = out
            .iter()
            .find(|m| m.at.hour() == 14)
            .unwrap()
            .intensity
            .value();
        assert!((night - 20.0).abs() < 1.0, "nuit = {night}");
        assert!((day - 80.0).abs() < 1.0, "jour = {day}");
        assert!(night < day);
    }

    #[test]
    fn persistence_correction_decays_with_horizon() {
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(90);
        let step = Duration::hours(1);
        let tau = Duration::hours(6);
        // Historique au motif horaire, puis on remplace la dernière observation
        // (l'ancre) par une valeur anormalement haute → biais positif.
        let mut h = history(from, step, 14 * 24, hourly_pattern);
        let anchor = h.last_mut().unwrap();
        let anchor_climo = hourly_pattern(anchor.at);
        anchor.intensity = CarbonIntensity::new(anchor_climo + 150.0).unwrap();

        let out = climatology_forecast(
            &h,
            from,
            Duration::hours(24),
            ClimatologyParams { step, tau },
        )
        .unwrap();

        // Près de l'ancre : tiré vers le haut par le biais ; loin : revenu à la
        // climatologie.
        let near = &out[1]; // ~2 h après l'ancre
        let far = &out[18]; // ~19 h après l'ancre
        let near_excess = near.intensity.value() - hourly_pattern(near.at);
        let far_excess = far.intensity.value() - hourly_pattern(far.at);
        assert!(near_excess > 50.0, "près : excès = {near_excess}");
        assert!(far_excess < 5.0, "loin : excès = {far_excess}");
        assert!(near_excess > far_excess);
    }

    #[test]
    fn unseen_slot_falls_back_to_overall_mean() {
        // Historique d'une seule valeur constante : tout créneau non observé
        // retombe sur la moyenne globale (= cette valeur), biais nul.
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(120);
        let step = Duration::hours(1);
        // 6 h d'historique seulement : la plupart des créneaux de la semaine sont
        // inconnus.
        let h = history(from, step, 6, |_| 42.0);
        let out = climatology_forecast(
            &h,
            from,
            Duration::hours(48),
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        )
        .unwrap();
        // Un point bien au-delà des créneaux observés vaut la moyenne globale.
        let late = out.last().unwrap().intensity.value();
        assert!((late - 42.0).abs() < 1e-9, "repli moyenne globale = {late}");
    }

    #[test]
    fn forecast_points_carry_region_methodology_and_no_mix() {
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(150);
        let step = Duration::minutes(15);
        let h = history(from, step, 96, |_| 30.0);
        let out = climatology_forecast(
            &h,
            from,
            Duration::hours(1),
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        )
        .unwrap();
        assert_eq!(out.len(), 4);
        for m in &out {
            assert_eq!(m.region, Region::National);
            assert_eq!(m.methodology, Methodology::rte_direct());
            assert_eq!(m.vintage, Vintage::Tr);
            assert!(m.mix.is_none());
        }
        // Premier point aligné sur `from`, pas régulier.
        assert_eq!(out[0].at, from);
        assert_eq!(out[1].at, from + step);
    }

    #[test]
    fn never_negative() {
        // Climatologie basse + biais fortement négatif : la prévision est bornée
        // à 0 (invariant CarbonIntensity), jamais négative.
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(200);
        let step = Duration::hours(1);
        let mut h = history(from, step, 48, |_| 5.0);
        h.last_mut().unwrap().intensity = CarbonIntensity::new(0.0).unwrap();
        let out = climatology_forecast(
            &h,
            from,
            Duration::hours(6),
            ClimatologyParams {
                step,
                tau: Duration::hours(6),
            },
        )
        .unwrap();
        assert!(out.iter().all(|m| m.intensity.value() >= 0.0));
    }
}
