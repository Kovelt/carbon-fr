//! `cargo run -p carbonfr-sdk --example api_error` — gestion typée d'une
//! erreur API (RFC 9457) : demande volontairement une région invalide pour
//! illustrer le `match` sur [`carbonfr_sdk::CarbonFrError`] et son ancrage
//! machine stable, `CarbonFrError::code()`.

use carbonfr_sdk::{CarbonFr, CarbonFrError, IntensityNowOptions, Region};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = CarbonFr::builder().build()?;

    // `Region` est `#[non_exhaustive]` : on construit un slug non reconnu via
    // `Region::from` (jamais `Region::Other(..)` en littéral hors du crate),
    // exactement le chemin qu'emprunterait un futur slug serveur inconnu du
    // SDK.
    let result = client
        .intensity_now(IntensityNowOptions {
            region: Some(Region::from("pas-une-region")),
            ..Default::default()
        })
        .await;

    match result {
        Ok(now) => println!(
            "inattendu : succès malgré une région invalide ({} — {:.1} {})",
            now.region, now.intensity.value, now.intensity.unit
        ),
        Err(CarbonFrError::Api {
            status, problem, ..
        }) => {
            // `problem.code()` est l'ancrage machine STABLE à matcher — pas
            // `problem.detail()`, message humain non contractuel.
            println!(
                "erreur API {status} — code={:?} detail={:?}",
                problem.code(),
                problem.detail(),
            );
        }
        Err(CarbonFrError::Transport(e)) => println!("erreur de transport : {e}"),
        // `CarbonFrError` est `#[non_exhaustive]` : un bras `_`/`other` est
        // obligatoire, pas seulement une bonne pratique.
        Err(other) => println!("autre erreur ({}) : {other}", other.code().unwrap_or("?")),
    }
    Ok(())
}
