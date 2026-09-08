//! Variante **météo-pilotée** de la prévision de part renouvelable
//! (`share-meteo@2`) — itération mesurable d'ADR-0028, **gardée par la
//! sous-commande dédiée `backtest-share-meteo`** : elle n'est promue que si
//! elle bat `share-clim@1` (RMSE global, mêmes origines) avec zéro faux
//! `pass` ferme. Ce module n'est **pas servi** (même politique de promotion
//! que `gbdt@1`, ADR-0012 — décision du 2026-07-04 : expérience conservée,
//! cf. addendum ADR-0028).
//!
//! Principe : **dérivation par canal** là où la météo *as-of* couvre la cible,
//! **repli exact sur la formule `share-clim@1`** au-delà —
//!
//! ```text
//! si météo(t | as-of origine) disponible :
//!   éolien̂(t)  = phys(vent(t))        + (obs₀ − phys(vent(t₀))) · exp(−Δt/τ)
//!   solairê(t) = phys(irr(t))  ×  ratio₀^decay  (ancre multiplicative, nuit → 0)
//!   hydrô/biô/nuĉ/fossilê(t) = climatologie de canal + anomalie d'ancre décroissante
//!   part̂(t)    = (éolien̂+solairê+hydrô+biô) / (Σ tous canaux)
//! sinon :
//!   part̂(t)    = share-clim@1 (climatologie de la part + anomalie d'ancre)
//! ```
//!
//! Le repli garantit **zéro régression** aux horizons non couverts par la
//! météo : le gain (ou la perte) ne peut venir que de la couverture météo
//! (~24 h en backtest — convention d'archive `run_at = valid_at − 24 h` —,
//! ~48 h en service avec `forecast_days=2`).
//!
//! Anti-fuite (ADR-0012 §6) : la lecture météo est *as-of* — dernier
//! `run_at` **strictement antérieur** à l'origine ; la calibration du
//! [`RenewableModel`] (capacités effectives) se refait **par origine** sur la
//! fenêtre d'apprentissage uniquement. Tout est pur (aucun port nouveau).

use std::collections::{BTreeMap, HashMap};

use carbonfr_core::domain::{
    ClimatologyParams, ErrorAccumulator, ErrorMetrics, GenerationMix, HorizonBands, Measurement,
    RenewableModel, RenewableSample, TimeRange, WeatherForecast, calibrate_renewable, week_slot,
};
use time::{Duration, OffsetDateTime};

use crate::share::renewable_share;
use crate::share_forecast::{
    FirmVerdictCounter, ShareClimatology, align_down, fill_empty_buckets, share_samples,
};

/// Identité versionnée du modèle candidat (ADR-0019/0028) : distincte de
/// `share-clim@1`, jamais de mutation silencieuse.
pub const SHARE_METEO_MODEL: &str = "share-meteo@2";

/// En dessous de cette production solaire *modélisée* à l'ancre (MW), l'ancre
/// multiplicative solaire est ignorée (nuit/aube : le ratio obs/modèle
/// exploserait sur un dénominateur quasi nul).
const SOLAR_ANCHOR_MIN_MW: f64 = 50.0;

/// Index météo prévisionnelle pour lectures *as-of* : par heure cible, les
/// runs triés par `run_at` croissant.
#[derive(Debug, Clone)]
pub struct WeatherIndex {
    by_hour: BTreeMap<i64, Vec<(OffsetDateTime, f64, f64)>>,
}

impl WeatherIndex {
    pub fn build(rows: &[WeatherForecast]) -> Self {
        let mut by_hour: BTreeMap<i64, Vec<(OffsetDateTime, f64, f64)>> = BTreeMap::new();
        for w in rows {
            by_hour
                .entry(w.valid_at.unix_timestamp().div_euclid(3600))
                .or_default()
                .push((w.run_at, w.wind, w.irradiance));
        }
        for runs in by_hour.values_mut() {
            runs.sort_by_key(|&(run, _, _)| run);
        }
        Self { by_hour }
    }

    /// Météo (vent km/h, irradiance W/m²) pour l'heure de `t`, telle que connue
    /// à `as_of` : dernier run **strictement antérieur** (anti-fuite). `None`
    /// si aucun run n'était encore publié.
    fn at(&self, t: OffsetDateTime, as_of: OffsetDateTime) -> Option<(f64, f64)> {
        let runs = self.by_hour.get(&t.unix_timestamp().div_euclid(3600))?;
        runs.iter()
            .rev()
            .find(|&&(run, _, _)| run < as_of)
            .map(|&(_, wind, irr)| (wind, irr))
    }
}

