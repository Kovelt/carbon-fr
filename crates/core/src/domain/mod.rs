//! Le domaine : types métier purs, sans aucune IO ni dépendance infra
//! (règle d'or, ADR-0002).
//!
//! Tout le vocabulaire du projet vit ici — intensité carbone, régions, mesures,
//! méthodologie, millésime — ainsi que la logique métier qui ne dépend que de
//! ces types (ex. [`greenest_window`]).

mod calculator;
mod cross_border;
mod factors;
mod forecast;
mod forecast_point;
mod horizon_bands;
mod intensity;
mod load;
mod measurement;
mod methodology;
mod metrics;
mod region;
mod stats;
mod time_range;
mod visit;
mod weather;
mod window;

pub use calculator::{
    AcvAdemeConsumption, AcvAdemeProduction, MethodologyCalculator, MethodologyContext, RteDirect,
};
pub use cross_border::{CrossBorderFlow, CrossBorderFlows, Neighbor};
pub use factors::{
    EmissionFactors, TD_LOSS_FACTOR_V1, acv_ademe_consumption_intensity, acv_ademe_intensity,
    derive_acv_ademe,
};
pub use forecast::{CLIMATOLOGY_ID, CLIMATOLOGY_VERSION, ClimatologyParams, climatology_forecast};
pub use forecast_point::{ForecastPoint, ModelVersion};
pub use horizon_bands::HorizonBands;
pub use intensity::CarbonIntensity;
pub use load::LoadRecord;
pub use measurement::{GenerationMix, Measurement, MeasurementKey};
pub use methodology::{Methodology, Vintage};
pub use metrics::{ErrorAccumulator, ErrorMetrics};
pub use region::Region;
pub use stats::{Granularity, IntensityStats, RollupBucket};
pub use time_range::TimeRange;
pub use visit::VisitStats;
pub use weather::WeatherForecast;
pub use window::{GreenWindow, WindowEstimator, greenest_window};
