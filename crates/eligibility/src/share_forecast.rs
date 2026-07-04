//! Prévision de la **part renouvelable** du mix national (`share-clim@1`,
//! ADR-0028) — pour le pilier `RenewableShare` du cadre `rfnbo`.
//!
//! Même famille de modèle que `climatology@1` (ADR-0009) : climatologie
//! **horaire-de-semaine** de la part renouvelable observée + correction
//! d'anomalie décroissante ancrée sur le nowcast,
//!
//! ```text
//! part(t) = clamp₀₁( C(t) + (s₀ − C(t₀)) · exp(−|t − t₀| / τ) )
//! ```
//!
//! avec un **intervalle par quantiles de résidus par horizon**
//! ([`HorizonBands`], ADR-0011 §5) calibré par backtest *walk-forward*.
//!
//! Disciplines héritées (ADR-0026) :
//! - **jamais de point sans intervalle** : sans bandes calibrées, on ne prévoit
//!   pas ([`ShareClimatology::forecast_at`] exige des bandes) ;
//! - **jamais d'extrapolation au-delà du calibré** : au-delà de `max_horizon`,
//!   `None` → signal `Indeterminate` (même discipline que le prix day-ahead) ;
//! - la part **observée** reste servie telle quelle (intervalle dégénéré,
//!   [`ShareEstimate::observed`]) — le comportement nowcast est inchangé.
//!
//! Tout est **pur** : le backtest et la calibration prennent l'historique en
//! mémoire (`&[Measurement]`) — la vérité se dérive du même historique, aucun
//! port supplémentaire (motif `BacktestConsumptionForecast`, ADR-0013).

use std::collections::{BTreeMap, HashMap};

use carbonfr_core::domain::{
    ClimatologyParams, ErrorAccumulator, ErrorMetrics, HorizonBands, Measurement, TimeRange,
    week_slot,
};
use time::{Duration, OffsetDateTime};

use crate::share::renewable_share;

/// Identité versionnée du modèle (ADR-0019/0028) : jamais de changement
/// silencieux — une évolution de la formule = nouvelle version + ADR.
pub const SHARE_FORECAST_MODEL: &str = "share-clim@1";

/// Provenance d'une estimation de part renouvelable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareSource {
    /// Mix observé (nowcast/historique) — valeur mesurée, pas un modèle.
    Observed,
    /// Mix prévu (`share-clim@1`) — estimation, intervalle calibré.
    Forecast,
}

impl ShareSource {
    pub fn slug(self) -> &'static str {
        match self {
            ShareSource::Observed => "observed",
            ShareSource::Forecast => "forecast",
        }
    }
}

/// Part renouvelable d'un créneau, avec provenance et intervalle (ADR-0011).
///
/// Invariant : `0 ≤ lower ≤ expected ≤ upper ≤ 1`. Une part observée porte un
/// intervalle **dégénéré** (`lower == expected == upper`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShareEstimate {
    pub expected: f64,
    pub lower: f64,
    pub upper: f64,
    pub source: ShareSource,
}

impl ShareEstimate {
    /// Part **observée** (nowcast/historique) : intervalle dégénéré.
    pub fn observed(share: f64) -> Self {
        let s = share.clamp(0.0, 1.0);
        Self {
            expected: s,
            lower: s,
            upper: s,
            source: ShareSource::Observed,
        }
    }

    pub fn is_observed(&self) -> bool {
        self.source == ShareSource::Observed
    }
}

/// Climatologie horaire-de-semaine de la part renouvelable, construite sur un
/// historique de mix national (`rte-direct`, ADR-0026 décision 9).
#[derive(Debug, Clone)]
pub struct ShareClimatology {
    step_secs: i64,
    slot_means: HashMap<i64, f64>,
    overall_mean: f64,
    /// Dernière part observée de l'historique — ancre de repli quand le nowcast
    /// n'est pas disponible au moment de la prévision.
    anchor: (OffsetDateTime, f64),
}