/// Canaux du mix retenus, dans l'ordre : les 4 renouvelables (numérateur de
/// [`renewable_share`]) puis nucléaire et fossile (reste du dénominateur).
/// Même convention que `share.rs` : productions bornées à ≥ 0, pompage et
/// échanges exclus, fossile = `thermique` agrégé s'il est présent.
const CHANNELS: usize = 6;
const CH_EOLIEN: usize = 0;
const CH_SOLAIRE: usize = 1;

fn channels(mix: &GenerationMix) -> [f64; CHANNELS] {
    let fossile = match mix.thermique {
        Some(t) => t.max(0.0),
        None => mix.gaz.max(0.0) + mix.charbon.max(0.0) + mix.fioul.max(0.0),
    };
    [
        mix.eolien.max(0.0),
        mix.solaire.max(0.0),
        mix.hydraulique.max(0.0),
        mix.bioenergies.max(0.0),
        mix.nucleaire.max(0.0),
        fossile,
    ]
}

fn share_from_channels(ch: &[f64; CHANNELS]) -> Option<f64> {
    let renewable: f64 = ch[..4].iter().sum();
    let total: f64 = ch.iter().sum();
    if total <= 0.0 {
        return None;
    }
    Some((renewable / total).clamp(0.0, 1.0))
}

/// Climatologie horaire-de-semaine d'un canal (MW) — non bornée à `[0, 1]`
/// (contrairement à [`ShareClimatology`], qui porte une part).
#[derive(Debug, Clone)]
struct ChannelClimatology {
    step_secs: i64,
    slot_means: HashMap<i64, f64>,
    overall_mean: f64,
}

impl ChannelClimatology {
    fn build(samples: &[(OffsetDateTime, f64)], step_secs: i64) -> Option<Self> {
        if step_secs <= 0 || samples.is_empty() {
            return None;
        }
        let mut sums: HashMap<i64, (f64, u32)> = HashMap::new();
        let mut total = 0.0;
        for &(at, v) in samples {
            let entry = sums.entry(week_slot(at, step_secs)).or_insert((0.0, 0));
            entry.0 += v;
            entry.1 += 1;
            total += v;
        }
        Some(Self {
            step_secs,
            slot_means: sums
                .into_iter()
                .map(|(slot, (sum, n))| (slot, sum / n as f64))
                .collect(),
            overall_mean: total / samples.len() as f64,
        })
    }

    fn at(&self, t: OffsetDateTime) -> f64 {
        self.slot_means
            .get(&week_slot(t, self.step_secs))
            .copied()
            .unwrap_or(self.overall_mean)
    }

    /// Climatologie + anomalie d'ancre décroissante, bornée à ≥ 0 (une
    /// production ne devient pas négative).
    fn expected_at(&self, t: OffsetDateTime, anchor: (OffsetDateTime, f64), decay: f64) -> f64 {
        let (t0, v0) = anchor;
        (self.at(t) + (v0 - self.at(t0)) * decay).max(0.0)
    }
}

/// Modèle `share-meteo@2` construit **par origine** sur une fenêtre
/// d'apprentissage strictement passée. Pur.
pub struct ShareMeteoModel<'w> {
    channel_clim: [ChannelClimatology; CHANNELS],
    /// Ancre = dernière mesure de la fenêtre dont la part est calculable
    /// (même règle que `fallback_anchor` — jamais une mesure dégénérée).
    anchor_at: OffsetDateTime,
    anchor_channels: [f64; CHANNELS],
    renewable: RenewableModel,
    /// Repli hors couverture météo : la formule `share-clim@1` **exacte**.
    fallback: ShareClimatology,
    fallback_anchor: (OffsetDateTime, f64),
    weather: &'w WeatherIndex,
    as_of: OffsetDateTime,
    tau: Duration,
}

