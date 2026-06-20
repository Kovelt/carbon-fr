//! Test d'intégration **live** ENTSO-E — `--ignored` (réseau + token requis).
//!
//! ```bash
//! CARBONFR_ENTSOE_TOKEN=<token> \
//!   cargo test -p carbonfr-adapter-entsoe --test live -- --ignored --nocapture
//! ```
//!
//! Valide les chemins XML et codes contre l'API réelle : on attend au moins un
//! snapshot d'import avec des flux et des intensités voisines plausibles.

use carbonfr_adapter_entsoe::EntsoeClient;
use carbonfr_core::ports::{CrossBorderSource, SpotPriceSource};

#[tokio::test]
#[ignore = "réseau + CARBONFR_ENTSOE_TOKEN requis"]
async fn recent_flows_live() {
    let client = EntsoeClient::from_env().expect("CARBONFR_ENTSOE_TOKEN requis");
    let snapshots = client.recent_flows().await.expect("appel ENTSO-E");

    assert!(!snapshots.is_empty(), "aucun snapshot d'import récupéré");
    let last = snapshots.last().unwrap();
    eprintln!(
        "snapshot {} — {} frontières",
        last.at,
        last.flows.flows.len()
    );
    for f in &last.flows.flows {
        eprintln!(
            "  {:?}: flux {:.0} MW, intensité voisin {:.0} gCO2eq/kWh",
            f.neighbor,
            f.flow_mw,
            f.neighbor_intensity.value()
        );
        // Intensité voisine plausible : entre 0 et 1200 (mix charbon pur ~1000).
        assert!(f.neighbor_intensity.value() >= 0.0);
        assert!(f.neighbor_intensity.value() < 1200.0);
    }
}

#[tokio::test]
#[ignore = "réseau + CARBONFR_ENTSOE_TOKEN requis"]
async fn recent_prices_live() {
    let client = EntsoeClient::from_env().expect("CARBONFR_ENTSOE_TOKEN requis");
    let prices = client.recent_prices().await.expect("appel ENTSO-E (A44)");

    assert!(!prices.is_empty(), "aucun prix spot day-ahead récupéré");
    for p in prices.iter().take(5) {
        eprintln!("prix {} — {:.2} €/MWh", p.at, p.eur_per_mwh);
    }
    // Plausibilité : prix de gros FR borné (peut être négatif, < 4000 €/MWh cap).
    for p in &prices {
        assert!(p.eur_per_mwh > -1000.0 && p.eur_per_mwh < 5000.0);
    }
}