impl ShareClimatology {
    /// Construit depuis un historique de mesures : les lignes sans mix ou sans
    /// part calculable sont ignorées. `None` si aucun échantillon exploitable.
    pub fn build(history: &[Measurement], step: Duration) -> Option<Self> {
        let samples: Vec<(OffsetDateTime, f64)> = history
            .iter()
            .filter_map(|m| {
                let mix = m.mix.as_ref()?;
                Some((m.at, renewable_share(mix)?))
            })
            .collect();
        Self::from_samples(&samples, step)
    }

    /// Construit depuis des échantillons `(horodatage, part)` déjà dérivés
    /// (variante bas niveau, utilisée par le backtest pour trancher l'historique
    /// par origine sans re-dériver la part).
    pub fn from_samples(samples: &[(OffsetDateTime, f64)], step: Duration) -> Option<Self> {
        let step_secs = step.whole_seconds();
        if step_secs <= 0 || samples.is_empty() {
            return None;
        }
        let mut sums: HashMap<i64, (f64, u32)> = HashMap::new();
        let mut total = 0.0;
        let mut anchor = samples[0];
        for &(at, share) in samples {
            let entry = sums.entry(week_slot(at, step_secs)).or_insert((0.0, 0));
            entry.0 += share;
            entry.1 += 1;
            total += share;
            if at > anchor.0 {
                anchor = (at, share);
            }
        }
        let slot_means = sums
            .into_iter()
            .map(|(slot, (sum, n))| (slot, sum / n as f64))
            .collect();
        Some(Self {
            step_secs,
            slot_means,
            overall_mean: total / samples.len() as f64,
            anchor,
        })
    }

    /// Moyenne climatologique du créneau de `t` (repli : moyenne globale).
    fn climatology_at(&self, t: OffsetDateTime) -> f64 {
        self.slot_means
            .get(&week_slot(t, self.step_secs))
            .copied()
            .unwrap_or(self.overall_mean)
    }

    /// Valeur attendue à `t` depuis l'ancre `(t₀, s₀)` (climatologie + biais de
    /// persistance décroissant), clampée à `[0, 1]`. Pur, sans intervalle —
    /// le chemin **servi** passe par [`Self::forecast_at`], qui exige des bandes.
    pub fn expected_at(
        &self,
        t: OffsetDateTime,
        anchor: (OffsetDateTime, f64),
        tau: Duration,
    ) -> f64 {
        let tau_secs = tau.whole_seconds() as f64;
        let (t0, s0) = anchor;
        let bias = s0.clamp(0.0, 1.0) - self.climatology_at(t0);
        let decay = if tau_secs > 0.0 {
            (-((t - t0).whole_seconds() as f64).abs() / tau_secs).exp()
        } else {
            0.0
        };
        (self.climatology_at(t) + bias * decay).clamp(0.0, 1.0)
    }

    /// Part **prévue** à `t`, ancrée sur `anchor` (le nowcast si disponible,
    /// sinon la dernière observation de l'historique).
    ///
    /// Retourne `None` (→ `Indeterminate`, jamais d'extrapolation) si :
    /// - `t` n'est pas strictement au-delà de l'ancre (le passé/présent est
    ///   **observé**, pas prévu) ;
    /// - le décalage dépasse `max_horizon` (borne de l'horizon **calibré**) ;
    /// - les bandes ne couvrent pas ce décalage.
    pub fn forecast_at(
        &self,
        t: OffsetDateTime,
        anchor: Option<(OffsetDateTime, f64)>,
        tau: Duration,
        bands: &HorizonBands,
        max_horizon: Duration,
    ) -> Option<ShareEstimate> {
        let anchor = anchor.unwrap_or(self.anchor);
        let dt = t - anchor.0;
        if dt <= Duration::ZERO || dt > max_horizon {
            return None;
        }
        let expected = self.expected_at(t, anchor, tau);
        let (q_low, q_high) = bands.at(dt)?;
        let lower = (expected + q_low).clamp(0.0, 1.0).min(expected);
        let upper = (expected + q_high).clamp(0.0, 1.0).max(expected);
        Some(ShareEstimate {
            expected,
            lower,
            upper,
            source: ShareSource::Forecast,
        })
    }
}

/// Erreur par horizon (modèle vs persistance), en points de part (0-1).
#[derive(Debug, Clone)]
pub struct ShareHorizonError {
    pub horizon: Duration,
    pub model: Option<ErrorMetrics>,
    pub persistence: Option<ErrorMetrics>,
}

