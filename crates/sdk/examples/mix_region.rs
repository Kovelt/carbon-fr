//! `cargo run -p carbonfr-sdk --example mix_region` — mix de production
//! d'une région donnée (Bretagne ici), illustre un paramètre de requête
//! typé ([`Region`]) via un struct d'options.

use carbonfr_sdk::{CarbonFr, MixOptions, Region};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = CarbonFr::builder().build()?;
    let mix = client
        .mix(MixOptions {
            region: Some(Region::Bretagne),
            ..Default::default()
        })
        .await?;

    println!("mix {} au {} ({}) :", mix.region, mix.timestamp, mix.unit);
    println!("  nucléaire     {:>10.1}", mix.mix.nucleaire);
    println!("  gaz           {:>10.1}", mix.mix.gaz);
    println!("  charbon       {:>10.1}", mix.mix.charbon);
    println!("  fioul         {:>10.1}", mix.mix.fioul);
    println!("  hydraulique   {:>10.1}", mix.mix.hydraulique);
    println!("  éolien        {:>10.1}", mix.mix.eolien);
    println!("  solaire       {:>10.1}", mix.mix.solaire);
    println!("  bioénergies   {:>10.1}", mix.mix.bioenergies);
    println!("  pompage       {:>10.1}", mix.mix.pompage);
    println!("  échanges      {:>10.1}", mix.mix.echanges);
    // `thermique` : uniquement pour le mix régional (agrégat fossile), absent
    // au national — voir `carbonfr_sdk::MixBody`.
    if let Some(thermique) = mix.mix.thermique {
        println!("  thermique     {thermique:>10.1}  (agrégat fossile régional)");
    }
    Ok(())
}