impl<'w> ShareMeteoModel<'w> {
    /// Construit le modèle depuis une fenêtre de mesures (avec mix), la météo
    /// *as-of* `as_of` (l'origine) et les paramètres de climatologie. `None`
    /// si la fenêtre est inexploitable (pas de mix, calibration renouvelable
    /// dégénérée, ou climatologie de repli impossible).
    pub fn build(
        window: &[Measurement],
        weather: &'w WeatherIndex,
        as_of: OffsetDateTime,
        params: ClimatologyParams,
    ) -> Option<Self> {
        let step_secs = params.step.whole_seconds();
        let mut per_channel: [Vec<(OffsetDateTime, f64)>; CHANNELS] = Default::default();
        let mut share_series: Vec<(OffsetDateTime, f64)> = Vec::new();
        let mut renew_samples: Vec<RenewableSample> = Vec::new();
        let mut anchor: Option<(OffsetDateTime, [f64; CHANNELS])> = None;
        let mut fallback_anchor: Option<(OffsetDateTime, f64)> = None;

        for m in window {
            let Some(mix) = m.mix.as_ref() else { continue };
            let ch = channels(mix);
            // Mix dégénéré (présent mais total ≤ 0 — trou de donnée) : aucune
            // part n'est calculable, et ses zéros pollueraient les
            // climatologies de canal comme la calibration éolien/solaire
            // (0 MW face à une vraie météo). La mesure est ignorée
            // entièrement (audit 2026-08).
            if ch.iter().sum::<f64>() <= 0.0 {
                continue;
            }
            for (i, series) in per_channel.iter_mut().enumerate() {
                series.push((m.at, ch[i]));
            }
            if let Some(share) = renewable_share(mix) {
                share_series.push((m.at, share));
                // Une SEULE règle d'ancrage pour les deux ancres : la dernière
                // mesure dont la part est calculable. Ancrer les canaux sur une
                // mesure dégénérée (mix présent mais total ≤ 0 — trou de donnée)
                // désynchroniserait la branche météo-pilotée du repli et
                // fabriquerait un biais éolien/solaire contre une référence
                // quasi nulle.
                if fallback_anchor.is_none_or(|(at, _)| m.at > at) {
                    anchor = Some((m.at, ch));
                    fallback_anchor = Some((m.at, share));
                }
            }
            // Échantillon de calibration : météo *as-of l'origine* pour cette
            // heure passée × production réelle (mêmes conventions que
            // `CalibrateRenewable`, mais anti-fuite par construction).
            if let Some((wind_kmh, irradiance_wm2)) = weather.at(m.at, as_of) {
                renew_samples.push(RenewableSample {
                    wind_kmh,
                    irradiance_wm2,
                    eolien_mw: mix.eolien.max(0.0),
                    solaire_mw: mix.solaire.max(0.0),
                });
            }
        }

        let (anchor_at, anchor_channels) = anchor?;
        let channel_clim: Vec<ChannelClimatology> = per_channel
            .iter()
            .map(|s| ChannelClimatology::build(s, step_secs))
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            channel_clim: channel_clim.try_into().ok()?,
            anchor_at,
            anchor_channels,
            renewable: calibrate_renewable(&renew_samples, RenewableModel::v1_uncalibrated())?,
            fallback: ShareClimatology::from_samples(&share_series, params.step)?,
            fallback_anchor: fallback_anchor?,
            weather,
            as_of,
            tau: params.tau,
        })
    }

    /// Part attendue à `t` + provenance du régime (`true` = météo-piloté,
    /// `false` = repli `share-clim@1`).
    pub fn expected_at(&self, t: OffsetDateTime) -> (f64, bool) {
        let tau_secs = self.tau.whole_seconds() as f64;
        let decay = if tau_secs > 0.0 {
            (-((t - self.anchor_at).whole_seconds() as f64).abs() / tau_secs).exp()
        } else {
            0.0
        };

        if let Some((wind, irr)) = self.weather.at(t, self.as_of) {
            let mut expected = [0.0; CHANNELS];
            // Canaux météo-pilotés, corrigés à l'ancre contre le biais de
            // niveau du modèle physique (erreur de calibration locale).
            let phys_wind_t = self.renewable.estimate_wind_mw(wind);
            let phys_solar_t = self.renewable.estimate_solar_mw(irr);
            let (wind_bias, solar_ratio) = match self.weather.at(self.anchor_at, self.as_of) {
                Some((w0, i0)) => {
                    let phys_w0 = self.renewable.estimate_wind_mw(w0);
                    let phys_s0 = self.renewable.estimate_solar_mw(i0);
                    // Ratio borné : au voisinage du seuil, `obs/phys` peut
                    // exploser (phys ≈ 50 MW, obs réel bien plus haut) — une
                    // correction de niveau > ×4 ou < ÷4 signale une ancre
                    // dégénérée, on préfère alors le modèle calibré.
                    let ratio = if phys_s0 >= SOLAR_ANCHOR_MIN_MW {
                        (self.anchor_channels[CH_SOLAIRE] / phys_s0).clamp(0.25, 4.0)
                    } else {
                        1.0
                    };
                    (self.anchor_channels[CH_EOLIEN] - phys_w0, ratio)
                }
                None => (0.0, 1.0),
            };
            // Éolien : ancre additive (le biais est un décalage de niveau).
            expected[CH_EOLIEN] = (phys_wind_t + wind_bias * decay).max(0.0);
            // Solaire : ancre MULTIPLICATIVE (une correction additive ancrée en
            // journée fabriquerait du solaire la nuit), décroissante vers 1.
            expected[CH_SOLAIRE] = (phys_solar_t * (1.0 + (solar_ratio - 1.0) * decay)).max(0.0);
            for (i, slot) in expected.iter_mut().enumerate().skip(2) {
                *slot = self.channel_clim[i].expected_at(
                    t,
                    (self.anchor_at, self.anchor_channels[i]),
                    decay,
                );
            }
            if let Some(share) = share_from_channels(&expected) {
                return (share, true);
            }
        }

        (
            self.fallback.expected_at(t, self.fallback_anchor, self.tau),
            false,
        )
    }
}