/// Rapport du backtest *walk-forward* de `share-clim@1`.
#[derive(Debug, Clone)]
pub struct ShareBacktestReport {
    pub origins: usize,
    pub model: Option<ErrorMetrics>,
    pub persistence: Option<ErrorMetrics>,
    pub by_horizon: Vec<ShareHorizonError>,
    /// Verdicts fermes émis au seuil (mesurés seulement si des bandes sont
    /// fournies) : la sécurité du GATE exige `false_pass == 0`.
    pub firm_pass: usize,
    pub false_pass: usize,
    pub firm_fail: usize,
    pub false_fail: usize,
    pub straddle: usize,
}

impl ShareBacktestReport {
    /// Critère GO/NO-GO du GATE (ADR-0028) : le modèle bat la persistance en
    /// RMSE global **et** aucun faux `pass` ferme au seuil.
    pub fn passes_gate(&self) -> bool {
        let beats = match (&self.model, &self.persistence) {
            (Some(m), Some(p)) => m.rmse < p.rmse,
            _ => false,
        };
        beats && self.false_pass == 0
    }
}

/// Backtest *walk-forward* **pur** de la part renouvelable prévue.
///
/// `history` doit couvrir `[test.start − lookback, test.end + max(checkpoints))`.
/// À chaque origine (espacée d'`origin_step`, alignée sur la grille du pas), le
/// modèle ne voit que `[origine − lookback, origine)` (anti-fuite) ; la vérité et
/// la baseline de **persistance** (dernière part observée avant l'origine) sont
/// dérivées du même historique. Si `bands` est fourni, compte aussi les verdicts
/// fermes au `threshold` (faux `pass`/`fail`) — la métrique de sécurité du GATE.
#[allow(clippy::too_many_arguments)]
pub fn backtest_share(
    history: &[Measurement],
    test: TimeRange,
    lookback: Duration,
    origin_step: Duration,
    params: ClimatologyParams,
    checkpoints: &[Duration],
    bands: Option<&HorizonBands>,
    threshold: f64,
) -> Option<ShareBacktestReport> {
    let step_secs = params.step.whole_seconds();
    if step_secs <= 0 || origin_step <= Duration::ZERO || checkpoints.is_empty() {
        return None;
    }
    let samples = share_samples(history);
    if samples.is_empty() {
        return None;
    }
    let truth: BTreeMap<i64, f64> = samples
        .iter()
        .map(|&(at, s)| (at.unix_timestamp(), s))
        .collect();

    let mut model_acc = ErrorAccumulator::default();
    let mut persistence_acc = ErrorAccumulator::default();
    let mut by_horizon: Vec<(Duration, ErrorAccumulator, ErrorAccumulator)> = checkpoints
        .iter()
        .map(|&h| (h, ErrorAccumulator::default(), ErrorAccumulator::default()))
        .collect();
    let (mut firm_pass, mut false_pass, mut firm_fail, mut false_fail, mut straddle) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut origins = 0usize;

    let mut cursor = align_down(test.start(), step_secs);
    while cursor < test.end() {
        let origin = cursor;
        cursor += origin_step;

        // Fenêtre d'apprentissage strictement passée + ancre de persistance.
        let window_start = origin - lookback;
        let end_idx = samples.partition_point(|&(at, _)| at < origin);
        let start_idx = samples.partition_point(|&(at, _)| at < window_start);
        let window = &samples[start_idx..end_idx];
        let Some(climo) = ShareClimatology::from_samples(window, params.step) else {
            continue;
        };
        let anchor = window.last().copied();
        let Some(anchor) = anchor else { continue };
        origins += 1;

        for (h, model_h, persistence_h) in by_horizon.iter_mut() {
            let t = origin + *h;
            let Some(&observed) = truth.get(&t.unix_timestamp()) else {
                continue;
            };
            let expected = climo.expected_at(t, anchor, params.tau);
            model_acc.observe(expected, observed);
            model_h.observe(expected, observed);
            persistence_acc.observe(anchor.1, observed);
            persistence_h.observe(anchor.1, observed);

            if let Some((q_low, q_high)) = bands.and_then(|b| b.at(*h)) {
                let lower = (expected + q_low).clamp(0.0, 1.0).min(expected);
                let upper = (expected + q_high).clamp(0.0, 1.0).max(expected);
                if lower >= threshold {
                    firm_pass += 1;
                    if observed < threshold {
                        false_pass += 1;
                    }
                } else if upper < threshold {
                    firm_fail += 1;
                    if observed >= threshold {
                        false_fail += 1;
                    }
                } else {
                    straddle += 1;
                }
            }
        }
    }

    Some(ShareBacktestReport {
        origins,
        model: model_acc.metrics(),
        persistence: persistence_acc.metrics(),
        by_horizon: by_horizon
            .into_iter()
            .map(|(horizon, m, p)| ShareHorizonError {
                horizon,
                model: m.metrics(),
                persistence: p.metrics(),
            })
            .collect(),
        firm_pass,
        false_pass,
        firm_fail,
        false_fail,
        straddle,
    })
}

