//! Charge électrique (consommation) — réalisée et **prévue** par RTE.
//!
//! La consommation **n'est pas du carbone** : elle ne vit pas dans
//! [`Measurement`](super::Measurement). C'est un signal d'entrée du modèle de
//! prévision ajusté par la charge (`climatology@2`, ADR-0011 §4) : la charge
//! anticipée prédit le recours au thermique à la marge. La **prévision** de
//! charge (J-1 / J, publiée par RTE pour des créneaux futurs) ne peut pas être
//! une mesure (pas d'intensité) — d'où ce type et son store dédié.

use time::OffsetDateTime;

use crate::domain::Region;

/// Charge à un instant : réalisée et/ou prévue (MW). Les deux sont optionnelles
/// (un créneau futur n'a que la prévision ; un créneau passé, la réalisée).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadRecord {
    pub at: OffsetDateTime,
    pub region: Region,
    /// Consommation réalisée (MW), si connue.
    pub realized: Option<f64>,
    /// Consommation prévue (MW) — prévision RTE J-1/J, si disponible.
    pub forecast: Option<f64>,
}

impl LoadRecord {
    /// Crée un enregistrement réalisé seul.
    pub fn realized(at: OffsetDateTime, region: Region, mw: f64) -> Self {
        Self {
            at,
            region,
            realized: Some(mw),
            forecast: None,
        }
    }

    /// Crée un enregistrement prévu seul.
    pub fn forecast(at: OffsetDateTime, region: Region, mw: f64) -> Self {
        Self {
            at,
            region,
            realized: None,
            forecast: Some(mw),
        }
    }
}
