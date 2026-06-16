//! Prévision `acv-ademe@2` (consumption-based) par **climatologie des entrées**
//! (ADR-0013).
//!
//! Principe (ADR-0013 §1-2) : on ne prévoit **pas** l'intensité `@2` directement
//! (boîte noire), on prévoit les **entrées** — le mix par filière et le contexte
//! d'import (flux + intensité de chaque voisin) — puis on applique le **même**
//! calculateur pur `AcvAdeme` (ADR-0010) qu'au nowcast. Le calculateur est
//! **agnostique au régime** (observé/prévu) : la prévision hérite ainsi de la
//! version de méthode, reste auditable, et **converge vers le nowcast** quand
//! l'horizon → 0 (chaque canal y est tiré vers sa dernière observation par la
//! correction de persistance, comme `climatology@1`).
//!
//! Chaque canal scalaire est prévu par la formule de `climatology@1` (climatologie
//! horaire-de-semaine + biais de persistance décroissant). Les flux sont **signés**
//! (non bornés à 0), les productions et intensités voisines bornées à 0.

use std::collections::{BTreeSet, HashMap};

use time::{Duration, OffsetDateTime};

use super::forecast::{band_offsets, week_slot};
use crate::domain::{
    AcvAdemeConsumption, CarbonIntensity, ClimatologyParams, CrossBorderFlow, CrossBorderFlows,
    CrossBorderSnapshot, EmissionFactors, ForecastPoint, GenerationMix, HorizonBands, Measurement,
    Methodology, MethodologyCalculator, MethodologyContext, ModelVersion, Neighbor,
    derive_consumption_series,
};

/// Identité versionnée du modèle de prévision `acv-ademe` (ADR-0013).
pub const ACV_FORECAST_ID: &str = "acv-clim";
pub const ACV_FORECAST_VERSION: u32 = 1;

/// Garde-fou sur le nombre de pas (cohérent avec `climatology@1`).
const MAX_STEPS: i64 = 100_000;

/// Prévision scalaire d'un canal : climatologie horaire-de-semaine + biais de
/// persistance décroissant (formule `climatology@1`, ADR-0009).
struct Channel {
    slot_mean: HashMap<i64, f64>,
    overall_mean: f64,
    t0: OffsetDateTime,
    bias: f64,
}

impl Channel {
    /// Construit le canal depuis ses échantillons `(horodatage, valeur)`. `None`
    /// si vide.
    fn build(samples: &[(OffsetDateTime, f64)], step_secs: i64) -> Option<Channel> {
        if samples.is_empty() {
            return None;
        }
        let mut sums: HashMap<i64, (f64, u32)> = HashMap::new();
        let mut total = 0.0;
        for (at, v) in samples {
            let e = sums.entry(week_slot(*at, step_secs)).or_insert((0.0, 0));
            e.0 += v;
            e.1 += 1;
            total += v;
        }
        let overall_mean = total / samples.len() as f64;
        let slot_mean = sums
            .iter()
            .map(|(k, (s, c))| (*k, s / *c as f64))
            .collect::<HashMap<_, _>>();
        let anchor = samples.iter().max_by_key(|(at, _)| *at)?;
        let climo_t0 = *slot_mean
            .get(&week_slot(anchor.0, step_secs))
            .unwrap_or(&overall_mean);
        Some(Channel {
            slot_mean,
            overall_mean,
            t0: anchor.0,
            bias: anchor.1 - climo_t0,
        })
    }

    /// Valeur prévue au temps `t`. `clamp_nonneg` borne à 0 (productions,
    /// intensités) ; sinon signé (flux transfrontaliers).
    fn at(&self, t: OffsetDateTime, step_secs: i64, tau_secs: f64, clamp_nonneg: bool) -> f64 {
        let mean = *self
            .slot_mean
            .get(&week_slot(t, step_secs))
            .unwrap_or(&self.overall_mean);
        let dt = (t - self.t0).abs().whole_seconds() as f64;
        let v = mean + self.bias * (-dt / tau_secs).exp();
        if clamp_nonneg { v.max(0.0) } else { v }
    }
}

/// Extrait la série `(horodatage, valeur)` d'une filière depuis l'historique de mix.
fn mix_channel(
    history: &[Measurement],
    get: fn(&GenerationMix) -> f64,
    step_secs: i64,
) -> Option<Channel> {
    let samples: Vec<(OffsetDateTime, f64)> = history
        .iter()
        .filter_map(|m| m.mix.as_ref().map(|mx| (m.at, get(mx))))
        .collect();
    Channel::build(&samples, step_secs)
}

