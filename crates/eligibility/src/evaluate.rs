//! Évaluation **pure** de l'éligibilité d'une série de créneaux.
//!
//! Stratégie par cadre (ADR-0025/0026), niveau **réseau** :
//!
//! - **`rfnbo`** — *disjonction* des exceptions réseau : un créneau est éligible
//!   si la part renouvelable instantanée ≥ seuil **OU** le prix day-ahead ≤ seuil
//!   de surplus. (L'Article 4 légal est une moyenne annuelle de zone, ≈ jamais
//!   atteinte en FR ; l'additionnalité PPA est au niveau site, hors périmètre.)
//! - **`low-carbon`** — *condition nécessaire* : l'intensité doit être ≤ seuil
//!   dérivé. La règle exploite l'intervalle de confiance (ADR-0011) : éligible si
//!   `upper ≤ seuil`, non éligible si `lower > seuil`, sinon **indéterminé**.
//!
//! `eligible` n'est `true` que si l'éligibilité est **certaine**. Une donnée
//! manquante donne un signal `Indeterminate` — jamais une extrapolation.

use crate::ruleset::{EligibilityFramework, EligibilityRuleset};
use crate::verdict::{EligibilitySignal, EligibilityVerdict, Pillar, SlotInput};

/// Évalue une série de créneaux contre un ruleset, pour une zone de dépôt.
pub fn evaluate(
    slots: &[SlotInput],
    ruleset: &EligibilityRuleset,
    bidding_zone: &'static str,
) -> Vec<EligibilityVerdict> {
    slots
        .iter()
        .map(|slot| evaluate_slot(slot, ruleset, bidding_zone))
        .collect()
}

/// Évalue un créneau unique.
pub fn evaluate_slot(
    slot: &SlotInput,
    ruleset: &EligibilityRuleset,
    bidding_zone: &'static str,
) -> EligibilityVerdict {
    let signals = match ruleset.framework {
        EligibilityFramework::Rfnbo => rfnbo_signals(slot, ruleset),
        EligibilityFramework::LowCarbon => low_carbon_signals(slot, ruleset),
    };
    // Éligible (certain) dès qu'un pilier passe — disjonction pour `rfnbo`,
    // pilier unique pour `low-carbon`.
    let eligible = signals.iter().any(|s| s.passed() == Some(true));
    EligibilityVerdict {
        timestamp: slot.at,
        bidding_zone,
        framework: ruleset.framework,
        ruleset_version: ruleset.version,
        eligible,
        signals,
        carbon_intensity: slot.intensity,
        intensity_lower: slot.intensity_lower,
        intensity_upper: slot.intensity_upper,
        score: score(slot, ruleset),
    }
}

fn rfnbo_signals(slot: &SlotInput, ruleset: &EligibilityRuleset) -> Vec<EligibilitySignal> {
    let mut signals = Vec::with_capacity(2);

    // Part renouvelable instantanée (proxy ; cf. doc du ruleset).
    match slot.renewable_share {
        Some(share) => {
            let threshold = ruleset.article4_renewable_threshold;
            signals.push(EligibilitySignal::RenewableShare {
                share,
                threshold,
                passed: share >= threshold,
            });
        }
        None => signals.push(EligibilitySignal::Indeterminate {
            pillar: Pillar::RenewableShare,
        }),
    }

    // Surplus prix (pilier actif seulement si le ruleset porte un seuil).
    // L'exception surplus de l'art. 4 est une DISJONCTION : prix ≤ seuil OU
    // prix < 0,36×prix EUA. Seule la branche prix est câblée (pas de flux EUA).
    // Donc on ne peut émettre qu'un PASS certain (prix ≤ seuil) ; un prix au-dessus
    // ne prouve PAS l'échec de l'exception (la branche EUA reste possible) → on
    // émet `Indeterminate`, jamais un échec ferme (sur-affirmerait un négatif).
    match (ruleset.surplus_price_eur_mwh, slot.spot_price_eur_mwh) {
        (Some(threshold), Some(price)) if price <= threshold => {
            signals.push(EligibilitySignal::SurplusPrice {
                spot_price_eur_mwh: price,
                threshold,
                passed: true,
            })
        }
        (Some(_), _) => signals.push(EligibilitySignal::Indeterminate {
            pillar: Pillar::SurplusPrice,
        }),
        (None, _) => {}
    }

    signals
}

fn low_carbon_signals(slot: &SlotInput, ruleset: &EligibilityRuleset) -> Vec<EligibilitySignal> {
    let Some(threshold) = ruleset.low_carbon_intensity_threshold_g_per_kwh else {
        return vec![EligibilitySignal::Indeterminate {
            pillar: Pillar::LowCarbonIntensity,
        }];
    };

    let indicative = ruleset.low_carbon_intensity_is_indicative;
    let lower = slot.intensity_lower.value();
    let upper = slot.intensity_upper.value();
    let reported = slot.intensity.value();

    // Règle d'intervalle (ADR-0011) : on ne tranche que hors recouvrement du seuil.
    let signal = if upper <= threshold {
        EligibilitySignal::LowCarbonIntensity {
            intensity_g_per_kwh: reported,
            threshold,
            indicative,
            passed: true,
        }
    } else if lower > threshold {
        EligibilitySignal::LowCarbonIntensity {
            intensity_g_per_kwh: reported,
            threshold,
            indicative,
            passed: false,
        }
    } else {
        EligibilitySignal::Indeterminate {
            pillar: Pillar::LowCarbonIntensity,
        }
    };
    vec![signal]
}

