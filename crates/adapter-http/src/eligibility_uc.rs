//! Assemblage de l'overlay d'éligibilité (ADR-0025/0026).
//!
//! Fonction d'orchestration **côté adapter** (le `core` reste intact) : elle
//! relie la prévision d'intensité (`points`), le mix nowcast et le prix spot
//! (via [`EligibilityRepo`](crate::EligibilityRepo)) au calcul **pur** de
//! `carbonfr-eligibility`.
//!
//! Choix assumés :
//! - **Part renouvelable = nowcast/historique uniquement** (D4) : `ForecastPoint`
//!   ne porte pas le mix → on n'attribue la part renouvelable observée qu'aux
//!   créneaux `at ≤ now_at`. Au-delà : `None` → signal `Indeterminate`. (Le mix
//!   prévu — `MixForecast` — est une évolution réservée, ADR-0026.)
//! - **Prix = day-ahead frais** (PIÈGE 2) : la fraîcheur est filtrée par
//!   l'implémentation de `spot_price_at` ; au-delà du day-ahead, `None`.

use carbonfr_core::domain::{ForecastPoint, TimeRange, WindowEstimator};
use carbonfr_eligibility::{
    EligibilityRuleset, EligibilityVerdict, FR_BIDDING_ZONE, SlotInput, evaluate, renewable_share,
};
use time::{Duration, OffsetDateTime};

/// Évalue l'éligibilité de chaque créneau prévu, en enrichissant `points` du mix
/// nowcast et du prix spot. **Best-effort** : une donnée absente devient
/// `Indeterminate`, jamais une erreur.
pub(crate) async fn evaluate_eligibility(
    repo: &dyn crate::EligibilityRepo,
    points: &[ForecastPoint],
    ruleset: &EligibilityRuleset,
    estimator: WindowEstimator,
) -> Vec<EligibilityVerdict> {
    // Mix nowcast NATIONAL (ancre rte-direct) → part renouvelable + borne de fraîcheur.
    let latest = repo.latest_national_mix().await;
    let now_at = latest.as_ref().map(|m| m.at);
    let now_share = latest
        .as_ref()
        .and_then(|m| m.mix.as_ref())
        .and_then(renewable_share);

    // F05 : le pilier prix n'existe que pour les cadres qui portent un seuil de
    // surplus (rfnbo) ; pour `low-carbon` (`surplus_price_eur_mwh = None`), on ne
    // requête **aucun** prix. Quand il en faut, on fait **un seul** aller-retour
    // couvrant tous les créneaux (au lieu d'une requête par créneau — jusqu'à 288
    // sur un horizon 72 h au pas 15 min) ; la fraîcheur est filtrée en mémoire.
    let prices = if ruleset.surplus_price_eur_mwh.is_some() {
        fetch_prices_once(repo, points).await
    } else {
        Vec::new()
    };

    let mut slots = Vec::with_capacity(points.len());
    for p in points {
        let spot = freshest_price(&prices, p.at);
        // D4 : la part renouvelable observée ne vaut que pour le nowcast/historique.
        let is_nowcast = now_at.map(|t| p.at <= t).unwrap_or(false);
        let renewable = if is_nowcast { now_share } else { None };
        let intensity = match estimator {
            WindowEstimator::Central => p.expected,
            WindowEstimator::Prudent => p.upper,
        };
        slots.push(SlotInput {
            at: p.at,
            intensity,
            intensity_lower: p.lower,
            intensity_upper: p.upper,
            renewable_share: renewable,
            spot_price_eur_mwh: spot,
        });
    }

    evaluate(&slots, ruleset, FR_BIDDING_ZONE)
}