/// Calibre les bandes de résidus **par horizon** de `share-clim@1` (quantiles
/// `q`/`1−q` de `observé − prévu`, en points de part), par walk-forward pur sur
/// `calib` — même discipline anti-fuite que [`backtest_share`]. `None` si aucune
/// origine exploitable.
pub fn calibrate_share_bands(
    history: &[Measurement],
    calib: TimeRange,
    lookback: Duration,
    origin_step: Duration,
    params: ClimatologyParams,
    horizon: Duration,
    q: f64,
) -> Option<HorizonBands> {
    let step_secs = params.step.whole_seconds();
    if step_secs <= 0 || origin_step <= Duration::ZERO || horizon <= Duration::ZERO {
        return None;
    }
    let samples = share_samples(history);
    if samples.is_empty() {
        return None;
    }
    let truth: BTreeMap<i64, f64> = samples
        .iter()
        .map(|&(at, s)| (at.unix_timestamp(), s))
        .collect();
    let slots = (horizon.whole_seconds() / step_secs).max(1) as usize;
    let mut residuals: Vec<Vec<f64>> = vec![Vec::new(); slots];
    let mut any = false;

    let mut cursor = align_down(calib.start(), step_secs);
    while cursor < calib.end() {
        let origin = cursor;
        cursor += origin_step;

        let end_idx = samples.partition_point(|&(at, _)| at < origin);
        let start_idx = samples.partition_point(|&(at, _)| at < origin - lookback);
        let window = &samples[start_idx..end_idx];
        let Some(climo) = ShareClimatology::from_samples(window, params.step) else {
            continue;
        };
        let Some(anchor) = window.last().copied() else {
            continue;
        };

        for (idx, bucket) in residuals.iter_mut().enumerate().skip(1) {
            let t = origin + params.step * idx as i32;
            let Some(&observed) = truth.get(&t.unix_timestamp()) else {
                continue;
            };
            let expected = climo.expected_at(t, anchor, params.tau);
            bucket.push(observed - expected);
            any = true;
        }
    }
    if !any {
        return None;
    }

    fill_empty_buckets(&mut residuals);
    Some(HorizonBands::from_residuals(params.step, &residuals, q))
}

/// Comble les seaux de résidus vides par le seau non vide le plus proche.
///
/// Un seau vide donnerait une bande nulle `(0, 0)` — soit un intervalle
/// DÉGÉNÉRÉ sur une prévision : une fausse certitude. Arrive quand la grille
/// de la vérité est plus grossière que le pas des bandes (ex. jeu consolidé
/// 30 min vs pas 15 min : les index impairs ne trouvent jamais d'observation).
/// On comble par le seau non vide le plus proche (précédent d'abord — la
/// dispersion croît avec l'horizon, l'emprunt au précédent est conservateur
/// dans le mauvais sens le plus faible ; repli suivant pour les seaux de tête).
pub(crate) fn fill_empty_buckets(residuals: &mut [Vec<f64>]) {
    let filled: Vec<usize> = (0..residuals.len())
        .filter(|&i| !residuals[i].is_empty())
        .collect();
    for i in 0..residuals.len() {
        if residuals[i].is_empty() {
            let nearest = filled
                .iter()
                .copied()
                .min_by_key(|&j| (j.abs_diff(i), usize::from(j > i)))
                .map(|j| residuals[j].clone());
            if let Some(source) = nearest {
                residuals[i] = source;
            }
        }
    }
}

