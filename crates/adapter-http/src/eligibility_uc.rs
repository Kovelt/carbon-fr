//! Assemblage de l'overlay d'éligibilité (ADR-0025/0026, part prévue ADR-0028).
//!
//! Fonction d'orchestration **côté adapter** (le `core` reste intact) : elle
//! relie la prévision d'intensité (`points`), le mix nowcast/historique et le
//! prix spot (via [`EligibilityRepo`](crate::EligibilityRepo)) au calcul **pur**
//! de `carbonfr-eligibility`.
//!
//! Choix assumés :
//! - **Part renouvelable** : **observée** pour les créneaux couverts par la
//!   dernière mesure (`at ≤ now_at`, à moins d'un pas — intervalle dégénéré) ;
//!   pour un créneau passé plus ancien (`from` rétrospectif), la part observée
//!   **au créneau** est relue dans le batch d'historique — jamais la part
//!   courante (audit 2026-08 : les deux piliers d'un même verdict doivent être
//!   évalués au même horodatage) ; **prévue** au-delà du nowcast via
//!   `share-clim@1` (ADR-0028) **seulement si** le modèle est câblé (bandes
//!   calibrées au démarrage) — sinon `None` → `Indeterminate`, comme avant.
//!   L'historique de mix est lu en aller-retour **batch** (un seul, deux si
//!   `from` précède la fenêtre climatologique — jamais un par créneau, même
//!   discipline que le prix, audit F05) et **seulement** pour `rfnbo` ; la
//!   fenêtre climatologique est de plus **mise en cache** (ancre nowcast +
//!   TTL au cycle du poller, audit perf 2026-08) — elle n'est relue que quand
//!   la donnée a pu changer.
//! - **Prix = day-ahead frais** (PIÈGE 2) : la fraîcheur est filtrée par
//!   l'implémentation de `spot_price_at` ; au-delà du day-ahead, `None`.

use std::sync::{Arc, Mutex};

use carbonfr_core::domain::{
    ClimatologyParams, ForecastPoint, HorizonBands, TimeRange, WindowEstimator,
};
use carbonfr_eligibility::{
    EligibilityRuleset, EligibilityVerdict, FR_BIDDING_ZONE, ShareClimatology, ShareEstimate,
    SlotInput, evaluate, renewable_share,
};
use time::{Duration, OffsetDateTime};

/// Couverture temporelle de la dernière mesure nationale (cadence quart
/// d'heure éCO2mix) : elle ne vaut « nowcast » que pour les créneaux à moins
/// d'un pas d'elle — la relecture historique applique la même tolérance.
/// Constante (et non `ShareForecastConfig::params.step`) : le modèle share est
/// optionnel alors que la borne doit toujours s'appliquer (audit 2026-08).
const NOWCAST_COVERAGE: Duration = Duration::minutes(15);

/// Borne de fraîcheur du prix spot : **pas de prix au-delà du day-ahead**
/// (ADR-0026 PIÈGE 2). STRICTE : un prix horaire couvre `[t, t + 1 h)` — à
/// `t + 1 h` exactement, c'est l'heure de livraison suivante (audit 2026-08).
/// Partagée entre `spot_price_at` (`EligibilityRepoAdapter`, lib.rs) et
/// [`freshest_price`] : une seule définition de « frais » pour le pilier prix.
pub(crate) const MAX_PRICE_AGE: Duration = Duration::hours(1);

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
    /// Cache de la fenêtre climatologique (audit perf 2026-08) : sans lui,
    /// chaque requête `?eligibility=rfnbo` relisait ~70 j de mix national et
    /// rebâtissait la [`ShareClimatology`], pour une donnée qui ne change qu'au
    /// cycle du poller. Partagé entre clones (`Arc`), invalidé par déplacement
    /// de l'ancre nowcast ou expiration du TTL.
    cache: ShareHistoryCache,
}

