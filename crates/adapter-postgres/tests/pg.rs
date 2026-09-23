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
    CarbonIntensity, CrossBorderFlow, CrossBorderFlows, CrossBorderSnapshot, GenerationMix,
    Granularity, LoadRecord, Measurement, Methodology, Neighbor, Region, SpotPrice, TimeRange,
    Vintage, WeatherForecast,
};
use carbonfr_core::ports::{
    ApiKeyRepository, ApiTier, ConsumptionRepository, CrossBorderRepository, IntensityRepository,
    SpotPriceRepository, VisitCounter, WeatherRepository,
};
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
    repo.rebuild_rollups().await.unwrap();

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
async fn refresh_rollups_covers_recent_buckets() {
    // Chemin INCRÉMENTAL du poller : refresh_rollups (fenêtre récente) doit
    // réagréger les seaux récemment écrits. On insère à l'heure courante (dans la
    // fenêtre de 7 j) et on vérifie le seau horaire.
    let m = "test-pg-rollup-recent";
    let Some(repo) = setup(m).await else { return };
    let now = OffsetDateTime::now_utc();
    let recent = now.replace_time(time::Time::from_hms(now.hour(), 0, 0).unwrap());

    repo.upsert_many(&[
        measurement(m, recent, 40.0, Vintage::Tr, None),
        measurement(m, recent + Duration::minutes(15), 60.0, Vintage::Tr, None),
    ])
    .await
    .unwrap();

    repo.refresh_rollups().await.unwrap();

    let window = TimeRange::new(recent, recent + Duration::hours(1)).unwrap();
    let hourly = repo
        .rollup(Region::National, m, window, Granularity::Hourly)
        .await
        .unwrap();
    assert_eq!(hourly.len(), 1, "le seau récent doit être réagrégé");
    assert_eq!(hourly[0].stats.average.value(), 50.0); // (40 + 60) / 2
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

#[tokio::test]
async fn consumption_merges_realized_and_forecast() {
    let Some(repo) = setup("test-pg-conso").await else {
        return;
    };
    // Horodatage dédié au test (clé = region + at), nettoyé pour la ré-exécution.
    let t = OffsetDateTime::UNIX_EPOCH + Duration::days(6000);
    sqlx::query("DELETE FROM consumption WHERE at = $1")
        .bind(t)
        .execute(repo.pool())
        .await
        .expect("nettoyage consumption");

    // La prévision arrive d'abord (créneau futur), puis la réalisée : l'upsert
    // fusionne sans qu'un NULL n'écrase l'existant (COALESCE).
    repo.upsert_loads(&[LoadRecord::forecast(t, Region::National, 45_000.0)])
        .await
        .unwrap();
    repo.upsert_loads(&[LoadRecord::realized(t, Region::National, 44_500.0)])
        .await
        .unwrap();

    let window = TimeRange::new(t, t + Duration::minutes(15)).unwrap();
    let got = repo.load_range(Region::National, window).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].realized, Some(44_500.0));
    assert_eq!(got[0].forecast, Some(45_000.0));
}

