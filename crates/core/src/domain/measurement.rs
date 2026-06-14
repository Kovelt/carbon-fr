//! Mesure : l'unité d'observation du domaine.

use time::OffsetDateTime;

use crate::domain::{CarbonIntensity, Methodology, Region, Vintage};

/// Mix de production électrique au pas de la mesure, en **MW** par filière
/// (ADR-0003). Optionnel : une intensité peut être servie sans le détail du mix.
///
/// `echanges` est le solde net des échanges aux interconnexions (positif =
/// import). Il n'entre pas dans la méthodologie `rte-direct` (émissions de la
/// seule production FR), mais est porté pour la future méthode `acv-ademe`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenerationMix {
    pub nucleaire: f64,
    pub gaz: f64,
    pub charbon: f64,
    pub fioul: f64,
    pub hydraulique: f64,
    pub eolien: f64,
    pub solaire: f64,
    pub bioenergies: f64,
    pub pompage: f64,
    pub echanges: f64,
    /// Thermique fossile **agrégé**, renseigné quand la source ne détaille pas
    /// gaz/charbon/fioul (cas du mix régional ODRÉ). `None` au national, où le
    /// détail par filière est disponible (ADR-0008).
    pub thermique: Option<f64>,
}

/// Une mesure d'intensité carbone, horodatée et géolocalisée.
///
/// Les champs [`methodology`](Measurement::methodology) (ADR-0005) et
/// [`vintage`](Measurement::vintage) (ADR-0006) sont portés explicitement par
/// chaque mesure — il n'existe pas de méthodologie ni de millésime « global ».
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    /// Horodatage (début du pas quart d'heure).
    pub at: OffsetDateTime,
    pub region: Region,
    pub intensity: CarbonIntensity,
    pub methodology: Methodology,
    pub vintage: Vintage,
    pub mix: Option<GenerationMix>,
}

impl Measurement {
    /// Clé d'unicité `(region, horodatage, methodology)` (ADR-0006).
    pub fn key(&self) -> MeasurementKey {
        MeasurementKey {
            region: self.region,
            at: self.at,
            methodology: self.methodology.clone(),
        }
    }
}

/// Clé d'unicité d'une mesure : `(region, horodatage, methodology)`.
///
/// La méthodologie fait partie de la clé car deux méthodes produisent deux
/// valeurs distinctes pour le même instant (ADR-0005). Le millésime n'en fait
/// **pas** partie : il qualifie la révision d'une même mesure (ADR-0006).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeasurementKey {
    pub region: Region,
    pub at: OffsetDateTime,
    pub methodology: Methodology,
}