/// Un **seul** aller-retour prix couvrant tous les créneaux. La borne basse
/// `premier − 1 h` capture un prix légèrement antérieur au premier créneau, dans
/// la limite de fraîcheur appliquée par [`freshest_price`].
async fn fetch_prices_once(
    repo: &dyn crate::EligibilityRepo,
    points: &[ForecastPoint],
) -> Vec<(OffsetDateTime, f64)> {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Vec::new();
    };
    match TimeRange::new(
        first.at - Duration::hours(1),
        last.at + Duration::minutes(1),
    ) {
        Some(range) => repo.spot_prices_range(range).await,
        None => Vec::new(),
    }
}

/// Prix day-ahead **frais** au créneau `at` : le plus récent tel que
/// `price.at ≤ at` et `at − price.at ≤ 1 h` (pas d'extrapolation au-delà du
/// day-ahead, PIÈGE 2). Même sémantique que `spot_price_at`, appliquée en mémoire
/// sur la série déjà chargée. `prices` triés par horodatage croissant.
fn freshest_price(prices: &[(OffsetDateTime, f64)], at: OffsetDateTime) -> Option<f64> {
    prices
        .iter()
        .rev()
        .find(|(t, _)| {
            let age = at - *t;
            age >= Duration::ZERO && age <= Duration::hours(1)
        })
        .map(|(_, eur)| *eur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbonfr_core::domain::{
        CarbonIntensity, GenerationMix, Measurement, Methodology, ModelVersion, Region, Vintage,
    };
    use carbonfr_eligibility::EligibilityRuleset;
    use time::{Duration, OffsetDateTime};

    /// Fake d'`EligibilityRepo` : mix nowcast optionnel + prix par créneau.
    struct FakeRepo {
        latest: Option<Measurement>,
        price: Option<f64>,
    }

    #[async_trait::async_trait]
    impl crate::EligibilityRepo for FakeRepo {
        async fn latest_national_mix(&self) -> Option<Measurement> {
            self.latest.clone()
        }
        async fn spot_price_at(&self, _at: OffsetDateTime) -> Option<f64> {
            self.price
        }
        async fn spot_prices_range(&self, range: TimeRange) -> Vec<(OffsetDateTime, f64)> {
            // Prix horaires constants sur l'intervalle (tri croissant) : chaque
            // créneau trouve ainsi un prix frais (≤ 1 h) via `freshest_price`.
            let Some(eur) = self.price else {
                return Vec::new();
            };
            let mut out = Vec::new();
            let mut t = range.start();
            while t <= range.end() {
                out.push((t, eur));
                t += Duration::hours(1);
            }
            out
        }
    }

    fn ci(g: f64) -> CarbonIntensity {
        CarbonIntensity::new(g).expect("intensité")
    }

    fn point(at: OffsetDateTime, g: f64) -> ForecastPoint {
        ForecastPoint::new(
            at,
            Region::National,
            ci(g),
            ci((g - 3.0).max(0.0)),
            ci(g + 3.0),
            Methodology::rte_direct(),
            ModelVersion::new("climatology", 1),
        )
    }

    fn renewable_mix() -> GenerationMix {
        GenerationMix {
            nucleaire: 0.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 100.0,
            eolien: 100.0,
            solaire: 0.0,
            bioenergies: 0.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        }
    }

    fn measurement(at: OffsetDateTime, mix: GenerationMix) -> Measurement {
        Measurement {
            at,
            region: Region::National,
            intensity: ci(30.0),
            methodology: Methodology::rte_direct(),
            vintage: Vintage::Tr,
            mix: Some(mix),
        }
    }

    #[tokio::test]
    async fn nowcast_fills_renewable_share_future_leaves_none() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        // Mix observé "maintenant" = t0 ; part renouvelable = 1,0 (100% EnR).
        let repo = FakeRepo {
            latest: Some(measurement(t0, renewable_mix())),
            price: None,
        };
        // p0 = nowcast (≤ now), p1 = futur (> now).
        let points = [point(t0, 20.0), point(t0 + Duration::hours(1), 20.0)];
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let verdicts = evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central).await;

        // p0 : part renouvelable connue (1,0 ≥ 0,90) → éligible.
        assert!(verdicts[0].eligible);
        // p1 : part renouvelable None + prix None → indéterminé (jamais extrapolé).
        assert!(!verdicts[1].eligible);
        assert!(verdicts[1].is_indeterminate());
    }

    #[tokio::test]
    async fn missing_repo_data_never_errors_low_carbon_uses_intensity() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let repo = FakeRepo {
            latest: None,
            price: None,
        };
        let points = [point(t0, 30.0), point(t0 + Duration::hours(1), 120.0)];
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let verdicts = evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central).await;
        // low-carbon n'a besoin ni du mix ni du prix.
        assert!(verdicts[0].eligible); // 30 ≤ 64
        assert!(!verdicts[1].eligible); // 120 > 64
    }

    #[tokio::test]
    async fn prudent_estimator_uses_upper_bound_for_reported_intensity() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let repo = FakeRepo {
            latest: None,
            price: None,
        };
        let points = [point(t0, 50.0)]; // expected 50, upper 53
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let central = evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central).await;
        let prudent = evaluate_eligibility(&repo, &points, &r, WindowEstimator::Prudent).await;
        assert_eq!(central[0].carbon_intensity.value(), 50.0);
        assert_eq!(prudent[0].carbon_intensity.value(), 53.0);
    }

    #[tokio::test]
    async fn rfnbo_surplus_price_passes_when_cheap() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        // Prix bas (10 ≤ 20) sur tous les créneaux → éligible même sans mix futur.
        let repo = FakeRepo {
            latest: Some(measurement(t0, renewable_mix())),
            price: Some(10.0),
        };
        let points = [point(t0 + Duration::hours(5), 20.0)]; // futur (pas de part renouvelable)
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let verdicts = evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central).await;
        assert!(verdicts[0].eligible); // surplus prix suffit
    }

    /// F05 : le prix ne doit être lu qu'une fois (batch), et pas du tout pour un
    /// cadre sans pilier prix — plus jamais une requête par créneau.
    #[tokio::test]
    async fn low_carbon_makes_no_price_query_rfnbo_batches_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountingRepo {
            calls: AtomicUsize,
        }
        #[async_trait::async_trait]
        impl crate::EligibilityRepo for CountingRepo {
            async fn latest_national_mix(&self) -> Option<Measurement> {
                None
            }
            async fn spot_price_at(&self, _at: OffsetDateTime) -> Option<f64> {
                None
            }
            async fn spot_prices_range(&self, _range: TimeRange) -> Vec<(OffsetDateTime, f64)> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Vec::new()
            }
        }

        let t0 = OffsetDateTime::UNIX_EPOCH;
        // 8 créneaux : l'ancien code aurait fait 8 requêtes prix séquentielles.
        let points: Vec<ForecastPoint> = (0i64..8)
            .map(|i| point(t0 + Duration::hours(i), 50.0))
            .collect();

        // low-carbon : aucun pilier prix → zéro requête.
        let lc = CountingRepo {
            calls: AtomicUsize::new(0),
        };
        let _ = evaluate_eligibility(
            &lc,
            &points,
            &EligibilityRuleset::low_carbon_2025_2359(),
            WindowEstimator::Central,
        )
        .await;
        assert_eq!(
            lc.calls.load(Ordering::SeqCst),
            0,
            "low-carbon ne doit requêter aucun prix"
        );

        // rfnbo : pilier prix → un SEUL aller-retour (batch), pas un par créneau.
        let rf = CountingRepo {
            calls: AtomicUsize::new(0),
        };
        let _ = evaluate_eligibility(
            &rf,
            &points,
            &EligibilityRuleset::rfnbo_2023_1184(),
            WindowEstimator::Central,
        )
        .await;
        assert_eq!(
            rf.calls.load(Ordering::SeqCst),
            1,
            "rfnbo doit faire un seul batch prix, pas une requête par créneau"
        );
    }
}