#[tokio::test]
async fn weather_keeps_run_history_per_valid_time() {
    let Some(repo) = setup("test-pg-weather").await else {
        return;
    };
    let valid = OffsetDateTime::UNIX_EPOCH + Duration::days(6100);
    sqlx::query("DELETE FROM weather_forecast WHERE valid_at = $1")
        .bind(valid)
        .execute(repo.pool())
        .await
        .expect("nettoyage weather");

    let run1 = OffsetDateTime::UNIX_EPOCH + Duration::days(6099);
    let run2 = run1 + Duration::hours(6);
    // Deux runs (run_at distincts) pour la même échéance → deux lignes (anti-fuite).
    repo.upsert_weather(&[
        WeatherForecast {
            valid_at: valid,
            run_at: run1,
            wind: 20.0,
            irradiance: 100.0,
        },
        WeatherForecast {
            valid_at: valid,
            run_at: run2,
            wind: 25.0,
            irradiance: 120.0,
        },
    ])
    .await
    .unwrap();

    let window = TimeRange::new(valid, valid + Duration::minutes(15)).unwrap();
    let got = repo.weather_range(window).await.unwrap();
    assert_eq!(got.len(), 2, "deux runs conservés pour la même échéance");
    // Tri (valid_at, run_at) croissants : run1 avant run2.
    assert_eq!(got[0].run_at, run1);
    assert_eq!(got[1].run_at, run2);
    assert_eq!(got[1].wind, 25.0);

    // F19 : `weather_latest` ne rend que le run le plus récent par échéance
    // (dédup côté base), sans transférer tout l'historique.
    let latest = repo.weather_latest(window).await.unwrap();
    assert_eq!(latest.len(), 1, "une seule ligne par échéance");
    assert_eq!(latest[0].run_at, run2);
    assert_eq!(latest[0].wind, 25.0);

    // Audit 2026-08 : un même couple `(valid_at, run_at)` dupliqué dans le MÊME
    // lot ne fait plus échouer tout l'INSERT (« ON CONFLICT ne peut affecter
    // deux fois la même ligne ») — dédup avant l'upsert, dernière occurrence
    // conservée (même sémantique que l'upsert lui-même).
    repo.upsert_weather(&[
        WeatherForecast {
            valid_at: valid,
            run_at: run2,
            wind: 30.0,
            irradiance: 130.0,
        },
        WeatherForecast {
            valid_at: valid,
            run_at: run2,
            wind: 31.0,
            irradiance: 131.0,
        },
    ])
    .await
    .unwrap();
    let latest = repo.weather_latest(window).await.unwrap();
    assert_eq!(latest.len(), 1, "toujours une seule ligne par échéance");
    assert_eq!(latest[0].run_at, run2);
    assert_eq!(latest[0].wind, 31.0, "dernière occurrence du lot conservée");
}