/// Dérive la série `(horodatage, part)` d'un historique de mesures, triée par
/// horodatage croissant.
pub(crate) fn share_samples(history: &[Measurement]) -> Vec<(OffsetDateTime, f64)> {
    let mut samples: Vec<(OffsetDateTime, f64)> = history
        .iter()
        .filter_map(|m| {
            let mix = m.mix.as_ref()?;
            Some((m.at, renewable_share(mix)?))
        })
        .collect();
    samples.sort_by_key(|&(at, _)| at);
    samples
}

/// Aligne vers le bas sur la grille du pas (multiples depuis l'epoch, comme le
/// backtest d'intensité du `core`).
fn align_down(at: OffsetDateTime, step_secs: i64) -> OffsetDateTime {
    let unix = at.unix_timestamp();
    let aligned = unix - unix.rem_euclid(step_secs);
    OffsetDateTime::from_unix_timestamp(aligned).unwrap_or(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbonfr_core::domain::{
        CarbonIntensity, GenerationMix, Measurement, Methodology, Region, Vintage,
    };

    /// 2024-01-01 00:00 UTC (un lundi — créneau de semaine déterministe).
    fn monday() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_704_067_200).unwrap()
    }

    fn mix(renewable_mw: f64, nuclear_mw: f64) -> GenerationMix {
        GenerationMix {
            nucleaire: nuclear_mw,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 0.0,
            eolien: renewable_mw,
            solaire: 0.0,
            bioenergies: 0.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        }
    }

    fn measurement(at: OffsetDateTime, renewable_mw: f64, nuclear_mw: f64) -> Measurement {
        Measurement {
            at,
            region: Region::National,
            intensity: CarbonIntensity::new(20.0).unwrap(),
            methodology: Methodology::rte_direct(),
            vintage: Vintage::Tr,
            mix: Some(mix(renewable_mw, nuclear_mw)),
        }
    }

    /// Historique synthétique : part constante 0,25 au pas 15 min.
    fn flat_history(from: OffsetDateTime, days: i64, share: f64) -> Vec<Measurement> {
        let steps = days * 96;
        (0..steps)
            .map(|i| {
                let at = from + Duration::minutes(15) * i as i32;
                measurement(at, share * 1000.0, (1.0 - share) * 1000.0)
            })
            .collect()
    }

    fn degenerate_bands() -> HorizonBands {
        // Bandes nulles sur 72 h au pas 15 min (288 index).
        HorizonBands::from_residuals(Duration::minutes(15), &vec![Vec::new(); 288], 0.1)
    }

    #[test]
    fn observed_estimate_is_degenerate_and_clamped() {
        let e = ShareEstimate::observed(1.4);
        assert_eq!((e.expected, e.lower, e.upper), (1.0, 1.0, 1.0));
        assert!(e.is_observed());
    }

    #[test]
    fn forecast_matches_climatology_on_flat_history() {
        let from = monday();
        let history = flat_history(from, 70, 0.25);
        let climo = ShareClimatology::build(&history, Duration::minutes(15)).unwrap();
        let anchor = (history.last().unwrap().at, 0.25);
        let bands = degenerate_bands();
        let est = climo
            .forecast_at(
                anchor.0 + Duration::hours(6),
                Some(anchor),
                Duration::days(14),
                &bands,
                Duration::hours(72),
            )
            .unwrap();
        assert!((est.expected - 0.25).abs() < 1e-9);
        assert_eq!(est.source, ShareSource::Forecast);
        assert!(est.lower <= est.expected && est.expected <= est.upper);
    }

    #[test]
    fn forecast_converges_to_anchor_near_nowcast() {
        // Anomalie au nowcast (part 0,50 sur une climatologie à 0,25) : à
        // horizon court, la prévision colle à l'ancre ; elle décroît ensuite.
        let from = monday();
        let history = flat_history(from, 70, 0.25);
        let climo = ShareClimatology::build(&history, Duration::minutes(15)).unwrap();
        let anchor = (history.last().unwrap().at, 0.50);
        let bands = degenerate_bands();
        let near = climo
            .forecast_at(
                anchor.0 + Duration::minutes(15),
                Some(anchor),
                Duration::days(14),
                &bands,
                Duration::hours(72),
            )
            .unwrap();
        assert!(
            (near.expected - 0.50).abs() < 0.01,
            "near={}",
            near.expected
        );
        let far = climo
            .forecast_at(
                anchor.0 + Duration::hours(48),
                Some(anchor),
                Duration::hours(6),
                &bands,
                Duration::hours(72),
            )
            .unwrap();
        assert!((far.expected - 0.25).abs() < 0.01, "far={}", far.expected);
    }

    #[test]
    fn forecast_is_clamped_to_unit_interval() {
        let from = monday();
        let history = flat_history(from, 70, 0.95);
        let climo = ShareClimatology::build(&history, Duration::minutes(15)).unwrap();
        // Ancre absurde au-dessus de 1 : clampée, jamais > 1.
        let anchor = (history.last().unwrap().at, 5.0);
        let bands = degenerate_bands();
        let est = climo
            .forecast_at(
                anchor.0 + Duration::hours(1),
                Some(anchor),
                Duration::days(14),
                &bands,
                Duration::hours(72),
            )
            .unwrap();
        assert!(est.expected <= 1.0 && est.lower >= 0.0 && est.upper <= 1.0);
    }

    #[test]
    fn forecast_refuses_past_and_beyond_max_horizon() {
        let from = monday();
        let history = flat_history(from, 70, 0.25);
        let climo = ShareClimatology::build(&history, Duration::minutes(15)).unwrap();
        let anchor = (history.last().unwrap().at, 0.25);
        let bands = degenerate_bands();
        // Le passé n'est pas prévu.
        assert!(
            climo
                .forecast_at(
                    anchor.0 - Duration::hours(1),
                    Some(anchor),
                    Duration::days(14),
                    &bands,
                    Duration::hours(72),
                )
                .is_none()
        );
        // Au-delà de l'horizon calibré : None, jamais d'extrapolation.
        assert!(
            climo
                .forecast_at(
                    anchor.0 + Duration::hours(73),
                    Some(anchor),
                    Duration::days(14),
                    &bands,
                    Duration::hours(72),
                )
                .is_none()
        );
    }

    #[test]
    fn backtest_beats_persistence_on_weekly_pattern() {
        // Motif hebdomadaire marqué (part haute la nuit, basse le jour) : la
        // climatologie doit battre la persistance nue à h+6/h+24.
        let from = monday();
        let days = 84;
        let history: Vec<Measurement> = (0..days * 96)
            .map(|i: i32| {
                let at = from + Duration::minutes(15) * i;
                let hour = at.hour() as f64;
                let share = 0.20 + 0.15 * ((hour - 12.0) / 12.0).abs();
                measurement(at, share * 1000.0, (1.0 - share) * 1000.0)
            })
            .collect();
        let test = TimeRange::new(from + Duration::days(70), from + Duration::days(83)).unwrap();
        let report = backtest_share(
            &history,
            test,
            Duration::days(70),
            Duration::days(1),
            ClimatologyParams {
                step: Duration::minutes(15),
                tau: Duration::hours(6),
            },
            &[Duration::hours(6), Duration::hours(24)],
            None,
            0.90,
        )
        .unwrap();
        let model = report.model.unwrap();
        let persistence = report.persistence.unwrap();
        assert!(
            model.rmse < persistence.rmse,
            "model={} persistence={}",
            model.rmse,
            persistence.rmse
        );
        assert!(report.origins > 0);
    }

    #[test]
    fn backtest_counts_firm_verdicts_with_bands() {
        // Part très loin du seuil 0,90 : tous les verdicts fermes sont des
        // `fail` corrects, aucun faux `pass`.
        let from = monday();
        let history = flat_history(from, 84, 0.25);
        let test = TimeRange::new(from + Duration::days(70), from + Duration::days(83)).unwrap();
        let params = ClimatologyParams {
            step: Duration::minutes(15),
            tau: Duration::days(14),
        };
        let calib = TimeRange::new(from + Duration::days(60), from + Duration::days(70)).unwrap();
        let bands = calibrate_share_bands(
            &history,
            calib,
            Duration::days(56),
            Duration::days(1),
            params,
            Duration::hours(24),
            0.1,
        )
        .unwrap();
        let report = backtest_share(
            &history,
            test,
            Duration::days(56),
            Duration::days(1),
            params,
            &[Duration::hours(6), Duration::hours(24)],
            Some(&bands),
            0.90,
        )
        .unwrap();
        assert_eq!(report.false_pass, 0);
        assert_eq!(report.false_fail, 0);
        assert!(report.firm_fail > 0);
        assert_eq!(report.firm_pass, 0);
        assert!(
            report.passes_gate() || report.model.unwrap().rmse >= report.persistence.unwrap().rmse
        );
    }

    #[test]
    fn coarse_truth_grid_never_yields_degenerate_forecast_bands() {
        // Vérité au pas 30 min, bandes au pas 15 min : sans comblement, les
        // index impairs seraient vides → bande (0,0) → intervalle DÉGÉNÉRÉ sur
        // une prévision (fausse certitude). Le comblement par le seau voisin
        // doit produire un intervalle non dégénéré à TOUT décalage.
        let from = monday();
        let history: Vec<Measurement> = (0..84 * 48)
            .map(|i: i32| {
                let at = from + Duration::minutes(30) * i;
                let noise = ((i as i64 * 2_654_435_761 % 1000) as f64 / 1000.0 - 0.5) * 0.1;
                let share = (0.25 + noise).clamp(0.0, 1.0);
                measurement(at, share * 1000.0, (1.0 - share) * 1000.0)
            })
            .collect();
        let params = ClimatologyParams {
            step: Duration::minutes(15),
            tau: Duration::days(14),
        };
        let calib = TimeRange::new(from + Duration::days(60), from + Duration::days(83)).unwrap();
        let bands = calibrate_share_bands(
            &history,
            calib,
            Duration::days(56),
            Duration::days(1),
            params,
            Duration::hours(24),
            0.1,
        )
        .unwrap();
        let climo = ShareClimatology::build(&history, params.step).unwrap();
        let anchor = (history.last().unwrap().at, 0.25);
        // Décalage IMPAIR au pas 15 min (jamais observé sur la grille 30 min).
        let est = climo
            .forecast_at(
                anchor.0 + Duration::minutes(45),
                Some(anchor),
                params.tau,
                &bands,
                Duration::hours(72),
            )
            .unwrap();
        assert!(
            est.upper > est.lower,
            "intervalle dégénéré sur un décalage non calibré : {est:?}"
        );
    }

    #[test]
    fn calibrated_bands_widen_intervals() {
        // Série bruitée par créneau : les bandes calibrées doivent produire un
        // intervalle non dégénéré.
        let from = monday();
        let history: Vec<Measurement> = (0..84 * 96)
            .map(|i| {
                let at = from + Duration::minutes(15) * i;
                // pseudo-bruit déterministe ±0,05
                let noise = ((i as i64 * 2_654_435_761 % 1000) as f64 / 1000.0 - 0.5) * 0.1;
                let share = (0.25 + noise).clamp(0.0, 1.0);
                measurement(at, share * 1000.0, (1.0 - share) * 1000.0)
            })
            .collect();
        let calib = TimeRange::new(from + Duration::days(60), from + Duration::days(83)).unwrap();
        let params = ClimatologyParams {
            step: Duration::minutes(15),
            tau: Duration::days(14),
        };
        let bands = calibrate_share_bands(
            &history,
            calib,
            Duration::days(56),
            Duration::days(1),
            params,
            Duration::hours(24),
            0.1,
        )
        .unwrap();
        let climo = ShareClimatology::build(&history, params.step).unwrap();
        let anchor = (history.last().unwrap().at, 0.25);
        let est = climo
            .forecast_at(
                anchor.0 + Duration::hours(6),
                Some(anchor),
                params.tau,
                &bands,
                Duration::hours(72),
            )
            .unwrap();
        assert!(est.upper > est.lower, "intervalle dégénéré : {est:?}");
    }
}