/// Score de classement (plus bas = meilleur), toujours fini.
///
/// `low-carbon` : l'intensité elle-même (homogène avec `greenest-window`).
/// `rfnbo` : favorise une forte part renouvelable et un prix bas ; une donnée
/// manquante reçoit une **forte** pénalité (tend à reléguer les créneaux
/// incertains). C'est une heuristique de **classement**, pas un ordre total
/// garanti entre certain et incertain ; le seul score consommé par l'overlay est
/// `best_eligible`, qui ne classe que des créneaux `eligible`.
fn score(slot: &SlotInput, ruleset: &EligibilityRuleset) -> f64 {
    match ruleset.framework {
        EligibilityFramework::LowCarbon => slot.intensity.value(),
        EligibilityFramework::Rfnbo => {
            let renew_gap = slot
                .renewable_share
                .map(|r| (1.0 - r).max(0.0))
                .unwrap_or(1.5);
            let price_term = slot.spot_price_eur_mwh.map(|p| p.max(0.0)).unwrap_or(200.0);
            renew_gap * 100.0 + price_term * 0.5 + slot.intensity.value() * 1e-3
        }
    }
}

/// Trie les verdicts par score croissant (`NaN` en queue, par sécurité).
pub fn rank_by_score(mut verdicts: Vec<EligibilityVerdict>) -> Vec<EligibilityVerdict> {
    verdicts.sort_by(|a, b| match (a.score.is_nan(), b.score.is_nan()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => a
            .score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal),
    });
    verdicts
}