/// F24 : deux `LoadRecord` **complémentaires** (réalisée seule + prévue seule)
/// pour la même clé `(region, at)` dans un **seul** `upsert_loads` doivent
/// **fusionner** (aucun champ perdu), pas s'écraser.
#[tokio::test]
async fn upsert_loads_merges_complementary_records_in_same_batch() {
    let Some(repo) = setup("test-pg-load-merge").await else {
        return;
    };
    let at = OffsetDateTime::UNIX_EPOCH + Duration::days(7000);
    sqlx::query("DELETE FROM consumption WHERE at = $1")
        .bind(at)
        .execute(repo.pool())
        .await
        .expect("nettoyage consumption");

    let written = repo
        .upsert_loads(&[
            LoadRecord::realized(at, Region::National, 55000.0),
            LoadRecord::forecast(at, Region::National, 54000.0),
        ])
        .await
        .unwrap();
    assert_eq!(written, 1, "une seule ligne fusionnée");

    use sqlx::Row;
    let row = sqlx::query("SELECT realized, forecast FROM consumption WHERE at = $1")
        .bind(at)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    let realized: Option<f64> = row.try_get("realized").unwrap();
    let forecast: Option<f64> = row.try_get("forecast").unwrap();
    assert_eq!(realized, Some(55000.0), "champ réalisé conservé");
    assert_eq!(forecast, Some(54000.0), "champ prévu conservé (pas écrasé)");
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
    repo.rebuild_rollups().await.unwrap();

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

#[tokio::test]
async fn cross_border_snapshot_roundtrips_and_picks_nearest() {
    let Some(repo) = setup("test-pg-xborder").await else {
        return;
    };
    let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(6200);
    let t1 = t0 + Duration::hours(1);
    sqlx::query("DELETE FROM cross_border_flow WHERE at >= $1 AND at <= $2")
        .bind(t0)
        .bind(t1)
        .execute(repo.pool())
        .await
        .expect("nettoyage cross_border");

    let snap = |at, flow_mw, intensity| CrossBorderSnapshot {
        at,
        flows: CrossBorderFlows::new(vec![
            CrossBorderFlow {
                neighbor: Neighbor::Germany,
                flow_mw,
                neighbor_intensity: CarbonIntensity::new(intensity).unwrap(),
            },
            CrossBorderFlow {
                neighbor: Neighbor::Spain,
                flow_mw: -500.0,
                neighbor_intensity: CarbonIntensity::new(180.0).unwrap(),
            },
        ]),
    };

    let written = repo
        .upsert_flows(&[snap(t0, 1000.0, 400.0), snap(t1, 2000.0, 420.0)])
        .await
        .unwrap();
    assert_eq!(written, 4, "2 créneaux × 2 voisins");

    // Pile sur t1 → le snapshot t1, deux frontières.
    let at_t1 = repo.flows_at(t1).await.unwrap().expect("snapshot t1");
    assert_eq!(at_t1.at, t1);
    assert_eq!(at_t1.flows.flows.len(), 2);

    // Entre t0 et t1 → le plus proche ≤, donc t0.
    let between = repo
        .flows_at(t0 + Duration::minutes(30))
        .await
        .unwrap()
        .expect("snapshot ≤ cible");
    assert_eq!(between.at, t0);
    let de = between
        .flows
        .flows
        .iter()
        .find(|f| f.neighbor == Neighbor::Germany)
        .unwrap();
    assert_eq!(de.flow_mw, 1000.0);

    // Avant tout → None.
    assert!(
        repo.flows_at(t0 - Duration::hours(1))
            .await
            .unwrap()
            .is_none()
    );

    // Contexte périmé (dernier snapshot vieux de 2 h > tolérance) → None,
    // plutôt qu'une extension illimitée du dernier snapshot connu.
    assert!(
        repo.flows_at(t1 + Duration::hours(2))
            .await
            .unwrap()
            .is_none()
    );

    // Ré-ingestion du même créneau → mise à jour, pas de doublon.
    let rewritten = repo.upsert_flows(&[snap(t0, 1500.0, 410.0)]).await.unwrap();
    assert_eq!(rewritten, 2);
    let updated = repo.flows_at(t0).await.unwrap().unwrap();
    let de = updated
        .flows
        .flows
        .iter()
        .find(|f| f.neighbor == Neighbor::Germany)
        .unwrap();
    assert_eq!(de.flow_mw, 1500.0, "valeur mise à jour");

    // flows_range : les deux créneaux, chacun avec ses 2 frontières, triés.
    let window = TimeRange::new(t0, t1 + Duration::minutes(1)).unwrap();
    let snapshots = repo.flows_range(window).await.unwrap();
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].at, t0);
    assert_eq!(snapshots[1].at, t1);
    assert_eq!(snapshots[0].flows.flows.len(), 2);
}

#[tokio::test]
async fn spot_price_roundtrips_and_picks_nearest() {
    let Some(repo) = setup("test-pg-spot").await else {
        return;
    };
    // Fenêtre lointaine et dédiée, pour l'isolation entre tests parallèles.
    let t0 = OffsetDateTime::UNIX_EPOCH + Duration::days(6300);
    let t1 = t0 + Duration::hours(1);
    let t2 = t0 + Duration::hours(2);
    sqlx::query("DELETE FROM spot_price WHERE at >= $1 AND at <= $2")
        .bind(t0)
        .bind(t2)
        .execute(repo.pool())
        .await
        .expect("nettoyage spot_price");

    let written = repo
        .upsert_prices(&[
            SpotPrice::new(t0, 42.5).unwrap(),
            SpotPrice::new(t1, -3.1).unwrap(), // prix négatif conservé
        ])
        .await
        .unwrap();
    assert_eq!(written, 2);

    // Pile sur t1 → le prix négatif.
    let at_t1 = repo.price_at(t1).await.unwrap().expect("prix t1");
    assert_eq!(at_t1.at, t1);
    assert_eq!(at_t1.eur_per_mwh, -3.1);

    // Entre t0 et t1 → le plus proche ≤, donc t0.
    let between = repo
        .price_at(t0 + Duration::minutes(30))
        .await
        .unwrap()
        .expect("prix ≤ cible");
    assert_eq!(between.at, t0);
    assert_eq!(between.eur_per_mwh, 42.5);

    // Avant tout prix → None.
    assert!(
        repo.price_at(t0 - Duration::hours(1))
            .await
            .unwrap()
            .is_none()
    );

    // Ré-ingestion du même horodatage → mise à jour, pas de doublon.
    let rewritten = repo
        .upsert_prices(&[SpotPrice::new(t0, 50.0).unwrap()])
        .await
        .unwrap();
    assert_eq!(rewritten, 1);
    let updated = repo.price_at(t0).await.unwrap().unwrap();
    assert_eq!(updated.eur_per_mwh, 50.0, "valeur mise à jour");

    // price_range : les deux créneaux, triés croissants.
    let window = TimeRange::new(t0, t1 + Duration::minutes(1)).unwrap();
    let prices = repo.price_range(window).await.unwrap();
    assert_eq!(prices.len(), 2);
    assert_eq!(prices[0].at, t0);
    assert_eq!(prices[1].at, t1);
}

