//! Tests d'intégration contre un vrai PostgreSQL.
//!
//! Pilotés par la variable d'environnement `DATABASE_URL`. Sans elle, les tests
//! s'auto-sautent (message explicite) pour rester hermétiques par défaut :
//!
//! ```bash
//! export DATABASE_URL=postgres://localhost/carbonfr_test
//! cargo test -p carbonfr-adapter-postgres --test pg
//! ```
//!
//! Les tests s'exécutent en parallèle : chacun s'isole via une **méthodologie
//! dédiée** (la clé d'unicité et les requêtes filtrent dessus) et nettoie ses
//! propres lignes au démarrage, ce qui les rend ré-exécutables et sans
//! interférence mutuelle.

use carbonfr_adapter_postgres::PgIntensityRepository;
use carbonfr_core::domain::{
    CarbonIntensity, GenerationMix, Granularity, Measurement, Methodology, Region, TimeRange,
    Vintage,
};
use carbonfr_core::ports::IntensityRepository;
use time::{Duration, OffsetDateTime};

/// Repository prêt (migré, lignes de `methodology` purgées), ou `None` si
/// `DATABASE_URL` n'est pas défini (test sauté).
async fn setup(methodology: &str) -> Option<PgIntensityRepository> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("SKIP : DATABASE_URL non défini — test PostgreSQL sauté");
            return None;
        }
    };
    let repo = PgIntensityRepository::connect(&url)
        .await
        .expect("connexion PostgreSQL");
    repo.migrate().await.expect("migrations");
    sqlx::query("DELETE FROM measurement WHERE methodology_id = $1")
        .bind(methodology)
        .execute(repo.pool())
        .await
        .expect("nettoyage");
    Some(repo)
}

fn measurement(
    methodology: &str,
    at: OffsetDateTime,
    g: f64,
    vintage: Vintage,
    mix: Option<GenerationMix>,
) -> Measurement {
    Measurement {
        at,
        region: Region::National,
        intensity: CarbonIntensity::new(g).expect("intensité valide"),
        methodology: Methodology::new(methodology, 1),
        vintage,
        mix,
    }
}

fn sample_mix() -> GenerationMix {
    GenerationMix {
        nucleaire: 38815.0,
        gaz: 666.0,
        charbon: 0.0,
        fioul: 34.0,
        hydraulique: 8893.0,
        eolien: 2555.0,
        solaire: 1050.0,
        bioenergies: 1006.0,
        pompage: -76.0,
        echanges: -11574.0,
        thermique: None,
    }
}

#[tokio::test]
async fn conditional_upsert_respects_vintage_quality() {
    let m = "test-pg-upsert";
    let Some(repo) = setup(m).await else { return };
    let t = OffsetDateTime::UNIX_EPOCH;

    // Temps réel d'abord.
    assert_eq!(
        repo.upsert_many(&[measurement(m, t, 50.0, Vintage::Tr, None)])
            .await
            .unwrap(),
        1
    );

    // Le consolidé (meilleure qualité) remplace.
    assert_eq!(
        repo.upsert_many(&[measurement(m, t, 40.0, Vintage::Consolidated, None)])
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        repo.latest(Region::National, m)
            .await
            .unwrap()
            .unwrap()
            .intensity
            .value(),
        40.0
    );

    // Un temps réel tardif (qualité inférieure) ne doit PAS écraser : 0 ligne.
    assert_eq!(
        repo.upsert_many(&[measurement(m, t, 99.0, Vintage::Tr, None)])
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        repo.latest(Region::National, m)
            .await
            .unwrap()
            .unwrap()
            .intensity
            .value(),
        40.0
    );

    // Ré-upsert du même millésime (>=) : autorisé, rafraîchit la valeur.
    assert_eq!(
        repo.upsert_many(&[measurement(m, t, 41.0, Vintage::Consolidated, None)])
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn latest_absent_is_none_and_mix_roundtrips() {
    let m = "test-pg-mix";
    let Some(repo) = setup(m).await else { return };

    // Aucune donnée pour une méthodologie inconnue.
    assert!(
        repo.latest(Region::National, "inexistante")
            .await
            .unwrap()
            .is_none()
    );

    let t = OffsetDateTime::UNIX_EPOCH + Duration::days(365);
    repo.upsert_many(&[measurement(m, t, 15.0, Vintage::Tr, Some(sample_mix()))])
        .await
        .unwrap();

    let read = repo.latest(Region::National, m).await.unwrap().unwrap();
    let mix = read.mix.expect("mix présent après round-trip");
    assert_eq!(mix.nucleaire, 38815.0);
    assert_eq!(mix.echanges, -11574.0);
    assert_eq!(mix.pompage, -76.0);
}

#[tokio::test]
async fn range_returns_chronological_window() {
    let m = "test-pg-range";
    let Some(repo) = setup(m).await else { return };
    let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(1000);
    let step = Duration::minutes(15);

    let points: Vec<Measurement> = (0..5)
        .map(|i| measurement(m, t0 + step * i, 20.0 + i as f64, Vintage::Tr, None))
        .collect();
    repo.upsert_many(&points).await.unwrap();

    // Fenêtre couvrant les 3 premiers points.
    let range = TimeRange::new(t0, t0 + step * 3).unwrap();
    let got = repo.range(Region::National, m, range).await.unwrap();

    assert_eq!(got.len(), 3);
    assert!(got.windows(2).all(|w| w[0].at < w[1].at), "tri croissant");
    assert_eq!(got[0].at, t0);
    assert_eq!(got[2].at, t0 + step * 2);
}

#[tokio::test]
async fn stats_summary_and_hourly_rollup() {
    let m = "test-pg-stats";
    let Some(repo) = setup(m).await else { return };
    // Borne d'heure (UNIX_EPOCH + n jours) pour des seaux alignés.
    let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(2000);

    repo.upsert_many(&[
        measurement(m, t0, 10.0, Vintage::Tr, None),
        measurement(m, t0 + Duration::minutes(30), 20.0, Vintage::Tr, None),
        measurement(m, t0 + Duration::hours(1), 60.0, Vintage::Tr, None),
    ])
    .await
    .unwrap();
    repo.refresh_rollups().await.unwrap();

    let window = TimeRange::new(t0, t0 + Duration::hours(2)).unwrap();

    // Résumé exact (sur measurement) : moy 30, min 10, max 60.
    let summary = repo
        .stats(Region::National, m, window)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(summary.count, 3);
    assert_eq!(summary.average.value(), 30.0);
    assert_eq!(summary.min.value(), 10.0);
    assert_eq!(summary.max.value(), 60.0);

    // Rollup horaire (sur la vue matérialisée) : 2 seaux.
    let hourly = repo
        .rollup(Region::National, m, window, Granularity::Hourly)
        .await
        .unwrap();
    assert_eq!(hourly.len(), 2);
    assert_eq!(hourly[0].start, t0);
    assert_eq!(hourly[0].stats.average.value(), 15.0);
    assert_eq!(hourly[1].stats.average.value(), 60.0);

    // Intervalle vide → None.
    let empty = TimeRange::new(t0 - Duration::days(1), t0).unwrap();
    assert!(
        repo.stats(Region::National, m, empty)
            .await
            .unwrap()
            .is_none()
    );
}
