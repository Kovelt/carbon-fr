//! `cargo run -p carbonfr-sdk --example stream --features stream` — flux
//! **live** (Server-Sent Events) des mises à jour d'intensité
//! (`GET /v1/intensity/stream`) : affiche les 3 premiers événements reçus
//! puis s'arrête. Aucun timeout total (voir [`carbonfr_sdk::IntensityStream`]) —
//! sur l'instance hébergée par défaut, dont le poller pousse par défaut
//! toutes les 15 minutes, ceci peut mettre du temps à afficher quoi que ce
//! soit : c'est le comportement normal du flux, pas un blocage du SDK.

use carbonfr_sdk::{CarbonFr, IntensityStreamOptions};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = CarbonFr::builder().build()?;
    let mut stream = client.intensity_stream(IntensityStreamOptions::default())?;

    eprintln!("en écoute de /v1/intensity/stream — 3 événements puis arrêt…");
    let mut received = 0;
    while let Some(event) = stream.next().await {
        match event {
            Ok(event) => {
                println!(
                    "{} {} — {:.1} {}",
                    event.timestamp, event.region, event.intensity, event.unit
                );
                received += 1;
                if received >= 3 {
                    break;
                }
            }
            // Une reconnexion réussie ne produit jamais d'`Err` intermédiaire
            // (silencieuse) : n'atteindre cette branche signale que la
            // reconnexion a été désactivée ou a fini par abandonner.
            Err(err) => {
                eprintln!("flux interrompu : {err}");
                break;
            }
        }
    }
    Ok(())
}