#[tokio::test]
async fn api_key_resolve_and_upsert() {
    let Some(repo) = setup("test-pg-apikey").await else {
        return;
    };
    let hash = "deadbeefcafe0001";
    sqlx::query("DELETE FROM api_key WHERE key_hash = $1")
        .bind(hash)
        .execute(repo.pool())
        .await
        .expect("nettoyage api_key");

    // Clé inconnue → None.
    assert!(repo.resolve(hash).await.unwrap().is_none());

    // Insertion puis résolution.
    repo.insert_key(hash, ApiTier::Free, "projet-test")
        .await
        .unwrap();
    let record = repo.resolve(hash).await.unwrap().expect("clé résolue");
    assert_eq!(record.tier, ApiTier::Free);
    assert_eq!(record.label, "projet-test");

    // Ré-insertion (même empreinte) → met à jour le libellé, pas de doublon.
    repo.insert_key(hash, ApiTier::Free, "renomme")
        .await
        .unwrap();
    assert_eq!(repo.resolve(hash).await.unwrap().unwrap().label, "renomme");
}

#[tokio::test]
async fn api_key_revoke_removes_key_and_only_its_subscriptions() {
    let Some(repo) = setup("test-pg-apikey-revoke").await else {
        return;
    };
    use carbonfr_core::domain::{Subscription, ThresholdDirection};
    use carbonfr_core::ports::SubscriptionRepository;

    let revoked = "deadbeefcafe0002";
    let kept = "deadbeefcafe0003";
    for hash in [revoked, kept] {
        sqlx::query("DELETE FROM webhook_subscription WHERE owner_key_hash = $1")
            .bind(hash)
            .execute(repo.pool())
            .await
            .expect("nettoyage abonnements");
        sqlx::query("DELETE FROM api_key WHERE key_hash = $1")
            .bind(hash)
            .execute(repo.pool())
            .await
            .expect("nettoyage api_key");
    }
    repo.insert_key(revoked, ApiTier::Free, "a-revoquer")
        .await
        .unwrap();
    repo.insert_key(kept, ApiTier::Free, "a-garder")
        .await
        .unwrap();
    let sub = |id: &str, owner: &str| Subscription {
        id: id.to_string(),
        owner_key_hash: owner.to_string(),
        region: Region::National,
        threshold: 50.0,
        direction: ThresholdDirection::Below,
        callback_url: "https://hooks.example.com/c".to_string(),
        secret: "s3cr3t".to_string(),
        disabled_at: None,
    };
    assert!(repo.create(&sub("wh-revoke-1", revoked), 50).await.unwrap());
    assert!(repo.create(&sub("wh-revoke-2", revoked), 50).await.unwrap());
    assert!(repo.create(&sub("wh-keep-1", kept), 50).await.unwrap());

    // `list_keys` voit les deux clés avec leur nombre d'abonnements.
    let keys = repo.list_keys().await.unwrap();
    let find = |hash: &str| keys.iter().find(|k| k.key_hash == hash).cloned();
    let summary = find(revoked).expect("clé listée");
    assert_eq!(summary.tier, Some(ApiTier::Free));
    assert_eq!(summary.label, "a-revoquer");
    assert_eq!(summary.subscriptions, 2);
    assert_eq!(find(kept).expect("clé listée").subscriptions, 1);

    // Révocation : la clé ET ses abonnements disparaissent, l'autre clé est intacte.
    let revocation = repo
        .revoke_key(revoked)
        .await
        .unwrap()
        .expect("clé révoquée");
    assert_eq!(revocation.subscriptions_removed, 2);
    assert!(repo.resolve(revoked).await.unwrap().is_none());
    assert!(repo.list_for_owner(revoked).await.unwrap().is_empty());
    assert!(repo.resolve(kept).await.unwrap().is_some());
    assert_eq!(repo.list_for_owner(kept).await.unwrap().len(), 1);
    assert!(
        !repo
            .active()
            .await
            .unwrap()
            .iter()
            .any(|s| s.owner_key_hash == revoked)
    );

    // La base refuse tout abonnement dont la clé n'existe pas (FK, migration
    // 0013) : une création concurrente d'une révocation ne peut pas laisser
    // d'orphelin, quel que soit l'ordre des requêtes applicatives.
    assert!(repo.create(&sub("wh-revoke-3", revoked), 50).await.is_err());
    assert!(repo.list_for_owner(revoked).await.unwrap().is_empty());

    // Empreinte inconnue (ou déjà révoquée) : `None`, rien n'est touché.
    assert!(repo.revoke_key(revoked).await.unwrap().is_none());
    assert_eq!(repo.list_for_owner(kept).await.unwrap().len(), 1);

    repo.revoke_key(kept).await.unwrap();
}

