//! # carbonfr-adapter-meteo
//!
//! Adapter **sortant** : prévision météo nationale via [Open-Meteo](https://open-meteo.com/)
//! (vent à 100 m, rayonnement solaire), agrégée sur quelques points de France
//! métropolitaine. Entrée du futur modèle ML (ADR-0012).
//!
//! Open-Meteo est **sans clé** et FR/EU — choix pragmatique de première brique
//! (l'ADR le retient en repli). Les prévisions de génération vent/solaire
//! d'ENTSO-E (souveraines, mais sous token) viendront **derrière le même port**
//! [`WeatherForecastSource`], sans toucher au domaine.
//!
//! Conformément au quota (ADR-0003), un **poller unique** appelle cet adapter.

mod dto;

use async_trait::async_trait;
use carbonfr_core::domain::WeatherForecast;
use carbonfr_core::ports::{SourceError, WeatherForecastSource};
use time::OffsetDateTime;

use dto::OpenMeteoResponse;

/// URL de base de l'API Open-Meteo.
const DEFAULT_BASE_URL: &str = "https://api.open-meteo.com";

/// Points (lat, lon) couvrant la France métropolitaine — moyennés pour un
/// agrégat « national » du vent et de l'irradiance.
const POINTS: &[(f64, f64)] = &[
    (48.85, 2.35),  // Paris
    (50.63, 3.06),  // Lille
    (48.58, 7.75),  // Strasbourg
    (45.76, 4.83),  // Lyon
    (43.30, 5.37),  // Marseille
    (44.84, -0.58), // Bordeaux
    (47.22, -1.55), // Nantes
];

/// Client de prévision météo (Open-Meteo).
#[derive(Clone)]
pub struct OpenMeteoClient {
    http: reqwest::Client,
    base_url: String,
}

impl OpenMeteoClient {
    /// Construit un client visant l'API publique d'Open-Meteo.
    pub fn new() -> Result<Self, SourceError> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("carbon-fr/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| SourceError::Unavailable(format!("construction du client HTTP : {e}")))?;
        Ok(Self::with_http(http, DEFAULT_BASE_URL))
    }

    /// Construit un client à partir d'un [`reqwest::Client`] et d'une URL de base
    /// explicites — utile pour pointer vers un serveur factice en test.
    pub fn with_http(http: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            http,
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
impl WeatherForecastSource for OpenMeteoClient {
    async fn current_forecast(&self) -> Result<Vec<WeatherForecast>, SourceError> {
        let run_at = OffsetDateTime::now_utc();
        let join = |f: fn(&(f64, f64)) -> f64| {
            POINTS
                .iter()
                .map(|p| f(p).to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let url = format!("{}/v1/forecast", self.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .get(url)
            .query(&[
                ("latitude", join(|p| p.0)),
                ("longitude", join(|p| p.1)),
                ("hourly", "wind_speed_100m,shortwave_radiation".to_string()),
                ("forecast_days", "2".to_string()),
                ("timezone", "UTC".to_string()),
            ])
            .send()
            .await
            .map_err(|e| SourceError::Unavailable(format!("requête Open-Meteo : {e}")))?;

        if !resp.status().is_success() {
            return Err(SourceError::Unavailable(format!(
                "Open-Meteo a répondu {}",
                resp.status()
            )));
        }

        // Plusieurs points → tableau de réponses (une par point).
        let bodies: Vec<OpenMeteoResponse> = resp
            .json()
            .await
            .map_err(|e| SourceError::Invalid(format!("réponse Open-Meteo illisible : {e}")))?;

        dto::aggregate_national(run_at, &bodies)
    }
}