impl ShareForecastConfig {
    /// Construit la configuration (composition root). `cache_ttl` = intervalle
    /// de poll (`CARBONFR_POLL_SECS`) : au-delà, un upsert qui ne déplace pas
    /// l'ancre (révision de millésime, backfill) a pu changer la fenêtre.
    pub fn new(
        bands: HorizonBands,
        params: ClimatologyParams,
        lookback: Duration,
        max_horizon: Duration,
        cache_ttl: std::time::Duration,
    ) -> Self {
        Self {
            bands,
            params,
            lookback,
            max_horizon,
            cache: ShareHistoryCache::new(cache_ttl),
        }
    }
}

/// Cache mono-entrée du [`ShareHistory`] de fenêtre climatologique, clé =
/// **ancre nowcast exacte** (une nouvelle mesure du poller déplace l'ancre et
/// invalide naturellement l'entrée), TTL en ceinture de sécurité.
#[derive(Debug, Clone)]
struct ShareHistoryCache {
    ttl: std::time::Duration,
    entry: Arc<Mutex<Option<ShareCacheEntry>>>,
}

#[derive(Debug)]
struct ShareCacheEntry {
    anchor: OffsetDateTime,
    stored_at: std::time::Instant,
    base: Arc<ShareHistory>,
}

impl ShareHistoryCache {
    fn new(ttl: std::time::Duration) -> Self {
        Self {
            ttl,
            entry: Arc::new(Mutex::new(None)),
        }
    }

    /// Historique de base (fenêtre `[anchor − lookback, anchor]`) : servi du
    /// cache si l'ancre est identique et le TTL non écoulé, sinon relu en base
    /// et rebâti. `None` si la fenêtre est invalide.
    async fn get_or_build(
        &self,
        repo: &dyn crate::EligibilityRepo,
        anchor: OffsetDateTime,
        lookback: Duration,
        step: Duration,
    ) -> Option<Arc<ShareHistory>> {
        {
            let guard = self.entry.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(e) = guard.as_ref()
                && e.anchor == anchor
                && e.stored_at.elapsed() < self.ttl
            {
                return Some(e.base.clone());
            }
        }
        let range = TimeRange::new(anchor - lookback, anchor + Duration::minutes(1))?;
        let mut history = repo.national_mix_range(range).await;
        if !history.is_sorted_by_key(|m| m.at) {
            history.sort_by_key(|m| m.at);
        }
        let base = Arc::new(ShareHistory {
            climatology: ShareClimatology::build(&history, step),
            observed: history
                .iter()
                .filter_map(|m| Some((m.at, renewable_share(m.mix.as_ref()?)?)))
                .collect(),
        });
        let mut guard = self.entry.lock().unwrap_or_else(|p| p.into_inner());
        *guard = Some(ShareCacheEntry {
            anchor,
            stored_at: std::time::Instant::now(),
            base: base.clone(),
        });
        Some(base)
    }
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

    // ADR-0028 : historique national en batch (jamais de N+1), seulement pour
    // un cadre qui consomme la part (rfnbo) avec un modèle câblé. Il nourrit la
    // climatologie (créneaux futurs) ET la part observée AU créneau (créneaux
    // passés d'un `from` rétrospectif — audit 2026-08). L'ancre de persistance
    // est le nowcast déjà lu ci-dessus (repli : dernière observation de la
    // fenêtre).
    let share_history = match (share, ruleset.framework) {
        (Some(cfg), carbonfr_eligibility::EligibilityFramework::Rfnbo) => {
            fetch_share_history(repo, cfg, now_at, points).await
        }
        _ => None,
    };
    let share_model = match (share, &share_history) {
        (Some(cfg), Some(h)) => h.climatology().map(|c| (c, cfg)),
        _ => None,
    };
    let anchor = match (now_at, now_share) {
        (Some(t), Some(s)) => Some((t, s)),
        _ => None,
    };

