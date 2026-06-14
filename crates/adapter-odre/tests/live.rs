//! Test d'intégration « live » contre l'API ODRÉ réelle.
//!
//! Ignoré par défaut (réseau, non hermétique). À lancer manuellement :
//!
//! ```bash
//! cargo test -p carbonfr-adapter-odre --test live -- --ignored
//! ```

use carbonfr_adapter_odre::OdreClient;
use carbonfr_core::domain::{Region, TimeRange};
use carbonfr_core::ports::Eco2mixSource;
use time::{Duration, OffsetDateTime};

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