/// Désactivation automatique (ADR-0016, addendum 2026-09) : un succès remet le
/// compteur à zéro, N échecs **consécutifs** désactivent l'abonnement, qui sort
/// du watcher (`active`) mais reste listé pour son propriétaire.
#[tokio::test]
async fn webhook_disabled_after_consecutive_failures() {
    let Some(repo) = setup("test-pg-webhook-disable").await else {
        return;
    };
    use carbonfr_core::domain::{Subscription, ThresholdDirection};
    use carbonfr_core::ports::SubscriptionRepository;

    let owner = "deadbeefcafe0005";
    let id = "wh-disable-1";
    sqlx::query("DELETE FROM api_key WHERE key_hash = $1")
        .bind(owner)
        .execute(repo.pool())
        .await
        .expect("nettoyage api_key");
    repo.insert_key(owner, ApiTier::Free, "echecs")
        .await
        .unwrap();
    let sub = Subscription {
        id: id.to_string(),
        owner_key_hash: owner.to_string(),
        region: Region::National,
        threshold: 50.0,
        direction: ThresholdDirection::Below,
        callback_url: "https://hooks.example.com/c".to_string(),
        secret: "s3cr3t".to_string(),
        disabled_at: None,
    };
    assert!(repo.create(&sub, 50).await.unwrap());
    let is_active = |subs: Vec<Subscription>| subs.iter().any(|s| s.id == id);

    // 2 échecs, puis un succès : le compteur repart de zéro.
    assert!(!repo.record_delivery(id, false, 3).await.unwrap());
    assert!(!repo.record_delivery(id, false, 3).await.unwrap());
    assert!(!repo.record_delivery(id, true, 3).await.unwrap());
    // 2 nouveaux échecs : toujours actif (le succès a remis le compteur à 0).
    assert!(!repo.record_delivery(id, false, 3).await.unwrap());
    assert!(!repo.record_delivery(id, false, 3).await.unwrap());
    assert!(is_active(repo.active().await.unwrap()));

    // 3ᵉ échec consécutif : désactivation, signalée une seule fois.
    assert!(repo.record_delivery(id, false, 3).await.unwrap());
    assert!(!is_active(repo.active().await.unwrap()));
    assert!(!repo.record_delivery(id, false, 3).await.unwrap());
    // Un succès tardif (livraison en vol) ne le réactive pas.
    assert!(!repo.record_delivery(id, true, 3).await.unwrap());
    assert!(!is_active(repo.active().await.unwrap()));

    // Toujours listé pour son propriétaire, avec l'instant de désactivation.
    let listed = repo.list_for_owner(owner).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].disabled_at.is_some());

    // Abonnement inconnu : aucune erreur, rien à signaler.
    assert!(!repo.record_delivery("wh-inconnu", false, 1).await.unwrap());

    repo.revoke_key(owner).await.unwrap();
}

