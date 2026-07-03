//! Assemblage de l'overlay d'éligibilité (ADR-0025/0026, part prévue ADR-0028).
//!
//! Fonction d'orchestration **côté adapter** (le `core` reste intact) : elle
//! relie la prévision d'intensité (`points`), le mix nowcast/historique et le
//! prix spot (via [`EligibilityRepo`](crate::EligibilityRepo)) au calcul **pur**
//! de `carbonfr-eligibility`.
//!
//! Choix assumés :
//! - **Part renouvelable** : **observée** pour les créneaux `at ≤ now_at`
//!   (intervalle dégénéré, comportement historique) ; **prévue** au-delà via
//!   `share-clim@1` (ADR-0028) **seulement si** le modèle est câblé (bandes
//!   calibrées au démarrage) — sinon `None` → `Indeterminate`, comme avant.
//!   L'historique de mix est lu en **un seul** aller-retour batch (même
//!   discipline que le prix, audit F05) et **seulement** pour `rfnbo`.
//! - **Prix = day-ahead frais** (PIÈGE 2) : la fraîcheur est filtrée par
//!   l'implémentation de `spot_price_at` ; au-delà du day-ahead, `None`.

use carbonfr_core::domain::{
    ClimatologyParams, ForecastPoint, HorizonBands, TimeRange, WindowEstimator,
};
use carbonfr_eligibility::{
    EligibilityRuleset, EligibilityVerdict, FR_BIDDING_ZONE, ShareClimatology, ShareEstimate,
    SlotInput, evaluate, renewable_share,
};
use time::{Duration, OffsetDateTime};

/// Configuration du modèle `share-clim@1` (ADR-0028), câblée par la composition
/// root quand les bandes ont pu être calibrées au démarrage. Absente → la part
/// renouvelable future reste `None` (comportement d'avant ADR-0028).
#[derive(Debug, Clone)]
pub struct ShareForecastConfig {
    /// Bandes de résidus par horizon, calibrées par walk-forward au démarrage.
    pub bands: HorizonBands,
    /// Pas + τ du modèle (mêmes défauts que `climatology@1`).
    pub params: ClimatologyParams,
    /// Profondeur d'historique de la climatologie (fenêtre glissante).
    pub lookback: Duration,
    /// Horizon **calibré** au-delà duquel on ne prévoit jamais (discipline du
    /// prix day-ahead : au-delà, `Indeterminate`).
    pub max_horizon: Duration,
}

/// Évalue l'éligibilité de chaque créneau prévu, en enrichissant `points` du mix
/// (observé puis prévu) et du prix spot. **Best-effort** : une donnée absente
/// devient `Indeterminate`, jamais une erreur.
pub(crate) async fn evaluate_eligibility(
    repo: &dyn crate::EligibilityRepo,
    points: &[ForecastPoint],
    ruleset: &EligibilityRuleset,
    estimator: WindowEstimator,
    share: Option<&ShareForecastConfig>,
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

    // ADR-0028 : climatologie de part renouvelable pour les créneaux futurs.
    // Un seul batch d'historique (jamais de N+1), et seulement pour un cadre qui
    // consomme la part (rfnbo) avec un modèle câblé. L'ancre de persistance est
    // le nowcast déjà lu ci-dessus (repli : dernière observation de la fenêtre).
    let share_model = match (share, ruleset.framework) {
        (Some(cfg), carbonfr_eligibility::EligibilityFramework::Rfnbo) => {
            build_share_climatology(repo, cfg, now_at)
                .await
                .map(|c| (c, cfg))
        }
        _ => None,
    };
    let anchor = match (now_at, now_share) {
        (Some(t), Some(s)) => Some((t, s)),
        _ => None,
    };

    let mut slots = Vec::with_capacity(points.len());
    for p in points {
        let spot = freshest_price(&prices, p.at);
        let is_nowcast = now_at.map(|t| p.at <= t).unwrap_or(false);
        // Part observée au nowcast (dégénérée) ; prévue au-delà (share-clim@1,
        // bornée à l'horizon calibré) ; sinon None → Indeterminate, avec la
        // CAUSE (hors horizon calibré vs donnée manquante — F12).
        let renewable = if is_nowcast {
            now_share.map(ShareEstimate::observed)
        } else {
            share_model.as_ref().and_then(|(climo, cfg)| {
                climo.forecast_at(p.at, anchor, cfg.params.tau, &cfg.bands, cfg.max_horizon)
            })
        };
        let renewable_share_gap = match (&renewable, share, anchor) {
            (Some(_), _, _) => carbonfr_eligibility::IndeterminateReason::MissingData,
            // Modèle câblé + ancre connue mais créneau au-delà de l'horizon
            // calibré : la cause est l'horizon, pas la donnée.
            (None, Some(cfg), Some((t0, _))) if p.at - t0 > cfg.max_horizon => {
                carbonfr_eligibility::IndeterminateReason::BeyondCalibratedHorizon
            }
            _ => carbonfr_eligibility::IndeterminateReason::MissingData,
        };
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
            renewable_share_gap,
            spot_price_eur_mwh: spot,
        });
    }

    evaluate(&slots, ruleset, FR_BIDDING_ZONE)
}