/// Erreur par horizon des trois estimateurs, en points de part (0-1).
#[derive(Debug, Clone)]
pub struct ShareMeteoHorizonError {
    pub horizon: Duration,
    pub meteo: Option<ErrorMetrics>,
    pub clim: Option<ErrorMetrics>,
    pub persistence: Option<ErrorMetrics>,
}

/// Rapport du backtest comparatif `share-meteo@2` vs `share-clim@1` vs
/// persistance — **mêmes origines, mêmes cibles** (comparaison honnête).
#[derive(Debug, Clone)]
pub struct ShareMeteoBacktestReport {
    pub origins: usize,
    pub meteo: Option<ErrorMetrics>,
    pub clim: Option<ErrorMetrics>,
    pub persistence: Option<ErrorMetrics>,
    pub by_horizon: Vec<ShareMeteoHorizonError>,
    /// Points où la météo a réellement piloté l'estimation…
    pub weather_driven: usize,
    /// …et points servis par le repli `share-clim@1` (hors couverture météo).
    pub fallback: usize,
    /// Verdicts fermes au seuil (bandes météo calibrées fournies).
    pub firm_pass: usize,
    pub false_pass: usize,
    pub firm_fail: usize,
    pub false_fail: usize,
    pub straddle: usize,
}

impl ShareMeteoBacktestReport {
    /// GATE de promotion (ADR-0028, itération météo) : battre **`share-clim@1`**
    /// (pas seulement la persistance) en RMSE global, et zéro faux `pass`.
    pub fn passes_gate(&self) -> bool {
        let beats = match (&self.meteo, &self.clim) {
            (Some(m), Some(c)) => m.rmse < c.rmse,
            _ => false,
        };
        beats && self.false_pass == 0
    }
}

