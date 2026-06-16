//! Analyse-gate de la **prévision météo-pilotée** (ADR-0018, étape A) : avant de
//! construire un `forecast@N`, on mesure si l'**anomalie de production renouvelable**
//! explique l'**anomalie d'intensité** *au-delà de la climatologie*.
//!
//! Protocole honnête : climatologie horaire-de-semaine + coefficient `β` calés
//! sur 70 % (train), erreur mesurée sur 30 % (test, hors échantillon). On utilise
//! le renouvelable **réel** (borne supérieure : si même le renouvelable parfait
//! n'améliore pas la climatologie, la version *prévue* échouera a fortiori — cf.
//! l'ajustement de charge écarté en ADR-0011 §4).

use crate::domain::{ErrorAccumulator, ErrorMetrics, Region, TimeRange};
use crate::ports::IntensityRepository;

use super::ApplicationError;

/// Nombre de créneaux horaire-de-semaine (7 j × 24 h).
const SLOTS: usize = 168;

/// Bilan de l'analyse : erreur de la climatologie seule vs climatologie + `β·anomalie
/// de renouvelable`, sur l'échantillon de test.
#[derive(Debug, Clone, Copy)]
pub struct RenewableSignalReport {
    /// Erreur de la climatologie horaire-de-semaine seule.
    pub baseline: ErrorMetrics,
    /// Erreur après ajustement `β·(renouvelable − climatologie de renouvelable)`.
    pub adjusted: ErrorMetrics,
    /// Coefficient calé (gCO₂eq/kWh par MW de renouvelable au-dessus de la normale).
    pub beta: f64,
    pub train: usize,
    pub test: usize,
}

impl RenewableSignalReport {
    /// L'ajustement améliore-t-il la climatologie (RMSE strictement plus faible) ?
    pub fn improves(&self) -> bool {
        self.adjusted.rmse < self.baseline.rmse
    }
}

/// Mesure le pouvoir explicatif de l'anomalie de renouvelable sur l'intensité.
pub struct AnalyzeRenewableSignal<R: IntensityRepository> {
    repository: R,
}

struct Sample {
    slot: usize,
    intensity: f64,
    renewable: f64,
}

impl<R: IntensityRepository> AnalyzeRenewableSignal<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        range: TimeRange,
    ) -> Result<RenewableSignalReport, ApplicationError> {
        let measurements = self
            .repository
            .range(Region::National, "rte-direct", range)
            .await?;

        let mut samples: Vec<Sample> = Vec::new();
        for m in &measurements {
            let Some(mix) = m.mix.as_ref() else { continue };
            samples.push(Sample {
                slot: hour_of_week(m.at),
                intensity: m.intensity.value(),
                renewable: mix.eolien + mix.solaire,
            });
        }
        if samples.len() < 2 * SLOTS {
            return Err(ApplicationError::InsufficientSeries);
        }
        analyze(&samples).ok_or(ApplicationError::InsufficientSeries)
    }
}

/// Cœur **pur** de l'analyse : climatologie + `β` calés sur 70 %, erreur mesurée
/// sur 30 %. Extrait pour être testable sans IO.
fn analyze(samples: &[Sample]) -> Option<RenewableSignalReport> {
    // Découpe temporelle 70/30 (échantillons supposés triés par horodatage).
    let split = samples.len() * 7 / 10;
    let (train, test) = samples.split_at(split);

    // Climatologie horaire-de-semaine (moyennes par créneau) sur le train.
    let clim_intensity = slot_means(train, |s| s.intensity);
    let clim_renewable = slot_means(train, |s| s.renewable);

    // β = Σ(anom_int · anom_ren) / Σ(anom_ren²) sur le train (régression à l'origine).
    let (mut num, mut den) = (0.0, 0.0);
    for s in train {
        let (Some(ci), Some(cr)) = (clim_intensity[s.slot], clim_renewable[s.slot]) else {
            continue;
        };
        let (ai, ar) = (s.intensity - ci, s.renewable - cr);
        num += ai * ar;
        den += ar * ar;
    }
    let beta = if den > f64::EPSILON { num / den } else { 0.0 };

    // Évaluation hors échantillon : climatologie seule vs climatologie + β·anomalie.
    let (mut base, mut adj) = (ErrorAccumulator::default(), ErrorAccumulator::default());
    for s in test {
        let (Some(ci), Some(cr)) = (clim_intensity[s.slot], clim_renewable[s.slot]) else {
            continue;
        };
        base.observe(ci, s.intensity);
        adj.observe(ci + beta * (s.renewable - cr), s.intensity);
    }

    Some(RenewableSignalReport {
        baseline: base.metrics()?,
        adjusted: adj.metrics()?,
        beta,
        train: train.len(),
        test: test.len(),
    })
}

/// Créneau horaire-de-semaine ∈ [0, 168) : `jour_de_semaine × 24 + heure` (UTC).
fn hour_of_week(at: time::OffsetDateTime) -> usize {
    let wd = at.weekday().number_days_from_monday() as usize;
    wd * 24 + at.hour() as usize
}

/// Moyenne par créneau (`None` si créneau jamais observé).
fn slot_means(samples: &[Sample], f: impl Fn(&Sample) -> f64) -> [Option<f64>; SLOTS] {
    let mut sum = [0.0f64; SLOTS];
    let mut n = [0u32; SLOTS];
    for s in samples {
        sum[s.slot] += f(s);
        n[s.slot] += 1;
    }
    std::array::from_fn(|i| (n[i] > 0).then(|| sum[i] / n[i] as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_strong_signal() {
        // Vérité construite : intensité = climatologie(créneau) + K·renouvelable.
        // L'analyse DOIT détecter le signal (sinon le résultat « pas de signal »
        // sur la vraie donnée pourrait venir d'un bug). RMSE ajustée ≪ baseline.
        const K: f64 = -0.01;
        let mut samples = Vec::new();
        for week in 0..12 {
            let renewable = 1000.0 + 200.0 * week as f64; // varie par semaine
            for slot in 0..SLOTS {
                let clim = 40.0 + 0.05 * slot as f64;
                samples.push(Sample {
                    slot,
                    intensity: clim + K * renewable,
                    renewable,
                });
            }
        }
        let report = analyze(&samples).unwrap();
        assert!(report.improves(), "un signal fort doit être détecté");
        assert!(
            report.adjusted.rmse < report.baseline.rmse * 0.2,
            "l'ajustement doit réduire fortement l'erreur (baseline {}, ajustée {})",
            report.baseline.rmse,
            report.adjusted.rmse
        );
    }

    #[test]
    fn no_signal_when_renewable_is_irrelevant() {
        // Intensité indépendante du renouvelable → l'ajustement n'améliore pas
        // (β ≈ 0, RMSE ajustée ≈ baseline).
        let mut samples = Vec::new();
        for week in 0..12 {
            for slot in 0..SLOTS {
                samples.push(Sample {
                    slot,
                    intensity: 40.0 + 0.05 * slot as f64 + (week % 3) as f64, // bruit non lié
                    renewable: 1000.0 + 200.0 * week as f64,
                });
            }
        }
        let report = analyze(&samples).unwrap();
        assert!(report.beta.abs() < 0.01, "β doit être ~0 sans lien");
    }
}