/// Meilleur créneau **éligible** (score le plus bas), emprunté — sans consommer
/// ni cloner la série.
pub fn best_eligible(verdicts: &[EligibilityVerdict]) -> Option<&EligibilityVerdict> {
    verdicts
        .iter()
        .filter(|v| v.eligible && !v.score.is_nan())
        .min_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ruleset::EligibilityRuleset;
    use carbonfr_core::domain::CarbonIntensity;
    use time::OffsetDateTime;

    fn ci(g: f64) -> CarbonIntensity {
        CarbonIntensity::new(g).expect("intensité de test")
    }

    /// Créneau avec bande symétrique de demi-largeur `half`.
    fn slot(g: f64, half: f64, share: Option<f64>, price: Option<f64>) -> SlotInput {
        SlotInput {
            at: OffsetDateTime::UNIX_EPOCH,
            intensity: ci(g),
            intensity_lower: ci((g - half).max(0.0)),
            intensity_upper: ci(g + half),
            renewable_share: share,
            spot_price_eur_mwh: price,
        }
    }

    // ---- low-carbon -------------------------------------------------------

    #[test]
    fn low_carbon_eligible_below_threshold() {
        let r = EligibilityRuleset::low_carbon_2025_2359(); // seuil 64
        let v = evaluate_slot(&slot(30.0, 2.0, None, None), &r, "FR");
        assert!(v.eligible);
        assert!(!v.is_indeterminate());
    }

    #[test]
    fn low_carbon_ineligible_above_threshold() {
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let v = evaluate_slot(&slot(120.0, 2.0, None, None), &r, "FR");
        assert!(!v.eligible);
        assert!(!v.is_indeterminate()); // certain : lower > seuil
    }

    #[test]
    fn low_carbon_servable_without_mix_or_price() {
        // low-carbon n'a besoin ni du mix ni du prix : éligible sur tout l'horizon.
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let v = evaluate_slot(&slot(40.0, 1.0, None, None), &r, "FR");
        assert!(v.eligible);
    }

    #[test]
    fn low_carbon_indeterminate_when_threshold_within_interval() {
        let r = EligibilityRuleset::low_carbon_2025_2359(); // seuil 64
        // bande [54, 74] traverse 64 → indéterminé.
        let v = evaluate_slot(&slot(64.0, 10.0, None, None), &r, "FR");
        assert!(!v.eligible);
        assert!(v.is_indeterminate());
    }

    #[test]
    fn low_carbon_signal_is_marked_indicative() {
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let v = evaluate_slot(&slot(30.0, 2.0, None, None), &r, "FR");
        let marked = v.signals.iter().any(|s| {
            matches!(
                s,
                EligibilitySignal::LowCarbonIntensity {
                    indicative: true,
                    ..
                }
            )
        });
        assert!(marked);
    }

    #[test]
    fn low_carbon_has_no_price_pillar() {
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let v = evaluate_slot(&slot(30.0, 2.0, Some(0.9), Some(5.0)), &r, "FR");
        assert!(
            !v.signals
                .iter()
                .any(|s| matches!(s, EligibilitySignal::SurplusPrice { .. }))
        );
    }

    // ---- rfnbo ------------------------------------------------------------

    #[test]
    fn rfnbo_eligible_when_renewable_share_above_threshold() {
        let r = EligibilityRuleset::rfnbo_2023_1184(); // seuil 0,90
        let v = evaluate_slot(&slot(20.0, 2.0, Some(0.95), Some(60.0)), &r, "FR");
        assert!(v.eligible); // part ≥ 0,90 suffit (disjonction)
    }

    #[test]
    fn rfnbo_eligible_when_price_below_surplus() {
        let r = EligibilityRuleset::rfnbo_2023_1184(); // surplus 20 €/MWh
        let v = evaluate_slot(&slot(20.0, 2.0, Some(0.30), Some(10.0)), &r, "FR");
        assert!(v.eligible); // prix ≤ 20 suffit (disjonction), même part faible
    }

    #[test]
    fn rfnbo_indeterminate_when_renewable_fails_and_price_above_surplus() {
        // Part renouvelable connue < seuil (échec) ET prix > seuil : la branche EUA
        // (non câblée) pourrait encore valider le surplus → INDÉTERMINÉ, pas un
        // « certain non-éligible » (on ne sur-affirme jamais un négatif, finding 5).
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let v = evaluate_slot(&slot(20.0, 2.0, Some(0.30), Some(80.0)), &r, "FR");
        assert!(!v.eligible);
        assert!(v.is_indeterminate());
        // Le pilier prix n'émet jamais d'échec ferme (seulement pass ou indéterminé).
        assert!(
            !v.signals
                .iter()
                .any(|s| matches!(s, EligibilitySignal::SurplusPrice { passed: false, .. }))
        );
    }

    #[test]
    fn rfnbo_indeterminate_when_price_missing_and_share_fails() {
        // part < seuil (échec connu) mais prix inconnu : pourrait passer via surplus.
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let v = evaluate_slot(&slot(20.0, 2.0, Some(0.30), None), &r, "FR");
        assert!(!v.eligible);
        assert!(v.is_indeterminate());
    }

    #[test]
    fn rfnbo_indeterminate_when_mix_absent_and_price_high() {
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let v = evaluate_slot(&slot(20.0, 2.0, None, Some(80.0)), &r, "FR");
        assert!(!v.eligible);
        assert!(v.is_indeterminate()); // part inconnue → pourrait basculer
    }

    // ---- score / classement ----------------------------------------------

    #[test]
    fn low_carbon_score_equals_intensity() {
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let v = evaluate_slot(&slot(42.0, 1.0, None, None), &r, "FR");
        assert_eq!(v.score, 42.0);
    }

    #[test]
    fn rfnbo_score_penalises_missing_data() {
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let known = evaluate_slot(&slot(20.0, 1.0, Some(0.5), Some(30.0)), &r, "FR");
        let missing = evaluate_slot(&slot(20.0, 1.0, None, None), &r, "FR");
        assert!(missing.score > known.score);
        assert!(known.score.is_finite() && missing.score.is_finite());
    }

    #[test]
    fn best_eligible_borrows_without_consuming() {
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let verdicts = vec![
            evaluate_slot(&slot(50.0, 1.0, None, None), &r, "FR"),
            evaluate_slot(&slot(20.0, 1.0, None, None), &r, "FR"),
            evaluate_slot(&slot(120.0, 1.0, None, None), &r, "FR"), // non éligible
        ];
        let best = best_eligible(&verdicts).expect("au moins un éligible");
        assert_eq!(best.score, 20.0);
        // `verdicts` toujours utilisable (emprunté, pas consommé).
        assert_eq!(verdicts.len(), 3);
    }

    #[test]
    fn rank_by_score_orders_lowest_first() {
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let verdicts = vec![
            evaluate_slot(&slot(50.0, 1.0, None, None), &r, "FR"),
            evaluate_slot(&slot(20.0, 1.0, None, None), &r, "FR"),
            evaluate_slot(&slot(90.0, 1.0, None, None), &r, "FR"),
        ];
        let ranked = rank_by_score(verdicts);
        let scores: Vec<f64> = ranked.iter().map(|v| v.score).collect();
        assert_eq!(scores, [20.0, 50.0, 90.0]);
    }

    #[test]
    fn bidding_zone_is_fr_not_region() {
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let v = evaluate_slot(
            &slot(20.0, 1.0, Some(0.95), None),
            &r,
            crate::FR_BIDDING_ZONE,
        );
        assert_eq!(v.bidding_zone, "FR");
    }

    #[test]
    fn evaluate_maps_every_slot() {
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let slots = [slot(30.0, 1.0, None, None), slot(120.0, 1.0, None, None)];
        let verdicts = evaluate(&slots, &r, "FR");
        assert_eq!(verdicts.len(), 2);
        assert!(verdicts[0].eligible);
        assert!(!verdicts[1].eligible);
    }
}