/// Backtest *walk-forward* pur comparant `share-meteo@2`, `share-clim@1` et la
/// persistance sur les mêmes points. `history` doit couvrir
/// `[test.start − lookback, test.end + max(checkpoints))` ; `weather` porte
/// l'historique **multi-run** brut (la sélection *as-of* se fait par origine).
#[allow(clippy::too_many_arguments)]
pub fn backtest_share_meteo(
    history: &[Measurement],
    weather: &WeatherIndex,
    test: TimeRange,
    lookback: Duration,
    origin_step: Duration,
    params: ClimatologyParams,
    checkpoints: &[Duration],
    bands: Option<&HorizonBands>,
    threshold: f64,
) -> Option<ShareMeteoBacktestReport> {
    let step_secs = params.step.whole_seconds();
    if step_secs <= 0 || origin_step <= Duration::ZERO || checkpoints.is_empty() {
        return None;
    }
    // L'appelant (repo Postgres `ORDER BY at`) fournit déjà un historique trié :
    // on n'en clone/trie une copie que si la précondition n'est pas satisfaite.
    let sorted_storage: Vec<Measurement>;
    let sorted: &[Measurement] = if history.is_sorted_by_key(|m| m.at) {
        history
    } else {
        sorted_storage = {
            let mut v = history.to_vec();
            v.sort_by_key(|m| m.at);
            v
        };
        &sorted_storage
    };
    let samples = share_samples(sorted);
    if samples.is_empty() {
        return None;
    }
    let truth: BTreeMap<i64, f64> = samples
        .iter()
        .map(|&(at, s)| (at.unix_timestamp(), s))
        .collect();

    let mut meteo_acc = ErrorAccumulator::default();
    let mut clim_acc = ErrorAccumulator::default();
    let mut persistence_acc = ErrorAccumulator::default();
    let mut by_horizon: Vec<(
        Duration,
        ErrorAccumulator,
        ErrorAccumulator,
        ErrorAccumulator,
    )> = checkpoints
        .iter()
        .map(|&h| {
            (
                h,
                Default::default(),
                Default::default(),
                Default::default(),
            )
        })
        .collect();
    let (mut weather_driven, mut fallback) = (0usize, 0usize);
    let mut firm = FirmVerdictCounter::default();
    let mut origins = 0usize;

    let mut cursor = align_down(test.start(), step_secs);
    while cursor < test.end() {
        let origin = cursor;
        cursor += origin_step;

        let window_start = origin - lookback;
        let end_idx = sorted.partition_point(|m| m.at < origin);
        let start_idx = sorted.partition_point(|m| m.at < window_start);
        let window = &sorted[start_idx..end_idx];
        let Some(model) = ShareMeteoModel::build(window, weather, origin, params) else {
            continue;
        };
        // Baselines sur la MÊME fenêtre : share-clim@1 est le repli interne du
        // modèle (mêmes échantillons), la persistance son ancre.
        let clim = &model.fallback;
        let anchor = model.fallback_anchor;
        origins += 1;

        for (h, meteo_h, clim_h, persistence_h) in by_horizon.iter_mut() {
            let t = origin + *h;
            let Some(&observed) = truth.get(&t.unix_timestamp()) else {
                continue;
            };
            let (meteo_expected, driven) = model.expected_at(t);
            if driven {
                weather_driven += 1;
            } else {
                fallback += 1;
            }
            let clim_expected = clim.expected_at(t, anchor, params.tau);
            meteo_acc.observe(meteo_expected, observed);
            meteo_h.observe(meteo_expected, observed);
            clim_acc.observe(clim_expected, observed);
            clim_h.observe(clim_expected, observed);
            persistence_acc.observe(anchor.1, observed);
            persistence_h.observe(anchor.1, observed);

            if let Some(band) = bands.and_then(|b| b.at(*h)) {
                firm.observe(meteo_expected, observed, band, threshold);
            }
        }
    }

    Some(ShareMeteoBacktestReport {
        origins,
        meteo: meteo_acc.metrics(),
        clim: clim_acc.metrics(),
        persistence: persistence_acc.metrics(),
        by_horizon: by_horizon
            .into_iter()
            .map(|(horizon, m, c, p)| ShareMeteoHorizonError {
                horizon,
                meteo: m.metrics(),
                clim: c.metrics(),
                persistence: p.metrics(),
            })
            .collect(),
        weather_driven,
        fallback,
        firm_pass: firm.firm_pass,
        false_pass: firm.false_pass,
        firm_fail: firm.firm_fail,
        false_fail: firm.false_fail,
        straddle: firm.straddle,
    })
}