    let mut slots = Vec::with_capacity(points.len());
    for p in points {
        let spot = freshest_price(&prices, p.at);
        // Part observée au nowcast (dégénérée) — restreinte aux créneaux que la
        // dernière mesure couvre réellement (≤ un pas). Un créneau passé plus
        // ancien (`from` rétrospectif) relit sa PROPRE part observée dans le
        // batch d'historique — jamais la part courante (audit 2026-08). Au-delà
        // du nowcast : part prévue (share-clim@1, bornée à l'horizon calibré) ;
        // sinon None → Indeterminate, avec la CAUSE (hors horizon calibré vs
        // donnée manquante — F12).
        let renewable = match now_at {
            Some(t) if p.at <= t && t - p.at <= NOWCAST_COVERAGE => {
                now_share.map(ShareEstimate::observed)
            }
            Some(t) if p.at <= t => share_history
                .as_ref()
                .and_then(|h| h.observed_share_at(p.at))
                .map(ShareEstimate::observed),
            _ => share_model.and_then(|(climo, cfg)| {
                climo.forecast_at(p.at, anchor, cfg.params.tau, &cfg.bands, cfg.max_horizon)
            }),
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

/// Historique national de la **fenêtre climatologique** (partie mise en
/// cache) : climatologie de part (créneaux futurs) + série observée
/// `(at, part)` triée par horodatage croissant.
#[derive(Debug)]
struct ShareHistory {
    climatology: Option<ShareClimatology>,
    observed: Vec<(OffsetDateTime, f64)>,
}

/// Vue servie à l'évaluation : fenêtre de base (cache, `Arc`) + éventuelle
/// extension rétro (`from` avant la fenêtre climatologique), **jamais mise en
/// cache** — elle dépend des créneaux demandés — et strictement antérieure à
/// la fenêtre de base.
struct ShareHistoryView {
    base: Arc<ShareHistory>,
    extra_observed: Vec<(OffsetDateTime, f64)>,
}

impl ShareHistoryView {
    /// Climatologie de la fenêtre de base (le `lookback` est un paramètre du
    /// modèle, ADR-0028 : l'extension rétro ne la nourrit jamais).
    fn climatology(&self) -> Option<&ShareClimatology> {
        self.base.climatology.as_ref()
    }

    /// Part observée AU créneau `at` : la mesure la plus récente telle que
    /// `m.at ≤ at` et `at − m.at ≤` un pas (même couverture que le nowcast).
    /// `None` sinon → `Indeterminate` (donnée manquante), jamais la part d'un
    /// autre horodatage. L'extension étant antérieure à la fenêtre de base, la
    /// consulter en repli équivaut à chercher dans la concaténation.
    fn observed_share_at(&self, at: OffsetDateTime) -> Option<f64> {
        observed_at(&self.base.observed, at).or_else(|| observed_at(&self.extra_observed, at))
    }
}

/// Dernier échantillon `(t, part)` de la série triée tel que `t ≤ at`, dans la
/// limite de couverture d'un pas (même tolérance que le nowcast).
fn observed_at(observed: &[(OffsetDateTime, f64)], at: OffsetDateTime) -> Option<f64> {
    let idx = observed.partition_point(|&(t, _)| t <= at);
    let &(t, share) = observed.get(idx.checked_sub(1)?)?;
    (at - t <= NOWCAST_COVERAGE).then_some(share)
}

/// Charge l'historique de mix national en **batch** : fenêtre climatologique
/// `[ancre − lookback, ancre + 1 min)` (servie du cache de `cfg`, audit perf
/// 2026-08), plus — quand `from` la précède — un second batch borné à l'étendue
/// des créneaux demandés (≤ horizon, jamais une extension d'un seul tenant
/// jusqu'à `from` : un `from` très ancien chargerait des années d'historique).
/// `None` sans nowcast (aucune ancre de fenêtre) — la part restera
/// `Indeterminate`.
async fn fetch_share_history(
    repo: &dyn crate::EligibilityRepo,
    cfg: &ShareForecastConfig,
    now_at: Option<OffsetDateTime>,
    points: &[ForecastPoint],
) -> Option<ShareHistoryView> {
    let end = now_at?;
    let climatology_start = end - cfg.lookback;
    let base = cfg
        .cache
        .get_or_build(repo, end, cfg.lookback, cfg.params.step)
        .await?;

    // `from` rétrospectif plus ancien que la fenêtre climatologique : les
    // créneaux concernés (préfixe des `points`, triés) relisent leur part dans
    // un batch supplémentaire couvrant exactement leur étendue.
    let mut extra_observed = Vec::new();
    if let Some(first) = points.first()
        && first.at < climatology_start
    {
        let below_end = points
            .iter()
            .map(|p| p.at)
            .take_while(|&at| at < climatology_start)
            .last()
            .unwrap_or(first.at);
        if let Some(extra) = TimeRange::new(
            first.at - NOWCAST_COVERAGE,
            (below_end + Duration::minutes(1)).min(climatology_start),
        ) {
            let mut batch = repo.national_mix_range(extra).await;
            if !batch.is_sorted_by_key(|m| m.at) {
                batch.sort_by_key(|m| m.at);
            }
            extra_observed = batch
                .iter()
                .filter_map(|m| Some((m.at, renewable_share(m.mix.as_ref()?)?)))
                .collect();
        }
    }
    Some(ShareHistoryView {
        base,
        extra_observed,
    })
}

/// Un **seul** aller-retour prix couvrant tous les créneaux. La borne basse
/// `premier − MAX_PRICE_AGE` capture un prix légèrement antérieur au premier
/// créneau, dans la limite de fraîcheur appliquée par [`freshest_price`].
async fn fetch_prices_once(
    repo: &dyn crate::EligibilityRepo,
    points: &[ForecastPoint],
) -> Vec<(OffsetDateTime, f64)> {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Vec::new();
    };
    match TimeRange::new(first.at - MAX_PRICE_AGE, last.at + Duration::minutes(1)) {
        Some(range) => repo.spot_prices_range(range).await,
        None => Vec::new(),
    }
}

/// Prix day-ahead **frais** au créneau `at` : le plus récent tel que
/// `price.at ≤ at` et `at − price.at < MAX_PRICE_AGE` (borne STRICTE portée
/// par [`MAX_PRICE_AGE`] : jamais le prix précédent reconduit — pas
/// d'extrapolation au-delà du day-ahead, PIÈGE 2). Même sémantique que
/// `spot_price_at`, appliquée en mémoire sur la série déjà chargée. `prices`
/// triés par horodatage croissant.
fn freshest_price(prices: &[(OffsetDateTime, f64)], at: OffsetDateTime) -> Option<f64> {
    prices
        .iter()
        .rev()
        .find(|(t, _)| {
            let age = at - *t;
            age >= Duration::ZERO && age < MAX_PRICE_AGE
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
        share_config_with_ttl(std::time::Duration::from_secs(900))
    }

    fn share_config_with_ttl(ttl: std::time::Duration) -> ShareForecastConfig {
        ShareForecastConfig::new(
            flat_bands(),
            ClimatologyParams {
                step: Duration::minutes(15),
                tau: Duration::days(14),
            },
            Duration::days(21),
            Duration::hours(72),
            ttl,
        )
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

    // ---- `from` rétrospectif (audit 2026-08) --------------------------------

    /// Signal `renewable-share` d'un verdict.
    fn share_signal(v: &EligibilityVerdict) -> carbonfr_eligibility::EligibilitySignal {
        v.signals
            .iter()
            .find(|s| s.pillar() == carbonfr_eligibility::Pillar::RenewableShare)
            .copied()
            .expect("signal renewable-share")
    }

    /// Audit 2026-08 : un `from` passé ne sert JAMAIS la part courante sur les
    /// créneaux passés — chaque créneau relit sa PROPRE part observée dans le
    /// batch d'historique, et la branche nowcast reste bornée à un pas de la
    /// dernière mesure.
    #[tokio::test]
    async fn past_from_serves_observed_share_of_each_slot_never_current() {
        let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        // Historique à 0,25 partout… sauf à t0 − 2 h : 100 % renouvelable.
        let special = t0 - Duration::hours(2);
        let mut history = quarter_history(t0, 21);
        for m in &mut history {
            if m.at == special {
                m.mix = Some(renewable_mix());
            }
        }
        let repo = FakeRepo {
            latest: Some(measurement(t0, quarter_mix())),
            price: None,
            history,
        };
        // Fenêtre rétrospective : t0 − 2 h et t0 − 1 h (passés), t0 (nowcast).
        let points = [
            point(special, 20.0),
            point(t0 - Duration::hours(1), 20.0),
            point(t0, 20.0),
        ];
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let verdicts = evaluate_eligibility(
            &repo,
            &points,
            &r,
            WindowEstimator::Central,
            Some(&share_config()),
        )
        .await;

        // t0 − 2 h : la part observée DE CE créneau (1,0 — pas les 0,25 du
        // nowcast), provenance `observed`, verdict ferme.
        let s = share_signal(&verdicts[0]);
        assert_eq!(s.provenance(), Some("observed"));
        assert_eq!(s.value(), Some(1.0));
        assert_eq!(s.passed(), Some(true));
        // t0 − 1 h : la part observée du créneau (0,25), pas celle du nowcast.
        let s = share_signal(&verdicts[1]);
        assert_eq!(s.provenance(), Some("observed"));
        assert_eq!(s.passed(), Some(false));
        assert!((s.value().unwrap() - 0.25).abs() < 1e-9);
        // t0 : nowcast (à ≤ un pas de la dernière mesure) → part courante.
        let s = share_signal(&verdicts[2]);
        assert_eq!(s.provenance(), Some("observed"));
        assert!((s.value().unwrap() - 0.25).abs() < 1e-9);
    }

    /// Audit 2026-08 : sans modèle share câblé, un créneau passé plus ancien
    /// qu'un pas n'hérite plus de la part courante — `Indeterminate` (donnée
    /// manquante), jamais un verdict ferme à un autre horodatage.
    #[tokio::test]
    async fn past_slot_without_wired_model_is_indeterminate_not_current_share() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let repo = FakeRepo {
            latest: Some(measurement(t0, renewable_mix())), // part courante = 1,0
            price: None,
            ..Default::default()
        };
        let points = [point(t0 - Duration::hours(3), 20.0)];
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let verdicts =
            evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central, None).await;
        // Avant correctif : part 1,0 `observed` → éligible FERME sur un créneau
        // vieux de 3 h. Désormais : indéterminé.
        assert!(!verdicts[0].eligible);
        assert!(verdicts[0].is_indeterminate());
        assert_eq!(share_signal(&verdicts[0]).passed(), None);
    }

    /// Audit 2026-08 : quand `from` précède la fenêtre climatologique, un
    /// second batch borné à l'étendue des créneaux est chargé — la part
    /// observée du créneau reste servie (jamais celle du nowcast).
    #[tokio::test]
    async fn past_from_beyond_lookback_reads_bounded_extra_batch() {
        let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let old = t0 - Duration::days(30); // bien avant le lookback (21 j)
        let mut history = quarter_history(t0, 21);
        history.push(measurement(old, renewable_mix()));
        let repo = FakeRepo {
            latest: Some(measurement(t0, quarter_mix())),
            price: None,
            history,
        };
        let points = [point(old, 20.0)];
        let r = EligibilityRuleset::rfnbo_2023_1184();
        let verdicts = evaluate_eligibility(
            &repo,
            &points,
            &r,
            WindowEstimator::Central,
            Some(&share_config()),
        )
        .await;
        let s = share_signal(&verdicts[0]);
        assert_eq!(s.provenance(), Some("observed"));
        assert_eq!(s.value(), Some(1.0));
    }

    /// Audit 2026-08 : la fraîcheur prix est STRICTE — un prix horaire couvre
    /// `[t, t + 1 h)` ; à `t + 1 h` exactement, c'est l'heure de livraison
    /// suivante (jamais le prix précédent reconduit).
    #[test]
    fn freshest_price_rejects_exactly_one_hour_old_price() {
        let t = OffsetDateTime::UNIX_EPOCH;
        let prices = vec![(t, 10.0)];
        assert_eq!(
            freshest_price(&prices, t + Duration::minutes(59)),
            Some(10.0)
        );
        assert_eq!(freshest_price(&prices, t + Duration::hours(1)), None);
        assert_eq!(freshest_price(&prices, t - Duration::minutes(1)), None);
    }

    // ---- cache de la fenêtre climatologique (audit perf 2026-08) ------------

    /// Repo espion : historique réel + compteur de batchs + ancre déplaçable.
    struct CachingSpyRepo {
        latest: Mutex<Option<Measurement>>,
        history: Vec<Measurement>,
        mix_calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl crate::EligibilityRepo for CachingSpyRepo {
        async fn latest_national_mix(&self) -> Option<Measurement> {
            self.latest.lock().unwrap().clone()
        }
        async fn spot_price_at(&self, _at: OffsetDateTime) -> Option<f64> {
            None
        }
        async fn spot_prices_range(&self, _range: TimeRange) -> Vec<(OffsetDateTime, f64)> {
            Vec::new()
        }
        async fn national_mix_range(&self, range: TimeRange) -> Vec<Measurement> {
            self.mix_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.history
                .iter()
                .filter(|m| range.contains(m.at))
                .cloned()
                .collect()
        }
    }

    /// Audit perf 2026-08 : deux évaluations successives (même ancre nowcast)
    /// partagent la fenêtre climatologique en cache — un seul batch de mix,
    /// verdicts identiques.
    #[tokio::test]
    async fn share_history_cached_across_evaluations() {
        let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let repo = CachingSpyRepo {
            latest: Mutex::new(Some(measurement(t0, quarter_mix()))),
            history: quarter_history(t0, 21),
            mix_calls: std::sync::atomic::AtomicUsize::new(0),
        };
        let cfg = share_config();
        let points = [point(t0 + Duration::hours(6), 20.0)];
        let r = EligibilityRuleset::rfnbo_2023_1184();

        let first =
            evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central, Some(&cfg)).await;
        let second =
            evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central, Some(&cfg)).await;

        assert_eq!(
            repo.mix_calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "la fenêtre climatologique ne doit être relue qu'une fois"
        );
        // Parité de comportement : la part prévue servie du cache est la même.
        assert_eq!(
            share_signal(&first[0]).value(),
            share_signal(&second[0]).value()
        );
        assert_eq!(share_signal(&second[0]).provenance(), Some("forecast"));
    }

    /// Audit perf 2026-08 : TTL écoulé → la fenêtre est relue (un upsert qui ne
    /// déplace pas l'ancre — millésime, backfill — a pu la changer).
    #[tokio::test]
    async fn share_history_cache_expires_with_ttl() {
        let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let repo = CachingSpyRepo {
            latest: Mutex::new(Some(measurement(t0, quarter_mix()))),
            history: quarter_history(t0, 21),
            mix_calls: std::sync::atomic::AtomicUsize::new(0),
        };
        let cfg = share_config_with_ttl(std::time::Duration::ZERO);
        let points = [point(t0 + Duration::hours(6), 20.0)];
        let r = EligibilityRuleset::rfnbo_2023_1184();

        for _ in 0..2 {
            let _ = evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central, Some(&cfg))
                .await;
        }
        assert_eq!(
            repo.mix_calls.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "TTL nul → chaque évaluation relit la fenêtre"
        );
    }

    /// Audit perf 2026-08 : une nouvelle mesure du poller déplace l'ancre
    /// nowcast et invalide le cache — la fenêtre suit la donnée fraîche.
    #[tokio::test]
    async fn share_history_cache_invalidated_when_anchor_moves() {
        let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(30);
        let repo = CachingSpyRepo {
            latest: Mutex::new(Some(measurement(t0, quarter_mix()))),
            history: quarter_history(t0, 21),
            mix_calls: std::sync::atomic::AtomicUsize::new(0),
        };
        let cfg = share_config();
        let points = [point(t0 + Duration::hours(6), 20.0)];
        let r = EligibilityRuleset::rfnbo_2023_1184();

        let _ =
            evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central, Some(&cfg)).await;
        // Nouvelle mesure (ancre + 15 min) : le cache doit être invalidé.
        *repo.latest.lock().unwrap() = Some(measurement(t0 + Duration::minutes(15), quarter_mix()));
        let _ =
            evaluate_eligibility(&repo, &points, &r, WindowEstimator::Central, Some(&cfg)).await;
        assert_eq!(
            repo.mix_calls.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "ancre déplacée → fenêtre relue"
        );
    }
}