/// Construit la climatologie de part renouvelable depuis **un seul** batch
/// d'historique national (`[ancre − lookback, ancre)`). `None` si l'historique
/// est vide/inexploitable — la part future restera `Indeterminate`.
async fn build_share_climatology(
    repo: &dyn crate::EligibilityRepo,
    cfg: &ShareForecastConfig,
    now_at: Option<OffsetDateTime>,
) -> Option<ShareClimatology> {
    // Sans nowcast, on ancre la fenêtre sur l'horloge du dernier point connu ;
    // à défaut de toute référence, on ne prévoit pas.
    let end = now_at?;
    let range = TimeRange::new(end - cfg.lookback, end + Duration::minutes(1))?;
    let history = repo.national_mix_range(range).await;
    ShareClimatology::build(&history, cfg.params.step)
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

    /// Fake d'`EligibilityRepo` : mix nowcast optionnel + prix par créneau +
    /// historique de mix (pour `share-clim@1`).
    #[derive(Default)]
    struct FakeRepo {
        latest: Option<Measurement>,
        price: Option<f64>,
        history: Vec<Measurement>,
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
        async fn national_mix_range(&self, range: TimeRange) -> Vec<Measurement> {
            self.history
                .iter()
                .filter(|m| range.contains(m.at))
                .cloned()
                .collect()
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
            ..Default::default()
        };
        // p0 = nowcast (≤ now), p1 = futur (> now).
        let points = [point(t0, 20.0), point(t0 + Duration::hours(1), 20.0)];
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let verdicts =
            evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central, None).await;

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
            ..Default::default()
        };
        let points = [point(t0, 30.0), point(t0 + Duration::hours(1), 120.0)];
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let verdicts =
            evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central, None).await;
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
            ..Default::default()
        };
        let points = [point(t0, 50.0)]; // expected 50, upper 53
        let r = EligibilityRuleset::low_carbon_2025_2359();
        let central =
            evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central, None).await;
        let prudent =
            evaluate_eligibility(&repo, &points, &r, WindowEstimator::Prudent, None).await;
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
            ..Default::default()
        };
        let points = [point(t0 + Duration::hours(5), 20.0)]; // futur (pas de part renouvelable)
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let verdicts =
            evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central, None).await;
        assert!(verdicts[0].eligible); // surplus prix suffit
    }

    /// F05 : le prix ne doit être lu qu'une fois (batch), et pas du tout pour un
    /// cadre sans pilier prix — plus jamais une requête par créneau.
    #[tokio::test]
    async fn low_carbon_makes_no_price_query_rfnbo_batches_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountingRepo {
            calls: AtomicUsize,
            mix_calls: AtomicUsize,
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
            async fn national_mix_range(&self, _range: TimeRange) -> Vec<Measurement> {
                self.mix_calls.fetch_add(1, Ordering::SeqCst);
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
            mix_calls: AtomicUsize::new(0),
        };
        let _ = evaluate_eligibility(
            &lc,
            &points,
            &EligibilityRuleset::low_carbon_2025_2359(),
            WindowEstimator::Central,
            None,
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
            mix_calls: AtomicUsize::new(0),
        };
        let _ = evaluate_eligibility(
            &rf,
            &points,
            &EligibilityRuleset::rfnbo_2023_1184(),
            WindowEstimator::Central,
            None,
        )
        .await;
        assert_eq!(
            rf.calls.load(Ordering::SeqCst),
            1,
            "rfnbo doit faire un seul batch prix, pas une requête par créneau"
        );
    }

    // ---- share-clim@1 (ADR-0028) -------------------------------------------

    /// Bandes dégénérées (résidus vides) couvrant 72 h au pas 15 min.
    fn flat_bands() -> carbonfr_core::domain::HorizonBands {
        carbonfr_core::domain::HorizonBands::from_residuals(
            Duration::minutes(15),
            &vec![Vec::new(); 289],
            0.1,
        )
    }

    fn share_config() -> ShareForecastConfig {
        ShareForecastConfig {
            bands: flat_bands(),
            params: ClimatologyParams {
                step: Duration::minutes(15),
                tau: Duration::days(14),
            },
            lookback: Duration::days(21),
            max_horizon: Duration::hours(72),
        }
    }

    /// Mix 25 % renouvelable (250 MW EnR / 1000 MW total).
    fn quarter_mix() -> GenerationMix {
        GenerationMix {
            nucleaire: 750.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 0.0,
            eolien: 250.0,
            solaire: 0.0,
            bioenergies: 0.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        }
    }

    /// Historique de mix constant (part 0,25) sur `days` jours avant `end`.
    fn quarter_history(end: OffsetDateTime, days: i64) -> Vec<Measurement> {
        (1..=days * 96)
            .map(|i| measurement(end - Duration::minutes(15) * i as i32, quarter_mix()))
            .collect()
    }

    #[tokio::test]
    async fn future_slots_get_forecast_share_when_wired() {
        let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let repo = FakeRepo {
            latest: Some(measurement(t0, quarter_mix())),
            price: None,
            history: quarter_history(t0, 21),
        };
        let points = [point(t0 + Duration::hours(6), 20.0)];
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let verdicts = evaluate_eligibility(
            &repo,
            &points,
            &r,
            WindowEstimator::Central,
            Some(&share_config()),
        )
        .await;

        // Part prévue ~0,25 : verdict FERME `fail` (upper < 0,90) au lieu
        // d'`Indeterminate`, avec provenance `forecast`.
        let signal = verdicts[0]
            .signals
            .iter()
            .find(|s| s.pillar() == carbonfr_eligibility::Pillar::RenewableShare)
            .copied()
            .expect("signal renewable-share");
        assert_eq!(signal.passed(), Some(false));
        assert_eq!(signal.provenance(), Some("forecast"));
        assert!((signal.value().unwrap() - 0.25).abs() < 0.02);
    }

    #[tokio::test]
    async fn forecast_share_not_served_beyond_max_horizon() {
        let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let repo = FakeRepo {
            latest: Some(measurement(t0, quarter_mix())),
            price: None,
            history: quarter_history(t0, 21),
        };
        // Au-delà de l'horizon calibré (72 h) : jamais d'extrapolation.
        let points = [point(t0 + Duration::hours(80), 20.0)];
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let verdicts = evaluate_eligibility(
            &repo,
            &points,
            &r,
            WindowEstimator::Central,
            Some(&share_config()),
        )
        .await;
        let signal = verdicts[0]
            .signals
            .iter()
            .find(|s| s.pillar() == carbonfr_eligibility::Pillar::RenewableShare)
            .copied()
            .expect("signal renewable-share");
        assert_eq!(signal.passed(), None, "au-delà du calibré : indéterminé");
    }

    /// L'historique de mix est lu en UN SEUL batch, et seulement pour `rfnbo`
    /// avec un modèle câblé (jamais pour low-carbon, jamais sans config).
    #[tokio::test]
    async fn mix_history_batched_once_for_rfnbo_only() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct MixCountingRepo {
            latest: Option<Measurement>,
            mix_calls: AtomicUsize,
        }
        #[async_trait::async_trait]
        impl crate::EligibilityRepo for MixCountingRepo {
            async fn latest_national_mix(&self) -> Option<Measurement> {
                self.latest.clone()
            }
            async fn spot_price_at(&self, _at: OffsetDateTime) -> Option<f64> {
                None
            }
            async fn spot_prices_range(&self, _range: TimeRange) -> Vec<(OffsetDateTime, f64)> {
                Vec::new()
            }
            async fn national_mix_range(&self, _range: TimeRange) -> Vec<Measurement> {
                self.mix_calls.fetch_add(1, Ordering::SeqCst);
                Vec::new()
            }
        }

        let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let cfg = share_config();
        let points: Vec<ForecastPoint> = (1i64..=8)
            .map(|i| point(t0 + Duration::hours(i), 50.0))
            .collect();

        // rfnbo + config : un seul batch, quel que soit le nombre de créneaux.
        let repo = MixCountingRepo {
            latest: Some(measurement(t0, quarter_mix())),
            mix_calls: AtomicUsize::new(0),
        };
        let _ = evaluate_eligibility(
            &repo,
            &points,
            &EligibilityRuleset::rfnbo_2023_1184(),
            WindowEstimator::Central,
            Some(&cfg),
        )
        .await;
        assert_eq!(repo.mix_calls.load(Ordering::SeqCst), 1);

        // low-carbon + config : la part n'est pas consommée → zéro lecture.
        let repo = MixCountingRepo {
            latest: Some(measurement(t0, quarter_mix())),
            mix_calls: AtomicUsize::new(0),
        };
        let _ = evaluate_eligibility(
            &repo,
            &points,
            &EligibilityRuleset::low_carbon_2025_2359(),
            WindowEstimator::Central,
            Some(&cfg),
        )
        .await;
        assert_eq!(repo.mix_calls.load(Ordering::SeqCst), 0);

        // rfnbo sans config : comportement d'avant ADR-0028, zéro lecture.
        let repo = MixCountingRepo {
            latest: Some(measurement(t0, quarter_mix())),
            mix_calls: AtomicUsize::new(0),
        };
        let _ = evaluate_eligibility(
            &repo,
            &points,
            &EligibilityRuleset::rfnbo_2023_1184(),
            WindowEstimator::Central,
            None,
        )
        .await;
        assert_eq!(repo.mix_calls.load(Ordering::SeqCst), 0);
    }
}
