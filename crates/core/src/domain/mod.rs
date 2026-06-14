//! Le domaine : types métier purs, sans aucune IO ni dépendance infra
//! (règle d'or, ADR-0002).
//!
//! Tout le vocabulaire du projet vit ici — intensité carbone, régions, mesures,
//! méthodologie, millésime — ainsi que la logique métier qui ne dépend que de
//! ces types (ex. [`greenest_window`]).

mod intensity;
mod measurement;
mod methodology;
mod region;
mod time_range;
mod window;

pub use intensity::CarbonIntensity;
pub use measurement::{GenerationMix, Measurement, MeasurementKey};
pub use methodology::{Methodology, Vintage};
pub use region::Region;
pub use time_range::TimeRange;
pub use window::{GreenWindow, greenest_window};
