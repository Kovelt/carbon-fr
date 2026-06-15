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
use carbonfr_core::ports::{IntensityRepository, VisitCounter};
use time::{Date, Duration, Month, OffsetDateTime};

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
    measurement_in(Region::National, methodology, at, g, vintage, mix)
}

/// Comme [`measurement`], mais pour une région arbitraire (clé d'unicité =
/// region + at + methodology).
fn measurement_in(
    region: Region,
    methodology: &str,
    at: OffsetDateTime,
    g: f64,
    vintage: Vintage,
    mix: Option<GenerationMix>,
) -> Measurement {
    Measurement {
        at,
        region,
        intensity: CarbonIntensity::new(g).expect("intensité valide"),
        methodology: Methodology::new(methodology, 1),
        vintage,
        mix,
    }
}

/// Mix régional façon éCO2mix régional : le fossile est agrégé en `thermique`,
/// le détail gaz/charbon/fioul est à zéro (cf. addendum ADR-0003).
fn regional_mix(thermique: f64) -> GenerationMix {
    GenerationMix {
        nucleaire: 0.0,
        gaz: 0.0,
        charbon: 0.0,
        fioul: 0.0,
        hydraulique: 1200.0,
        eolien: 800.0,
        solaire: 300.0,
        bioenergies: 150.0,
        pompage: 0.0,
        echanges: 0.0,
        thermique: Some(thermique),
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

#[tokio::test]
async fn visit_counter_dedups_per_day() {
    // Pas de méthodologie ici, mais on réutilise setup pour migrer/connecter.
    let Some(repo) = setup("test-pg-visit").await else {
        return;
    };
    // Le compteur est global → on repart d'une table propre (aucun autre test
    // n'écrit dans `visit`).
    sqlx::query("DELETE FROM visit")
        .execute(repo.pool())
        .await
        .expect("nettoyage visit");

    let day = Date::from_calendar_date(2026, Month::June, 15).unwrap();

    // Même visiteur, même jour → compté une fois.
    repo.record_visit("hash-a", day).await.unwrap();
    let stats = repo.record_visit("hash-a", day).await.unwrap();
    assert_eq!(stats.unique, 1);
    assert_eq!(stats.total, 1);
    assert_eq!(stats.since, Some(day));

    // Un autre visiteur → 2 uniques.
    let stats = repo.record_visit("hash-b", day).await.unwrap();
    assert_eq!(stats.unique, 2);
    assert_eq!(stats.total, 2);
}

/// Deux lignes de **même clé** dans un **seul** `upsert_many` : sans la dédup
/// (`dedup_by_key`), PostgreSQL refuserait (« ON CONFLICT ne peut affecter deux
/// fois la même ligne »). On vérifie que le batch passe, ne compte qu'une ligne
/// et conserve le meilleur millésime — quel que soit l'ordre d'entrée.
#[tokio::test]
async fn upsert_dedups_same_key_within_one_batch() {
    let m = "test-pg-batch-dedup";
    let Some(repo) = setup(m).await else { return };
    let t = OffsetDateTime::UNIX_EPOCH + Duration::days(500);

    // Le moins bon en premier, le meilleur ensuite : un seul survivant.
    let written = repo
        .upsert_many(&[
            measurement(m, t, 50.0, Vintage::Tr, None),
            measurement(m, t, 40.0, Vintage::Consolidated, None),
        ])
        .await
        .unwrap();
    assert_eq!(written, 1, "même clé → une seule ligne écrite");
    assert_eq!(
        repo.latest(Region::National, m)
            .await
            .unwrap()
            .unwrap()
            .intensity
            .value(),
        40.0,
        "le meilleur millésime du batch est conservé"
    );
}

/// L'upsert découpe en paquets de `UPSERT_CHUNK` (1000) lignes au sein d'une
/// transaction. On franchit la borne (1001 lignes distinctes) pour prouver que
/// le découpage, la transaction multi-paquets et la somme de `written`
/// fonctionnent — c'est le chemin du backfill (~494k lignes).
#[tokio::test]
async fn upsert_spans_multiple_chunks() {
    let m = "test-pg-chunks";
    let Some(repo) = setup(m).await else { return };
    let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(3000);
    let step = Duration::minutes(15);

    let n: i32 = 1001; // > UPSERT_CHUNK ⇒ au moins deux paquets.
    let points: Vec<Measurement> = (0..n)
        .map(|i| measurement(m, t0 + step * i, 30.0, Vintage::Tr, None))
        .collect();

    let written = repo.upsert_many(&points).await.unwrap();
    assert_eq!(
        written as i32, n,
        "toutes les lignes des deux paquets écrites"
    );

    let range = TimeRange::new(t0, t0 + step * n).unwrap();
    let got = repo.range(Region::National, m, range).await.unwrap();
    assert_eq!(got.len() as i32, n, "relecture complète après commit");
}

/// La clé d'unicité inclut la région : même horodatage et même méthodologie sur
/// deux régions distinctes coexistent sans collision, et `latest`/`range`
/// filtrent bien par région.
#[tokio::test]
async fn distinct_regions_coexist_at_same_timestamp() {
    let m = "test-pg-region-iso";
    let Some(repo) = setup(m).await else { return };
    let t = OffsetDateTime::UNIX_EPOCH + Duration::days(4000);

    repo.upsert_many(&[
        measurement_in(Region::National, m, t, 100.0, Vintage::Tr, None),
        measurement_in(Region::Bretagne, m, t, 20.0, Vintage::Tr, None),
    ])
    .await
    .unwrap();

    assert_eq!(
        repo.latest(Region::National, m)
            .await
            .unwrap()
            .unwrap()
            .intensity
            .value(),
        100.0
    );
    assert_eq!(
        repo.latest(Region::Bretagne, m)
            .await
            .unwrap()
            .unwrap()
            .intensity
            .value(),
        20.0
    );

    let window = TimeRange::new(t, t + Duration::minutes(15)).unwrap();
    assert_eq!(
        repo.range(Region::National, m, window).await.unwrap().len(),
        1
    );
    assert_eq!(
        repo.range(Region::Bretagne, m, window).await.unwrap().len(),
        1
    );
}

/// Le mix régional agrège le fossile en `thermique` (colonne optionnelle,
/// migration 0003). On vérifie qu'il fait l'aller-retour, distinct du mix
/// national (où `thermique` reste `None`).
#[tokio::test]
async fn regional_thermique_mix_roundtrips() {
    let m = "test-pg-thermique";
    let Some(repo) = setup(m).await else { return };
    let t = OffsetDateTime::UNIX_EPOCH + Duration::days(4500);

    repo.upsert_many(&[
        measurement_in(
            Region::Bretagne,
            m,
            t,
            45.0,
            Vintage::Tr,
            Some(regional_mix(640.0)),
        ),
        measurement_in(
            Region::National,
            m,
            t,
            55.0,
            Vintage::Tr,
            Some(sample_mix()),
        ),
    ])
    .await
    .unwrap();

    let regional = repo.latest(Region::Bretagne, m).await.unwrap().unwrap();
    let mix = regional.mix.expect("mix régional présent");
    assert_eq!(mix.thermique, Some(640.0), "thermique agrégé restitué");
    assert_eq!(mix.hydraulique, 1200.0);

    let national = repo.latest(Region::National, m).await.unwrap().unwrap();
    assert_eq!(
        national.mix.expect("mix national").thermique,
        None,
        "le national garde le détail par filière, pas de thermique agrégé"
    );
}

/// Rollup journalier (`Granularity::Daily`) : seaux alignés sur le jour UTC,
/// via la vue matérialisée `measurement_rollup_daily` (distincte de l'horaire).
#[tokio::test]
async fn daily_rollup_buckets_by_utc_day() {
    let m = "test-pg-daily";
    let Some(repo) = setup(m).await else { return };
    // UNIX_EPOCH + n jours = minuit UTC ⇒ bornes de jour nettes.
    let day0 = OffsetDateTime::UNIX_EPOCH + Duration::days(5000);
    let day1 = day0 + Duration::days(1);

    repo.upsert_many(&[
        measurement(m, day0, 10.0, Vintage::Tr, None),
        measurement(m, day0 + Duration::hours(6), 20.0, Vintage::Tr, None),
        measurement(m, day1, 60.0, Vintage::Tr, None),
    ])
    .await
    .unwrap();
    repo.refresh_rollups().await.unwrap();

    let window = TimeRange::new(day0, day1 + Duration::days(1)).unwrap();
    let daily = repo
        .rollup(Region::National, m, window, Granularity::Daily)
        .await
        .unwrap();

    assert_eq!(daily.len(), 2, "deux jours ⇒ deux seaux");
    assert_eq!(daily[0].start, day0);
    assert_eq!(daily[0].stats.average.value(), 15.0);
    assert_eq!(daily[1].start, day1);
    assert_eq!(daily[1].stats.average.value(), 60.0);
}

/// Les agrégats (`stats`) et les plages (`range`) sont cloisonnés par
/// méthodologie : `rte-direct` et `acv-ademe` au même horodatage/région ne se
/// mélangent pas (la méthodologie fait partie de la clé et du filtre).
#[tokio::test]
async fn stats_and_range_isolate_methodology() {
    let m = "test-pg-meth-iso";
    let other = "test-pg-meth-iso-other";
    let Some(repo) = setup(m).await else { return };
    // setup(m) ne purge que `m` : on nettoie aussi l'autre méthodologie.
    sqlx::query("DELETE FROM measurement WHERE methodology_id = $1")
        .bind(other)
        .execute(repo.pool())
        .await
        .expect("nettoyage other");

    let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(5500);
    let step = Duration::minutes(15);

    // Deux points sous `m` (avg 15), un point très élevé sous `other` au même t0.
    repo.upsert_many(&[
        measurement(m, t0, 10.0, Vintage::Tr, None),
        measurement(m, t0 + step, 20.0, Vintage::Tr, None),
        measurement(other, t0, 9999.0, Vintage::Tr, None),
    ])
    .await
    .unwrap();

    let window = TimeRange::new(t0, t0 + step * 2).unwrap();
    let summary = repo
        .stats(Region::National, m, window)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(summary.count, 2, "seuls les points de `m` sont comptés");
    assert_eq!(summary.average.value(), 15.0);
    assert_eq!(
        summary.max.value(),
        20.0,
        "le 9999 de l'autre méthodologie est exclu"
    );

    let got = repo.range(Region::National, m, window).await.unwrap();
    assert_eq!(got.len(), 2);
    assert!(got.iter().all(|p| p.intensity.value() <= 20.0));
}