/// Scénario de la revue adversariale de `revoke-key` : un `POST /v1/webhooks`
/// est « en vol » (INSERT fait, transaction pas encore validée) quand la
/// révocation démarre. La révocation doit attendre la création, puis emporter
/// l'abonnement — jamais d'orphelin, et un décompte exact.
#[tokio::test]
async fn revoke_key_waits_for_in_flight_subscription_and_removes_it() {
    let Some(repo) = setup("test-pg-apikey-race").await else {
        return;
    };
    use carbonfr_core::ports::SubscriptionRepository;

    let hash = "deadbeefcafe0004";
    sqlx::query("DELETE FROM api_key WHERE key_hash = $1")
        .bind(hash)
        .execute(repo.pool())
        .await
        .expect("nettoyage api_key");
    repo.insert_key(hash, ApiTier::Free, "course")
        .await
        .unwrap();

    // Création en vol : transaction ouverte, ligne insérée, pas encore commitée.
    let mut in_flight = repo.pool().begin().await.expect("begin");
    sqlx::query(
        "INSERT INTO webhook_subscription \
         (id, owner_key_hash, region, threshold, direction, callback_url, secret) \
         VALUES ('wh-race-1', $1, 'national', 50, 'below', 'https://hooks.example.com/c', 's')",
    )
    .bind(hash)
    .execute(&mut *in_flight)
    .await
    .expect("insertion en vol");

    let revoker = {
        let repo = repo.clone();
        tokio::spawn(async move { repo.revoke_key(hash).await })
    };
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        !revoker.is_finished(),
        "la révocation doit attendre la création en vol (verrou sur la clé)"
    );

    in_flight.commit().await.expect("commit de la création");
    let revocation = revoker
        .await
        .expect("tâche de révocation")
        .unwrap()
        .expect("clé révoquée");
    assert_eq!(revocation.subscriptions_removed, 1);
    assert!(
        repo.active()
            .await
            .unwrap()
            .iter()
            .all(|s| s.owner_key_hash != hash),
        "aucun abonnement orphelin ne doit survivre à la révocation"
    );
}

#[tokio::test]
async fn webhook_subscription_crud_scoped_to_owner() {
    let Some(repo) = setup("test-pg-webhook").await else {
        return;
    };
    use carbonfr_core::domain::{Subscription, ThresholdDirection};
    use carbonfr_core::ports::SubscriptionRepository;

    let id = "wh-test-1";
    sqlx::query("DELETE FROM webhook_subscription WHERE id = $1")
        .bind(id)
        .execute(repo.pool())
        .await
        .expect("nettoyage");
    // Un abonnement appartient à une clé existante (FK, migration 0013).
    repo.insert_key("owner-aaa", ApiTier::Free, "proprietaire")
        .await
        .unwrap();

    let sub = Subscription {
        id: id.to_string(),
        owner_key_hash: "owner-aaa".to_string(),
        region: Region::National,
        threshold: 50.0,
        direction: ThresholdDirection::Below,
        callback_url: "https://hooks.example.com/c".to_string(),
        secret: "s3cr3t".to_string(),
        disabled_at: None,
    };
    assert!(repo.create(&sub, 50).await.unwrap());

    // Listé pour son propriétaire, pas pour un autre.
    assert_eq!(repo.list_for_owner("owner-aaa").await.unwrap().len(), 1);
    assert!(repo.list_for_owner("owner-bbb").await.unwrap().is_empty());

    // active() le retourne.
    assert!(repo.active().await.unwrap().iter().any(|s| s.id == id));

    // Suppression par un autre propriétaire : aucun effet.
    assert!(!repo.delete(id, "owner-bbb").await.unwrap());
    assert_eq!(repo.list_for_owner("owner-aaa").await.unwrap().len(), 1);

    // Suppression par le propriétaire : effective.
    assert!(repo.delete(id, "owner-aaa").await.unwrap());
    assert!(repo.list_for_owner("owner-aaa").await.unwrap().is_empty());
}

