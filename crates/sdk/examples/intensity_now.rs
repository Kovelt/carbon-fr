//! `cargo run -p carbonfr-sdk --example intensity_now` — dernière intensité
//! carbone nationale connue, contre l'instance hébergée par défaut
//! (`CarbonFr::builder()` sans `base_url` → [`carbonfr_sdk::DEFAULT_BASE_URL`]).

use carbonfr_sdk::{CarbonFr, IntensityNowOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = CarbonFr::builder().build()?;
    let now = client.intensity_now(IntensityNowOptions::default()).await?;

    println!(
        "{} — {:.1} {} ({}, version {}, vintage {})",
        now.region,
        now.intensity.value,
        now.intensity.unit,
        now.methodology,
        now.methodology_version,
        now.vintage,
    );
    Ok(())
}
