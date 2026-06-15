//! Le domaine : types métier purs, sans aucune IO ni dépendance infra
//! (règle d'or, ADR-0002).
//!
//! Tout le vocabulaire du projet vit ici — intensité carbone, régions, mesures,
//! méthodologie, millésime — ainsi que la logique métier qui ne dépend que de
//! ces types (ex. [`greenest_window`]).

mod factors;
mod forecast;
mod intensity;
mod measurement;
mod methodology;
mod region;
mod stats;
mod time_range;
mod visit;
mod window;

pub use factors::{EmissionFactors, acv_ademe_intensity, derive_acv_ademe};
pub use forecast::{CLIMATOLOGY_ID, CLIMATOLOGY_VERSION, ClimatologyParams, climatology_forecast};
pub use intensity::CarbonIntensity;
pub use measurement::{GenerationMix, Measurement, MeasurementKey};
pub use methodology::{Methodology, Vintage};
pub use region::Region;
pub use stats::{Granularity, IntensityStats, RollupBucket};
pub use time_range::TimeRange;
pub use visit::VisitStats;
pub use window::{GreenWindow, greenest_window};
