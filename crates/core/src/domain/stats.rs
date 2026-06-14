//! Statistiques agrégées d'intensité carbone (résumés et rollups).

use time::OffsetDateTime;

use crate::domain::CarbonIntensity;

/// Statistiques d'intensité sur un ensemble de mesures (gCO₂eq/kWh).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntensityStats {
    pub average: CarbonIntensity,
    pub min: CarbonIntensity,
    pub max: CarbonIntensity,
    /// Nombre de mesures agrégées.
    pub count: u64,
}

/// Un seau temporel d'un rollup : son début et les statistiques associées.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RollupBucket {
    /// Début du seau (aligné sur le pas, en UTC).
    pub start: OffsetDateTime,
    pub stats: IntensityStats,
}

/// Pas d'agrégation d'un rollup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Hourly,
    Daily,
}

impl Granularity {
    /// Étiquette stable (clé d'API, libellé de réponse).
    pub fn label(self) -> &'static str {
        match self {
            Granularity::Hourly => "hour",
            Granularity::Daily => "day",
        }
    }

    /// Pas correspondant à une étiquette d'API, ou `None` si inconnue.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "hour" | "hourly" => Some(Granularity::Hourly),
            "day" | "daily" => Some(Granularity::Daily),
            _ => None,
        }
    }
}
