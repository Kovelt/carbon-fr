//! Test d'intégration « live » contre l'API ODRÉ réelle.
//!
//! Ignoré par défaut (réseau, non hermétique). À lancer manuellement :
//!
//! ```bash
//! cargo test -p carbonfr-adapter-odre --test live -- --ignored
//! ```

use carbonfr_adapter_odre::OdreClient;
use carbonfr_core::domain::{Methodology, Region, TimeRange, Vintage};
use carbonfr_core::ports::{Eco2mixArchive, Eco2mixSource};
use time::{Date, Duration, Month, OffsetDateTime};

#[tokio::test]
#[ignore = "réseau : frappe l'API ODRÉ réelle"]
async fn latest_national_is_fetched() {
    let client = OdreClient::new().expect("client");
    let m = client
        .latest(Region::National)
        .await
        .expect("dernière mesure nationale");

    assert_eq!(m.region, Region::National);
    assert!(m.intensity.value() >= 0.0);
    assert!(m.mix.is_some(), "le mix de production doit être présent");
}

#[tokio::test]
#[ignore = "réseau : frappe l'API ODRÉ réelle"]
async fn range_national_returns_chronological_series() {
    let client = OdreClient::new().expect("client");
    let now = OffsetDateTime::now_utc();
    let range = TimeRange::new(now - Duration::hours(6), now).expect("intervalle valide");

    let series = client
        .range(Region::National, range)
        .await
        .expect("série nationale");

    assert!(!series.is_empty(), "6 h de national-tr ne peut être vide");
    // Tri chronologique croissant garanti par l'adapter (order_by asc).
    assert!(series.windows(2).all(|w| w[0].at <= w[1].at));
}

#[tokio::test]
#[ignore = "réseau : frappe l'API ODRÉ réelle"]
async fn export_national_window_is_fetched() {
    let client = OdreClient::new().expect("client");
    // Un jour d'historique consolidé/définitif (donnée stable et passée).
    let start = Date::from_calendar_date(2024, Month::January, 1)
        .unwrap()
        .midnight()
        .assume_utc();
    let range = TimeRange::new(start, start + Duration::days(1)).expect("intervalle valide");

    let measurements = client
        .export_national(range)
        .await
        .expect("export national");

    assert!(
        !measurements.is_empty(),
        "un jour de cons-def ne peut être vide"
    );
    assert!(measurements.iter().all(|m| m.region == Region::National));
    // L'historique cons-def ne porte que des millésimes consolidés/définitifs.
    assert!(
        measurements
            .iter()
            .all(|m| matches!(m.vintage, Vintage::Consolidated | Vintage::Definitive))
    );
}

#[tokio::test]
#[ignore = "réseau : frappe l'API ODRÉ réelle"]
async fn latest_regional_is_derived_acv_ademe() {
    let client = OdreClient::new().expect("client");
    let m = client
        .latest(Region::Bretagne)
        .await
        .expect("dernière mesure régionale");

    assert_eq!(m.region, Region::Bretagne);
    // Pas de taux_co2 régional → intensité dérivée acv-ademe.
    assert_eq!(m.methodology, Methodology::acv_ademe());
    // Le mix régional porte le thermique agrégé.
    assert!(m.mix.expect("mix").thermique.is_some());
}