/// Purge des abonnements désactivés (ADR-0016, addendum 2026-09-23 « Purge des
/// abonnements désactivés ») : seul un abonnement **désactivé depuis plus
/// longtemps** que le seuil disparaît — un abonnement désactivé récemment ou
/// toujours **actif** (quelle que soit son ancienneté) est conservé. Le
/// décompte retourné est exact.
#[tokio::test]
async fn purge_disabled_removes_only_old_disabled_subscriptions() {
    let Some(repo) = setup("test-pg-webhook-purge").await else {
        return;
    };
    use carbonfr_core::domain::{Subscription, ThresholdDirection};
    use carbonfr_core::ports::SubscriptionRepository;

    let owner = "deadbeefcafe0006";
    sqlx::query("DELETE FROM api_key WHERE key_hash = $1")
        .bind(owner)
        .execute(repo.pool())
        .await
        .expect("nettoyage api_key");
    repo.insert_key(owner, ApiTier::Free, "purge")
        .await
        .unwrap();

    let sub = |id: &str| Subscription {
        id: id.to_string(),
        owner_key_hash: owner.to_string(),
        region: Region::National,
        threshold: 50.0,
        direction: ThresholdDirection::Below,
        callback_url: "https://hooks.example.com/c".to_string(),
        secret: "s3cr3t".to_string(),
        disabled_at: None,
    };
    assert!(repo.create(&sub("wh-purge-active"), 50).await.unwrap());
    assert!(repo.create(&sub("wh-purge-recent"), 50).await.unwrap());
    assert!(repo.create(&sub("wh-purge-old"), 50).await.unwrap());

    let now = OffsetDateTime::now_utc();
    // Désactivé il y a 400 jours : au-delà du seuil de 30 jours (défaut).
    sqlx::query("UPDATE webhook_subscription SET disabled_at = $2 WHERE id = $1")
        .bind("wh-purge-old")
        .bind(now - Duration::days(400))
        .execute(repo.pool())
        .await
        .expect("désactivation ancienne");
    // Désactivé il y a 1 jour : sous le seuil, conservé.
    sqlx::query("UPDATE webhook_subscription SET disabled_at = $2 WHERE id = $1")
        .bind("wh-purge-recent")
        .bind(now - Duration::days(1))
        .execute(repo.pool())
        .await
        .expect("désactivation récente");
    // "wh-purge-active" reste actif (disabled_at NULL), quelle que soit la
    // date de création — jamais purgé.

    let cutoff = now - Duration::days(30);
    let purged = repo.purge_disabled(cutoff).await.unwrap();
    assert_eq!(
        purged, 1,
        "seul l'abonnement désactivé depuis 400 j doit être purgé"
    );

    let remaining = repo.list_for_owner(owner).await.unwrap();
    let ids: Vec<&str> = remaining.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"wh-purge-active"));
    assert!(ids.contains(&"wh-purge-recent"));
    assert!(!ids.contains(&"wh-purge-old"));
    assert_eq!(remaining.len(), 2);

    // Ré-exécuter la purge : plus rien à purger, décompte exact à zéro.
    assert_eq!(repo.purge_disabled(cutoff).await.unwrap(), 0);

    repo.revoke_key(owner).await.unwrap();
}
