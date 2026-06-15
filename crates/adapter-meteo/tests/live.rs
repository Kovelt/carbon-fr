//! Test « live » contre l'API réelle Open-Meteo. `#[ignore]` par défaut (réseau)
//! :
//!
//! ```bash
//! cargo test -p carbonfr-adapter-meteo --test live -- --ignored
//! ```

use carbonfr_adapter_meteo::OpenMeteoClient;
use carbonfr_core::ports::WeatherForecastSource;

#[tokio::test]
#[ignore = "réseau : API Open-Meteo réelle"]
async fn fetches_real_national_forecast() {
    let client = OpenMeteoClient::new().expect("client");
    let forecast = client.current_forecast().await.expect("prévision");

    // ~48 h de pas horaire attendus, valeurs plausibles.
    assert!(forecast.len() >= 24, "n = {}", forecast.len());
    assert!(forecast.iter().all(|f| f.wind >= 0.0 && f.wind < 300.0));
    assert!(
        forecast
            .iter()
            .all(|f| f.irradiance >= 0.0 && f.irradiance < 1500.0)
    );
    // Horodatages strictement croissants.
    assert!(forecast.windows(2).all(|w| w[0].valid_at < w[1].valid_at));
    // Tous issus du même run.
    let run = forecast[0].run_at;
    assert!(forecast.iter().all(|f| f.run_at == run));
}
