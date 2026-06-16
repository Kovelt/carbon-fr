//! Événement de mise à jour du read-model (ADR-0014 §2).
//!
//! Valeur **légère** diffusée en push (SSE) à chaque nouvelle mesure ingérée :
//! juste ce qu'un client live a besoin de savoir, sans le mix complet. Pur — la
//! diffusion (canal, sérialisation) est une préoccupation d'adapter.

use time::OffsetDateTime;

use crate::domain::{CarbonIntensity, Measurement, Methodology, Region};

/// Une nouvelle observation d'intensité, prête à être poussée aux abonnés live.
#[derive(Debug, Clone, PartialEq)]
pub struct IntensityUpdate {
    pub region: Region,
    pub at: OffsetDateTime,
    pub intensity: CarbonIntensity,
    pub methodology: Methodology,
}

impl IntensityUpdate {
    /// Projette une mesure en événement (sans le mix).
    pub fn from_measurement(m: &Measurement) -> Self {
        Self {
            region: m.region,
            at: m.at,
            intensity: m.intensity,
            methodology: m.methodology.clone(),
        }
    }
}
