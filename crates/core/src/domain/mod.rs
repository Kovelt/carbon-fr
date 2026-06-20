//! Le domaine : types métier purs, sans aucune IO ni dépendance infra
//! (règle d'or, ADR-0002).
//!
//! Tout le vocabulaire du projet vit ici — intensité carbone, régions, mesures,
//! méthodologie, millésime — ainsi que la logique métier qui ne dépend que de
//! ces types (ex. [`greenest_window`]).

mod calculator;
mod cost;
mod cross_border;
mod factors;
mod forecast;
mod forecast_acv;
mod forecast_point;
mod horizon_bands;
mod intensity;
mod load;
mod measurement;
mod methodology;
mod metrics;
mod price;
mod region;
mod renewable;
mod schedule;
mod stats;
mod time_range;
mod update;
mod visit;
mod weather;
mod webhook;
mod window;

pub use calculator::{
    AcvAdemeConsumption, AcvAdemeProduction, MethodologyCalculator, MethodologyContext, RteDirect,
};
pub use cost::{
    COST_REFERENCE_DISCLAIMER, CostAssumptions, CostBasis, CostEstimate, CostReferenceCatalog,
    CostReferenceKey, CostSource, CostTechnology, LcoeRange, Perimeter, cost_reference_catalog,
};
pub use cross_border::{CrossBorderFlow, CrossBorderFlows, CrossBorderSnapshot, Neighbor};
pub use factors::{
    EmissionFactors, TD_LOSS_FACTOR_V1, acv_ademe_consumption_intensity, acv_ademe_intensity,
    derive_acv_ademe, derive_consumption_series,
};
pub use forecast::{CLIMATOLOGY_ID, CLIMATOLOGY_VERSION, ClimatologyParams, climatology_forecast};
pub use forecast_acv::{ACV_FORECAST_ID, ACV_FORECAST_VERSION, acv_ademe_forecast};
pub use forecast_point::{ForecastPoint, ModelVersion};
pub use horizon_bands::HorizonBands;
pub use intensity::CarbonIntensity;
pub use load::LoadRecord;
pub use measurement::{GenerationMix, Measurement, MeasurementKey};
pub use methodology::{Methodology, Vintage};
pub use metrics::{ErrorAccumulator, ErrorMetrics};
pub use price::{
    Filiere, MarginalTechnology, MixShare, PriceBreakdown, PriceComponent, PriceComponentKind,
    PriceContext, SpotPrice, TrvReference, price_breakdown, price_series,
};
pub use region::Region;
pub use renewable::{RenewableModel, RenewableSample, calibrate as calibrate_renewable};
pub use schedule::{
    Savings, ScheduleSlot, greenest_window_before, lowest_slots, savings_vs_now, slots_below,
};
pub use stats::{Granularity, IntensityStats, RollupBucket, bucketize, summarize};
pub use time_range::TimeRange;
pub use update::IntensityUpdate;
pub use visit::VisitStats;
pub use weather::WeatherForecast;
pub use webhook::{
    Subscription, ThresholdDirection, WebhookUrlError, hmac_sha256_hex, is_public_ip,
    render_webhook_payload, should_fire, validate_webhook_url, webhook_host,
};
pub use window::{GreenWindow, WindowEstimator, greenest_window};