/// Calibre les bandes de résidus par horizon de `share-meteo@2` (quantiles
/// `q`/`1−q` de `observé − prévu`), walk-forward pur — même discipline
/// anti-fuite et même comblement de seaux vides que `calibrate_share_bands`.
#[allow(clippy::too_many_arguments)]
pub fn calibrate_share_meteo_bands(
    history: &[Measurement],
    weather: &WeatherIndex,
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
    // L'appelant (repo Postgres `ORDER BY at`) fournit déjà un historique trié :
    // on n'en clone/trie une copie que si la précondition n'est pas satisfaite.
    let sorted_storage: Vec<Measurement>;
    let sorted: &[Measurement] = if history.is_sorted_by_key(|m| m.at) {
        history
    } else {
        sorted_storage = {
            let mut v = history.to_vec();
            v.sort_by_key(|m| m.at);
            v
        };
        &sorted_storage
    };
    let samples = share_samples(sorted);
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

        let end_idx = sorted.partition_point(|m| m.at < origin);
        let start_idx = sorted.partition_point(|m| m.at < origin - lookback);
        let window = &sorted[start_idx..end_idx];
        let Some(model) = ShareMeteoModel::build(window, weather, origin, params) else {
            continue;
        };

        for (idx, bucket) in residuals.iter_mut().enumerate().skip(1) {
            let t = origin + params.step * idx as i32;
            let Some(&observed) = truth.get(&t.unix_timestamp()) else {
                continue;
            };
            let (expected, _) = model.expected_at(t);
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

#[cfg(test)]
mod tests {
    use super::*;
    use carbonfr_core::domain::{CarbonIntensity, Methodology, Region, Vintage};

    /// 2024-01-01 00:00 UTC (un lundi — créneau de semaine déterministe).
    fn monday() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_704_067_200).unwrap()
    }

    fn measurement(at: OffsetDateTime, mix: GenerationMix) -> Measurement {
        Measurement {
            at,
            region: Region::National,
            intensity: CarbonIntensity::new(30.0).unwrap(),
            methodology: Methodology::rte_direct(),
            vintage: Vintage::Tr,
            mix: Some(mix),
        }
    }

    /// Mix synthétique piloté par une « météo » : éolien = 100 × vent,
    /// solaire = 20 × irradiance, le reste constant.
    fn mix_for(wind_kmh: f64, irradiance_wm2: f64) -> GenerationMix {
        GenerationMix {
            nucleaire: 40_000.0,
            gaz: 2_000.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 8_000.0,
            eolien: 100.0 * wind_kmh,
            solaire: 20.0 * irradiance_wm2,
            bioenergies: 1_000.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        }
    }

    /// Météo horaire sur `hours` heures depuis `start`, vent alterné
    /// faible/fort par blocs de 5 h (période 10 h : **apériodique** vis-à-vis de
    /// la grille horaire-de-semaine, 168/10 non entier — la climatologie ne peut
    /// pas la capturer, seule la météo le peut), irradiance diurne ; runs 24 h
    /// avant validité (convention d'archive anti-fuite).
    fn weather_rows(start: OffsetDateTime, hours: i64) -> Vec<WeatherForecast> {
        (0..hours)
            .map(|h| {
                let valid_at = start + Duration::hours(h);
                WeatherForecast {
                    run_at: valid_at - Duration::hours(24),
                    valid_at,
                    wind: if (h / 5) % 2 == 0 { 12.0 } else { 45.0 },
                    irradiance: if (6..18).contains(&(h % 24)) {
                        400.0
                    } else {
                        0.0
                    },
                }
            })
            .collect()
    }

    fn history(hours: i64, weather: &[WeatherForecast]) -> Vec<Measurement> {
        weather
            .iter()
            .take(hours as usize)
            .map(|w| measurement(w.valid_at, mix_for(w.wind, w.irradiance)))
            .collect()
    }

    fn params() -> ClimatologyParams {
        ClimatologyParams {
            step: Duration::hours(1),
            tau: Duration::days(14),
        }
    }

    #[test]
    fn channels_decomposition_matches_renewable_share_rule() {
        // `channels`/`share_from_channels` réencodent la règle de `share.rs`
        // (numérateur, clamps, pompage/échanges exclus, thermique agrégé) :
        // ce test golden les garde synchronisés — toute dérive casse ici.
        let mut mixes = vec![mix_for(30.0, 500.0)];
        let mut regional = mix_for(20.0, 100.0);
        regional.thermique = Some(3_000.0);
        regional.gaz = 9_999.0; // ignoré quand `thermique` est présent
        mixes.push(regional);
        let mut negatif = mix_for(10.0, 0.0);
        negatif.hydraulique = -500.0;
        negatif.pompage = -2_000.0;
        negatif.echanges = 5_000.0;
        mixes.push(negatif);
        for mix in &mixes {
            assert_eq!(share_from_channels(&channels(mix)), renewable_share(mix));
        }
        let zero = GenerationMix {
            nucleaire: 0.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 0.0,
            eolien: 0.0,
            solaire: 0.0,
            bioenergies: 0.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        };
        assert_eq!(share_from_channels(&channels(&zero)), None);
        assert_eq!(renewable_share(&zero), None);
    }

    #[test]
    fn degenerate_trailing_mix_never_anchors_the_model() {
        // Dernière mesure de la fenêtre = mix présent mais total ≤ 0 (trou de
        // donnée) : l'ancre doit rester sur la dernière mesure VALIDE, pour la
        // branche météo-pilotée comme pour le repli (constat de revue).
        let start = monday();
        let train_hours = 6 * 7 * 24;
        let rows = weather_rows(start, train_hours + 24);
        let mut hist = history(train_hours, &rows);
        let zero = GenerationMix {
            nucleaire: 0.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 0.0,
            eolien: 0.0,
            solaire: 0.0,
            bioenergies: 0.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        };
        let degenerate_at = start + Duration::hours(train_hours - 1) + Duration::minutes(30);
        hist.push(measurement(degenerate_at, zero));
        let index = WeatherIndex::build(&rows);
        let origin = start + Duration::hours(train_hours);
        let model = ShareMeteoModel::build(&hist, &index, origin, params()).expect("modèle");
        assert!(
            model.anchor_at < degenerate_at,
            "l'ancre ne doit pas être la mesure dégénérée"
        );
        assert_eq!(
            model.anchor_at, model.fallback_anchor.0,
            "les deux ancres suivent la même règle"
        );
    }

    #[test]
    fn degenerate_mixes_do_not_pollute_climatologies_or_calibration() {
        // Audit 2026-08 : un mix présent mais total ≤ 0 (trou de donnée) ne
        // doit alimenter NI les climatologies de canal (zéros dans les
        // moyennes de créneau) NI la calibration éolien/solaire (0 MW face à
        // une vraie météo) : le modèle bâti avec ces trous doit être
        // IDENTIQUE au modèle bâti sans.
        let start = monday();
        let train_hours = 6 * 7 * 24;
        let rows = weather_rows(start, train_hours + 24);
        let clean = history(train_hours, &rows);
        let zero = GenerationMix {
            nucleaire: 0.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 0.0,
            eolien: 0.0,
            solaire: 0.0,
            bioenergies: 0.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        };
        let mut polluted = clean.clone();
        // Trous à des heures couvertes par la météo (candidates à la
        // calibration) et dans des créneaux de la climatologie de canal.
        for h in [10i64, 100, 500] {
            polluted.push(measurement(
                start + Duration::hours(h) + Duration::minutes(30),
                zero,
            ));
        }
        polluted.sort_by_key(|m| m.at);
        let index = WeatherIndex::build(&rows);
        let origin = start + Duration::hours(train_hours);
        let a = ShareMeteoModel::build(&clean, &index, origin, params()).expect("modèle");
        let b = ShareMeteoModel::build(&polluted, &index, origin, params()).expect("modèle");
        for h in [2i64, 7, 12] {
            let t = origin + Duration::hours(h);
            assert_eq!(a.expected_at(t), b.expected_at(t), "h+{h}");
        }
    }

    #[test]
    fn as_of_lookup_never_uses_runs_published_after_origin() {
        let start = monday();
        let valid_at = start + Duration::hours(2);
        // Deux runs pour la même heure : un ancien, un publié APRÈS l'origine.
        let rows = vec![
            WeatherForecast {
                run_at: start - Duration::hours(24),
                valid_at,
                wind: 10.0,
                irradiance: 0.0,
            },
            WeatherForecast {
                run_at: start + Duration::hours(1),
                valid_at,
                wind: 99.0,
                irradiance: 0.0,
            },
        ];
        let index = WeatherIndex::build(&rows);
        // As-of l'origine `start` : seul le run antérieur est visible.
        assert_eq!(index.at(valid_at, start), Some((10.0, 0.0)));
        // As-of après le second run : le plus récent gagne.
        assert_eq!(
            index.at(valid_at, start + Duration::hours(2)),
            Some((99.0, 0.0))
        );
    }

    #[test]
    fn weather_driven_forecast_tracks_synthetic_weather() {
        let start = monday();
        // 6 semaines d'apprentissage + 24 h de météo future connue as-of origine.
        let train_hours = 6 * 7 * 24;
        let rows = weather_rows(start, train_hours + 24);
        let hist = history(train_hours, &rows);
        let index = WeatherIndex::build(&rows);
        let origin = start + Duration::hours(train_hours);
        let model = ShareMeteoModel::build(&hist, &index, origin, params()).expect("modèle");

        // À h+7 le régime de vent a basculé : la vérité synthétique est connue.
        let t = origin + Duration::hours(7);
        let (expected, driven) = model.expected_at(t);
        assert!(driven, "la météo couvre t → régime météo-piloté");
        let truth_mix = mix_for(
            if ((train_hours + 7) / 5) % 2 == 0 {
                12.0
            } else {
                45.0
            },
            if (6..18).contains(&((train_hours + 7) % 24)) {
                400.0
            } else {
                0.0
            },
        );
        let truth = renewable_share(&truth_mix).unwrap();
        assert!(
            (expected - truth).abs() < 0.02,
            "météo-piloté ≈ vérité synthétique (attendu {truth:.4}, obtenu {expected:.4})"
        );
    }

    #[test]
    fn beyond_weather_coverage_falls_back_to_share_clim_exactly() {
        let start = monday();
        let train_hours = 6 * 7 * 24;
        let rows = weather_rows(start, train_hours + 24);
        let hist = history(train_hours, &rows);
        let index = WeatherIndex::build(&rows);
        let origin = start + Duration::hours(train_hours);
        let model = ShareMeteoModel::build(&hist, &index, origin, params()).expect("modèle");

        // h+30 : au-delà de la couverture météo (24 h) → repli exact.
        let t = origin + Duration::hours(30);
        let (expected, driven) = model.expected_at(t);
        assert!(!driven, "hors couverture météo → repli");
        let clim = ShareClimatology::build(&hist, Duration::hours(1)).unwrap();
        let anchor = model.fallback_anchor;
        assert_eq!(
            expected,
            clim.expected_at(t, anchor, Duration::days(14)),
            "le repli EST la formule share-clim@1"
        );
    }

    #[test]
    fn solar_anchor_at_night_never_fabricates_solar() {
        let start = monday();
        let train_hours = 6 * 7 * 24; // se termine à minuit → ancre nocturne
        let rows = weather_rows(start, train_hours + 24);
        let hist = history(train_hours, &rows);
        let index = WeatherIndex::build(&rows);
        let origin = start + Duration::hours(train_hours);
        let model = ShareMeteoModel::build(&hist, &index, origin, params()).expect("modèle");

        // Cible nocturne (h+2, avant l'aube) : le solaire modélisé doit être nul
        // même si l'ancre porte un biais — la part reste calculable et bornée.
        let (expected, driven) = model.expected_at(origin + Duration::hours(2));
        assert!(driven);
        assert!((0.0..=1.0).contains(&expected));
    }

    #[test]
    fn backtest_compares_all_three_on_same_points_and_gates() {
        let start = monday();
        let total_hours = 10 * 7 * 24;
        let rows = weather_rows(start, total_hours);
        let hist = history(total_hours, &rows);
        let index = WeatherIndex::build(&rows);
        let test = TimeRange::new(
            start + Duration::hours(8 * 7 * 24),
            start + Duration::hours(9 * 7 * 24),
        )
        .unwrap();
        let report = backtest_share_meteo(
            &hist,
            &index,
            test,
            Duration::days(35),
            Duration::hours(12),
            params(),
            &[Duration::hours(1), Duration::hours(6)],
            None,
            0.90,
        )
        .expect("rapport");
        assert!(report.origins > 0);
        let (m, c, p) = (
            report.meteo.as_ref().unwrap(),
            report.clim.as_ref().unwrap(),
            report.persistence.as_ref().unwrap(),
        );
        assert_eq!(m.n, c.n, "mêmes points évalués pour les trois estimateurs");
        assert_eq!(m.n, p.n);
        assert!(
            report.weather_driven > 0,
            "la couverture météo est exploitée"
        );
        // Sur données synthétiques météo-pilotées, le modèle météo domine.
        assert!(
            m.rmse < c.rmse,
            "météo {m:?} doit battre climatologie {c:?}"
        );
    }

    #[test]
    fn calibrated_bands_are_never_degenerate() {
        let start = monday();
        let total_hours = 10 * 7 * 24;
        let rows = weather_rows(start, total_hours);
        let hist = history(total_hours, &rows);
        let index = WeatherIndex::build(&rows);
        let calib = TimeRange::new(
            start + Duration::hours(8 * 7 * 24),
            start + Duration::hours(10 * 7 * 24),
        )
        .unwrap();
        let bands = calibrate_share_meteo_bands(
            &hist,
            &index,
            calib,
            Duration::days(35),
            Duration::hours(12),
            params(),
            Duration::hours(24),
            0.1,
        )
        .expect("bandes");
        for h in [1, 6, 12, 24] {
            let (low, high) = bands.at(Duration::hours(h)).expect("bande");
            assert!(low <= high, "bande ordonnée à h+{h}");
            assert!(
                low != 0.0 || high != 0.0,
                "bande non dégénérée à h+{h} (le comblement de seaux vides a joué)"
            );
        }
    }
}
