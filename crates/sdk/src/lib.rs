//! # carbonfr-sdk
//!
//! Client Rust **asynchrone** (tokio) de l'API [carbon-fr](https://github.com/Kovelt/carbon-fr)
//! — intensité carbone de l'électricité française (gCO₂eq/kWh), temps réel et
//! historique.
//!
//! ```no_run
//! # #[cfg(feature = "stream")]
//! # async fn go() -> Result<(), carbonfr_sdk::CarbonFrError> {
//! use carbonfr_sdk::{CarbonFr, IntensityStreamOptions};
//! use futures_util::StreamExt;
//!
//! let client = CarbonFr::builder().build()?;
//! let mut stream = client.intensity_stream(IntensityStreamOptions::default())?;
//! while let Some(event) = stream.next().await {
//!     println!("{:?}", event?);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Couverture
//!
//! **Parité avec les 26 opérations `/v1`** du SDK TypeScript 0.2.0
//! (ADR-0031 décision 8 ; `health`/`health_ready`, d'exploitation, ne sont pas
//! exposées) : construction du client ([`CarbonFr`]/[`CarbonFrBuilder`]),
//! erreurs typées ([`CarbonFrError`], RFC 9457), 25 méthodes REST (ex.
//! [`CarbonFr::intensity_now`], [`CarbonFr::mix`], [`CarbonFr::schedule`],
//! [`CarbonFr::create_webhook`]…) et le flux SSE `CarbonFr::intensity_stream`
//! (feature Cargo `stream`). Exemples exécutables dans `examples/` du dépôt.
//!
//! ## Construction TLS (ADR-0031 décision 3)
//!
//! `reqwest` 0.13 sans provider crypto installé **panique** à la construction
//! d'un client. Ce SDK ne s'appuie **jamais** sur un provider par défaut du
//! **processus** (`rustls::crypto::CryptoProvider::get_default()` /
//! `install_default()`, utilisés par `bin/server` et les adapters de ce
//! dépôt) : il construit son propre `rustls::ClientConfig` avec le provider
//! `ring` passé **explicitement**, et le donne à `reqwest` via
//! `ClientBuilder::tls_backend_preconfigured`. Utilisable tel quel dans un
//! process qui n'installe aucun provider par défaut — voir
//! `client::tests::crypto_provider_default_stays_uninstalled`.

mod client;
mod dto;
mod enums;
mod error;
mod methods;
mod options;
mod query;
mod region;
#[cfg(feature = "stream")]
mod stream;

pub use client::{
    CarbonFr, CarbonFrBuilder, DEFAULT_BASE_URL, DEFAULT_CONNECT_TIMEOUT, DEFAULT_TIMEOUT,
};
pub use dto::*;
pub use enums::{
    EligibilityFramework, EligibilitySignalProvenance, EligibilitySignalVerdict, Estimator,
    FlowDirection, Interval, Methodology, ThresholdDirection, WebhookStatus,
};
pub use error::{CarbonFrError, ProblemDetails};
pub use options::*;
pub use region::Region;
#[cfg(feature = "stream")]
pub use stream::{
    DEFAULT_IDLE_TIMEOUT, IntensityEvent, IntensityStream, IntensityStreamOptions, Reconnect,
    StreamConfig,
};