/// Prévoit la série `acv-ademe@2` sur `[from, from + horizon)` au pas
/// `params.step`, par climatologie des entrées + calculateur (ADR-0013).
///
/// `mix_history` (mesures portant le mix FR, p. ex. `acv-ademe@1`) et
/// `flow_history` (contexte d'import) doivent être **triés par horodatage
/// croissant**. **National** uniquement (ADR-0013 §8 ; `thermique = None`).
///
/// Intervalle : dispersion empirique par créneau de la **série `@2` dérivée de
/// l'historique** (repli sur la dispersion globale), recentrée sur la prévision —
/// la calibration par quantiles de résidus par horizon (ADR-0011 §5) la
/// raffinera derrière le même contrat. `None` si un historique est vide ou les
/// paramètres/horizon invalides.
#[allow(clippy::too_many_arguments)]
pub fn acv_ademe_forecast(
    mix_history: &[Measurement],
    flow_history: &[CrossBorderSnapshot],
    from: OffsetDateTime,
    horizon: Duration,
    params: ClimatologyParams,
    factors: &EmissionFactors,
    td_loss: f64,
    bands: Option<&HorizonBands>,
) -> Option<Vec<ForecastPoint>> {
    let step_secs = params.step.whole_seconds();
    let tau_secs = params.tau.whole_seconds() as f64;
    if mix_history.is_empty()
        || flow_history.is_empty()
        || step_secs <= 0
        || tau_secs <= 0.0
        || horizon <= Duration::ZERO
        || horizon.whole_seconds() / step_secs > MAX_STEPS
    {
        return None;
    }

    let region = mix_history.iter().max_by_key(|m| m.at)?.region;

    // Canaux mix par filière (national : thermique agrégé absent).
    let nucleaire = mix_channel(mix_history, |m| m.nucleaire, step_secs)?;
    let gaz = mix_channel(mix_history, |m| m.gaz, step_secs)?;
    let charbon = mix_channel(mix_history, |m| m.charbon, step_secs)?;
    let fioul = mix_channel(mix_history, |m| m.fioul, step_secs)?;
    let hydraulique = mix_channel(mix_history, |m| m.hydraulique, step_secs)?;
    let eolien = mix_channel(mix_history, |m| m.eolien, step_secs)?;
    let solaire = mix_channel(mix_history, |m| m.solaire, step_secs)?;
    let bioenergies = mix_channel(mix_history, |m| m.bioenergies, step_secs)?;

    // Canaux d'import : un canal flux + un canal intensité par voisin observé.
    let mut neighbors: BTreeSet<Neighbor> = BTreeSet::new();
    for s in flow_history {
        for f in &s.flows.flows {
            neighbors.insert(f.neighbor);
        }
    }
    let mut neighbor_channels: Vec<(Neighbor, Channel, Channel)> = Vec::new();
    for n in neighbors {
        let flow_samples: Vec<(OffsetDateTime, f64)> = flow_history
            .iter()
            .filter_map(|s| {
                s.flows
                    .flows
                    .iter()
                    .find(|f| f.neighbor == n)
                    .map(|f| (s.at, f.flow_mw))
            })
            .collect();
        let int_samples: Vec<(OffsetDateTime, f64)> = flow_history
            .iter()
            .filter_map(|s| {
                s.flows
                    .flows
                    .iter()
                    .find(|f| f.neighbor == n)
                    .map(|f| (s.at, f.neighbor_intensity.value()))
            })
            .collect();
        if let (Some(flow_ch), Some(int_ch)) = (
            Channel::build(&flow_samples, step_secs),
            Channel::build(&int_samples, step_secs),
        ) {
            neighbor_channels.push((n, flow_ch, int_ch));
        }
    }
    if neighbor_channels.is_empty() {
        return None;
    }

    // Dispersion de la série `@2` historique, par créneau (pour l'intervalle).
    let acv_history = derive_consumption_series(mix_history, flow_history, factors, td_loss);
    let mut acv_slots: HashMap<i64, Vec<f64>> = HashMap::new();
    let mut acv_all: Vec<f64> = Vec::with_capacity(acv_history.len());
    for m in &acv_history {
        let v = m.intensity.value();
        acv_slots
            .entry(week_slot(m.at, step_secs))
            .or_default()
            .push(v);
        acv_all.push(v);
    }

    let model = ModelVersion::new(ACV_FORECAST_ID, ACV_FORECAST_VERSION);
    let methodology = Methodology::acv_ademe_consumption();
    let end = from + horizon;
    let mut points = Vec::new();
    let mut t = from;

    while t < end {
        let mix = GenerationMix {
            nucleaire: nucleaire.at(t, step_secs, tau_secs, true),
            gaz: gaz.at(t, step_secs, tau_secs, true),
            charbon: charbon.at(t, step_secs, tau_secs, true),
            fioul: fioul.at(t, step_secs, tau_secs, true),
            hydraulique: hydraulique.at(t, step_secs, tau_secs, true),
            eolien: eolien.at(t, step_secs, tau_secs, true),
            solaire: solaire.at(t, step_secs, tau_secs, true),
            bioenergies: bioenergies.at(t, step_secs, tau_secs, true),
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        };
        let flows = CrossBorderFlows::new(
            neighbor_channels
                .iter()
                .filter_map(|(n, flow_ch, int_ch)| {
                    let intensity = CarbonIntensity::new(int_ch.at(t, step_secs, tau_secs, true))?;
                    Some(CrossBorderFlow {
                        neighbor: *n,
                        flow_mw: flow_ch.at(t, step_secs, tau_secs, false),
                        neighbor_intensity: intensity,
                    })
                })
                .collect(),
        );

        let ctx = MethodologyContext {
            mix: &mix,
            cross_border: Some(&flows),
            factors,
            td_loss,
            published: None,
        };
        if let Some(expected) = AcvAdemeConsumption.intensity(&ctx) {
            let ev = expected.value();
            // Intervalle : quantiles de résidus par horizon si calibrés
            // (ADR-0011 §5), sinon dispersion empirique par créneau de la série
            // `@2` historique (repli sur la dispersion globale).
            let (lower_value, upper_value) = match bands.and_then(|b| b.at(t - from)) {
                Some((q_low, q_high)) => ((ev + q_low).max(0.0), (ev + q_high).max(ev)),
                None => {
                    let (low_off, high_off) = match acv_slots.get(&week_slot(t, step_secs)) {
                        Some(s) if s.len() >= 2 => {
                            let mean = s.iter().sum::<f64>() / s.len() as f64;
                            band_offsets(s, mean)
                        }
                        _ if acv_all.len() >= 2 => {
                            let mut sorted = acv_all.clone();
                            sorted.sort_by(|a, b| a.total_cmp(b));
                            let mean = acv_all.iter().sum::<f64>() / acv_all.len() as f64;
                            band_offsets(&sorted, mean)
                        }
                        _ => (0.0, 0.0),
                    };
                    ((ev - low_off).max(0.0), ev + high_off)
                }
            };
            if let (Some(lower), Some(upper)) = (
                CarbonIntensity::new(lower_value),
                CarbonIntensity::new(upper_value),
            ) {
                points.push(ForecastPoint::new(
                    t,
                    region,
                    expected,
                    lower,
                    upper,
                    methodology.clone(),
                    model.clone(),
                ));
            }
        }
        t += params.step;
    }
    Some(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Region, Vintage, acv_ademe_consumption_intensity};

    fn national_mix(nuclear: f64) -> GenerationMix {
        GenerationMix {
            nucleaire: nuclear,
            gaz: 1000.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 5000.0,
            eolien: 3000.0,
            solaire: 500.0,
            bioenergies: 800.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        }
    }

    fn flows(import_de: f64, intensity: f64) -> CrossBorderFlows {
        CrossBorderFlows::new(vec![CrossBorderFlow {
            neighbor: Neighbor::Germany,
            flow_mw: import_de,
            neighbor_intensity: CarbonIntensity::new(intensity).unwrap(),
        }])
    }

    /// Historiques alignés au pas horaire sur `count` points finissant à `end`
    /// (exclu), motif jour/nuit sur le nucléaire et l'import.
    fn histories(
        end: OffsetDateTime,
        step: Duration,
        count: i32,
    ) -> (Vec<Measurement>, Vec<CrossBorderSnapshot>) {
        let mut mix = Vec::new();
        let mut flow = Vec::new();
        for i in 0..count {
            let at = end - step * (count - i);
            let nuclear = if (0..12).contains(&at.hour()) {
                40000.0
            } else {
                35000.0
            };
            let import = if (0..12).contains(&at.hour()) {
                1000.0
            } else {
                4000.0
            };
            mix.push(Measurement {
                at,
                region: Region::National,
                intensity: CarbonIntensity::new(12.0).unwrap(),
                methodology: Methodology::acv_ademe(),
                vintage: Vintage::Consolidated,
                mix: Some(national_mix(nuclear)),
            });
            flow.push(CrossBorderSnapshot {
                at,
                flows: flows(import, 400.0),
            });
        }
        (mix, flow)
    }

    #[test]
    fn empty_history_returns_none() {
        let from = OffsetDateTime::UNIX_EPOCH;
        assert!(
            acv_ademe_forecast(
                &[],
                &[],
                from,
                Duration::hours(24),
                ClimatologyParams::default(),
                &EmissionFactors::acv_ademe_v1(),
                0.072,
                None,
            )
            .is_none()
        );
    }

    #[test]
    fn forecast_carries_consumption_methodology_and_model() {
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(60);
        let step = Duration::hours(1);
        let (mix, flow) = histories(from, step, 14 * 24);
        let out = acv_ademe_forecast(
            &mix,
            &flow,
            from,
            Duration::hours(24),
            ClimatologyParams {
                step,
                tau: Duration::days(14),
            },
            &EmissionFactors::acv_ademe_v1(),
            0.072,
            None,
        )
        .unwrap();
        assert_eq!(out.len(), 24);
        for p in &out {
            assert_eq!(p.methodology, Methodology::acv_ademe_consumption());
            assert_eq!(
                p.model,
                ModelVersion::new(ACV_FORECAST_ID, ACV_FORECAST_VERSION)
            );
            assert!(p.lower.value() <= p.expected.value());
            assert!(p.expected.value() <= p.upper.value());
        }
    }

    #[test]
    fn converges_to_nowcast_at_anchor() {
        // À `from` = ancre, chaque canal vaut sa dernière observation → les
        // entrées prévues = les entrées observées → la prévision = le nowcast
        // (calculateur appliqué au dernier mix + dernier contexte d'import).
        let step = Duration::hours(1);
        let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(90);
        // Historique finissant *à* t0 inclus (dernière obs = t0).
        let (mut mix, mut flow) = histories(t0 + step, step, 14 * 24);
        // Dernière obs exactement à t0.
        assert_eq!(mix.last().unwrap().at, t0);
        assert_eq!(flow.last().unwrap().at, t0);

        let factors = EmissionFactors::acv_ademe_v1();
        let td = 0.072;
        let nowcast = {
            let m = mix.last().unwrap();
            acv_ademe_consumption_intensity(
                m.mix.as_ref().unwrap(),
                &flow.last().unwrap().flows,
                &factors,
                td,
            )
            .unwrap()
        };

        let out = acv_ademe_forecast(
            &mix,
            &flow,
            t0,
            Duration::hours(2),
            ClimatologyParams {
                step,
                tau: Duration::days(14),
            },
            &factors,
            td,
            None,
        )
        .unwrap();
        // mut pour neutraliser un warning si jamais inutilisé.
        let _ = (&mut mix, &mut flow);
        let first = &out[0];
        assert_eq!(first.at, t0);
        assert!(
            (first.expected.value() - nowcast.value()).abs() < 1e-6,
            "prévision {} ≠ nowcast {}",
            first.expected.value(),
            nowcast.value()
        );
    }

    #[test]
    fn carbon_import_pattern_is_reflected() {
        // L'import nocturne est plus carboné (4000 MW vs 1000) → l'intensité @2
        // prévue de nuit dépasse celle de jour.
        let from = OffsetDateTime::UNIX_EPOCH + Duration::days(120);
        let step = Duration::hours(1);
        let (mix, flow) = histories(from, step, 14 * 24);
        let out = acv_ademe_forecast(
            &mix,
            &flow,
            from,
            Duration::hours(24),
            ClimatologyParams {
                step,
                tau: Duration::days(14),
            },
            &EmissionFactors::acv_ademe_v1(),
            0.072,
            None,
        )
        .unwrap();
        let day = out
            .iter()
            .find(|p| p.at.hour() == 6)
            .unwrap()
            .expected
            .value();
        let night = out
            .iter()
            .find(|p| p.at.hour() == 20)
            .unwrap()
            .expected
            .value();
        assert!(night > day, "nuit {night} devrait dépasser jour {day}");
    }
}
