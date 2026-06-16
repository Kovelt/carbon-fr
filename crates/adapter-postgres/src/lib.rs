//! # carbonfr-adapter-postgres
//!
//! Adapter **sortant** : implémentation de [`IntensityRepository`] sur
//! PostgreSQL natif, sans extension (ADR-0004), via `sqlx`.
//!
//! Point clé : l'écriture est un **upsert conditionnel au millésime**
//! (ADR-0006). Sur conflit de clé `(region, at, methodology_id,
//! methodology_version)`, la ligne n'est remplacée que si le millésime entrant
//! est de qualité supérieure ou égale — exprimé en SQL par
//! `WHERE EXCLUDED.vintage_rank >= measurement.vintage_rank`.
//!
//! Les requêtes sont construites à l'exécution (pas via les macros `query!`) :
//! le crate compile et `cargo check` passe **sans base de données**.

mod mapping;

use async_trait::async_trait;
use carbonfr_core::domain::{
    CarbonIntensity, CrossBorderFlow, CrossBorderFlows, CrossBorderSnapshot, Granularity,
    IntensityStats, LoadRecord, Measurement, Neighbor, Region, RollupBucket, Subscription,
    ThresholdDirection, TimeRange, VisitStats, WeatherForecast,
};
use carbonfr_core::ports::{
    ApiKeyRecord, ApiKeyRepository, ApiTier, ConsumptionRepository, CrossBorderRepository,
    IntensityRepository, RepositoryError, SubscriptionRepository, VisitCounter, WeatherRepository,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, QueryBuilder, Row};
use time::{Date, OffsetDateTime};

use mapping::{
    backend, dedup_by_key, intensity_stats, mix_field, rollup_row, row_to_measurement, vintage_rank,
};

/// Liste des colonnes, partagée par les requêtes de lecture et d'écriture.
const COLUMNS: &str = "region, at, methodology_id, methodology_version, intensity, vintage_rank, \
     mix_nucleaire, mix_gaz, mix_charbon, mix_fioul, mix_hydraulique, mix_eolien, \
     mix_solaire, mix_bioenergies, mix_pompage, mix_echanges, mix_thermique";

/// Taille de paquet pour l'INSERT multi-lignes : 17 colonnes × 1000 = 17 000
/// paramètres liés, sous la limite de 65 535 de PostgreSQL.
const UPSERT_CHUNK: usize = 1000;

/// Clause d'upsert conditionnel au millésime (ADR-0006), appliquée par ligne :
/// on n'écrase que par une qualité de millésime supérieure ou égale.
const ON_CONFLICT_UPSERT: &str = " ON CONFLICT (region, at, methodology_id, methodology_version) DO UPDATE SET \
     intensity = EXCLUDED.intensity, vintage_rank = EXCLUDED.vintage_rank, \
     mix_nucleaire = EXCLUDED.mix_nucleaire, mix_gaz = EXCLUDED.mix_gaz, \
     mix_charbon = EXCLUDED.mix_charbon, mix_fioul = EXCLUDED.mix_fioul, \
     mix_hydraulique = EXCLUDED.mix_hydraulique, mix_eolien = EXCLUDED.mix_eolien, \
     mix_solaire = EXCLUDED.mix_solaire, mix_bioenergies = EXCLUDED.mix_bioenergies, \
     mix_pompage = EXCLUDED.mix_pompage, mix_echanges = EXCLUDED.mix_echanges, \
     mix_thermique = EXCLUDED.mix_thermique \
     WHERE EXCLUDED.vintage_rank >= measurement.vintage_rank";

/// Implémentation PostgreSQL du port [`IntensityRepository`].
#[derive(Clone)]
pub struct PgIntensityRepository {
    pool: PgPool,
}

impl PgIntensityRepository {
    /// Ouvre un pool de connexions vers `database_url`.
    ///
    /// Taille via `CARBONFR_DB_MAX_CONNECTIONS` (défaut **20** — le pool est
    /// partagé par l'API, le poller et le watcher, et un `REFRESH MATERIALIZED
    /// VIEW CONCURRENTLY` monopolise une connexion le temps de l'agrégat).
    /// `acquire_timeout` borné : sous saturation, une requête **échoue vite**
    /// (→ 500) plutôt que de pendre jusqu'au défaut de 30 s. Recyclage
    /// (`idle`/`max_lifetime`) pour éviter les connexions mortes côté serveur.
    pub async fn connect(database_url: &str) -> Result<Self, RepositoryError> {
        let max_connections = std::env::var("CARBONFR_DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20);
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .idle_timeout(std::time::Duration::from_secs(600))
            .max_lifetime(std::time::Duration::from_secs(1800))
            .connect(database_url)
            .await
            // On ne propage PAS `{e}` : un message d'erreur sqlx de connexion peut
            // contenir le DSN (donc le mot de passe). On reste générique.
            .map_err(|_| backend("connexion à PostgreSQL impossible".to_string()))?;
        Ok(Self { pool })
    }

    /// Construit le repository à partir d'un pool existant (composition root,
    /// tests).
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Accès au pool sous-jacent.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Applique les migrations embarquées (`./migrations`).
    pub async fn migrate(&self) -> Result<(), RepositoryError> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| backend(format!("migration : {e}")))
    }
}

#[async_trait]
impl IntensityRepository for PgIntensityRepository {
    async fn upsert_many(&self, measurements: &[Measurement]) -> Result<usize, RepositoryError> {
        // Dédup par clé : une même ligne ne peut être affectée deux fois dans
        // un seul INSERT ... ON CONFLICT (sinon PostgreSQL refuse).
        let deduped = dedup_by_key(measurements);
        if deduped.is_empty() {
            return Ok(0);
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| backend(format!("ouverture de transaction : {e}")))?;

        let mut written = 0usize;
        for chunk in deduped.chunks(UPSERT_CHUNK) {
            let mut builder = QueryBuilder::new(format!("INSERT INTO measurement ({COLUMNS}) "));
            builder.push_values(chunk.iter().copied(), |mut row, m| {
                row.push_bind(m.region.slug())
                    .push_bind(m.at)
                    .push_bind(m.methodology.id.as_str())
                    .push_bind(m.methodology.version as i32)
                    .push_bind(m.intensity.value())
                    .push_bind(vintage_rank(m.vintage))
                    .push_bind(mix_field(&m.mix, |x| x.nucleaire))
                    .push_bind(mix_field(&m.mix, |x| x.gaz))
                    .push_bind(mix_field(&m.mix, |x| x.charbon))
                    .push_bind(mix_field(&m.mix, |x| x.fioul))
                    .push_bind(mix_field(&m.mix, |x| x.hydraulique))
                    .push_bind(mix_field(&m.mix, |x| x.eolien))
                    .push_bind(mix_field(&m.mix, |x| x.solaire))
                    .push_bind(mix_field(&m.mix, |x| x.bioenergies))
                    .push_bind(mix_field(&m.mix, |x| x.pompage))
                    .push_bind(mix_field(&m.mix, |x| x.echanges))
                    .push_bind(m.mix.as_ref().and_then(|x| x.thermique));
            });
            builder.push(ON_CONFLICT_UPSERT);

            let result = builder
                .build()
                .execute(&mut *tx)
                .await
                .map_err(|e| backend(format!("upsert : {e}")))?;
            written += result.rows_affected() as usize;
        }

        tx.commit()
            .await
            .map_err(|e| backend(format!("commit : {e}")))?;
        Ok(written)
    }

    async fn latest(
        &self,
        region: Region,
        methodology_id: &str,
    ) -> Result<Option<Measurement>, RepositoryError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM measurement \
             WHERE region = $1 AND methodology_id = $2 \
             ORDER BY at DESC LIMIT 1"
        );
        let row = sqlx::query(&sql)
            .bind(region.slug())
            .bind(methodology_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| backend(format!("latest : {e}")))?;

        row.as_ref().map(row_to_measurement).transpose()
    }

    async fn range(
        &self,
        region: Region,
        methodology_id: &str,
        range: TimeRange,
    ) -> Result<Vec<Measurement>, RepositoryError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM measurement \
             WHERE region = $1 AND methodology_id = $2 AND at >= $3 AND at < $4 \
             ORDER BY at ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(region.slug())
            .bind(methodology_id)
            .bind(range.start())
            .bind(range.end())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| backend(format!("range : {e}")))?;

        rows.iter().map(row_to_measurement).collect()
    }

    async fn stats(
        &self,
        region: Region,
        methodology_id: &str,
        range: TimeRange,
    ) -> Result<Option<IntensityStats>, RepositoryError> {
        // Résumé exact calculé sur les mesures brutes (pas sur les rollups).
        let row = sqlx::query(
            "SELECT avg(intensity) AS avg, min(intensity) AS min, max(intensity) AS max, \
                    count(*) AS n \
             FROM measurement \
             WHERE region = $1 AND methodology_id = $2 AND at >= $3 AND at < $4",
        )
        .bind(region.slug())
        .bind(methodology_id)
        .bind(range.start())
        .bind(range.end())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| backend(format!("stats : {e}")))?;

        let count: i64 = row
            .try_get("n")
            .map_err(|e| backend(format!("stats : {e}")))?;
        if count == 0 {
            return Ok(None);
        }
        // Pour count > 0, les agrégats ne sont pas NULL.
        let avg: f64 = row
            .try_get("avg")
            .map_err(|e| backend(format!("stats : {e}")))?;
        let min: f64 = row
            .try_get("min")
            .map_err(|e| backend(format!("stats : {e}")))?;
        let max: f64 = row
            .try_get("max")
            .map_err(|e| backend(format!("stats : {e}")))?;
        Ok(Some(intensity_stats(avg, min, max, count)?))
    }

    async fn rollup(
        &self,
        region: Region,
        methodology_id: &str,
        range: TimeRange,
        granularity: Granularity,
    ) -> Result<Vec<RollupBucket>, RepositoryError> {
        // Le nom de vue provient d'un enum (pas d'entrée utilisateur).
        let view = match granularity {
            Granularity::Hourly => "measurement_rollup_hourly",
            Granularity::Daily => "measurement_rollup_daily",
        };
        let sql = format!(
            "SELECT bucket, avg_intensity, min_intensity, max_intensity, n FROM {view} \
             WHERE region = $1 AND methodology_id = $2 AND bucket >= $3 AND bucket < $4 \
             ORDER BY bucket ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(region.slug())
            .bind(methodology_id)
            .bind(range.start())
            .bind(range.end())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| backend(format!("rollup : {e}")))?;

        rows.iter().map(rollup_row).collect()
    }

    async fn refresh_rollups(&self) -> Result<(), RepositoryError> {
        for view in ["measurement_rollup_hourly", "measurement_rollup_daily"] {
            // CONCURRENTLY : ne verrouille pas les lectures (index unique requis).
            sqlx::query(&format!("REFRESH MATERIALIZED VIEW CONCURRENTLY {view}"))
                .execute(&self.pool)
                .await
                .map_err(|e| backend(format!("refresh {view} : {e}")))?;
        }
        Ok(())
    }
}

#[async_trait]
impl VisitCounter for PgIntensityRepository {
    async fn record_visit(&self, visitor: &str, day: Date) -> Result<VisitStats, RepositoryError> {
        sqlx::query("INSERT INTO visit (visitor_hash, day) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(visitor)
            .bind(day)
            .execute(&self.pool)
            .await
            .map_err(|e| backend(format!("record_visit : {e}")))?;
        self.visit_stats().await
    }

    async fn visit_stats(&self) -> Result<VisitStats, RepositoryError> {
        let row = sqlx::query(
            "SELECT COUNT(DISTINCT visitor_hash) AS uniques, COUNT(*) AS total, MIN(day) AS since \
             FROM visit",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| backend(format!("visit_stats : {e}")))?;

        let uniques: i64 = row
            .try_get("uniques")
            .map_err(|e| backend(format!("visit_stats : {e}")))?;
        let total: i64 = row
            .try_get("total")
            .map_err(|e| backend(format!("visit_stats : {e}")))?;
        let since: Option<Date> = row
            .try_get("since")
            .map_err(|e| backend(format!("visit_stats : {e}")))?;

        Ok(VisitStats {
            unique: uniques.max(0) as u64,
            total: total.max(0) as u64,
            since,
        })
    }
}

#[async_trait]
impl ConsumptionRepository for PgIntensityRepository {
    async fn upsert_loads(&self, loads: &[LoadRecord]) -> Result<usize, RepositoryError> {
        if loads.is_empty() {
            return Ok(0);
        }
        // Dédup par clé `(region, at)` : un même couple ne peut être affecté deux
        // fois dans un seul `ON CONFLICT` (la source peut renvoyer des doublons —
        // p. ex. l'export consolidé). Le dernier l'emporte.
        let mut seen: std::collections::HashMap<(&str, OffsetDateTime), &LoadRecord> =
            std::collections::HashMap::with_capacity(loads.len());
        for load in loads {
            seen.insert((load.region.slug(), load.at), load);
        }
        let deduped: Vec<&LoadRecord> = seen.into_values().collect();

        let mut written = 0usize;
        // Paquets bornés : 4 colonnes × 10 000 = 40 000 paramètres < 65 535.
        for chunk in deduped.chunks(10_000) {
            let mut builder =
                QueryBuilder::new("INSERT INTO consumption (region, at, realized, forecast) ");
            builder.push_values(chunk.iter(), |mut row, load| {
                row.push_bind(load.region.slug())
                    .push_bind(load.at)
                    .push_bind(load.realized)
                    .push_bind(load.forecast);
            });
            // Réalisée et prévue arrivent séparément : un NULL n'écrase pas une
            // valeur déjà présente (COALESCE garde l'existante).
            builder.push(
                " ON CONFLICT (region, at) DO UPDATE SET \
                 realized = COALESCE(EXCLUDED.realized, consumption.realized), \
                 forecast = COALESCE(EXCLUDED.forecast, consumption.forecast)",
            );
            let result = builder
                .build()
                .execute(&self.pool)
                .await
                .map_err(|e| backend(format!("upsert_loads : {e}")))?;
            written += result.rows_affected() as usize;
        }
        Ok(written)
    }

    async fn load_range(
        &self,
        region: Region,
        range: TimeRange,
    ) -> Result<Vec<LoadRecord>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT region, at, realized, forecast FROM consumption \
             WHERE region = $1 AND at >= $2 AND at < $3 ORDER BY at ASC",
        )
        .bind(region.slug())
        .bind(range.start())
        .bind(range.end())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| backend(format!("load_range : {e}")))?;

        rows.iter()
            .map(|row| {
                let slug: String = row.try_get("region").map_err(|e| backend(e.to_string()))?;
                let region = Region::from_slug(&slug)
                    .ok_or_else(|| backend(format!("région inconnue en base : {slug}")))?;
                Ok(LoadRecord {
                    region,
                    at: row.try_get("at").map_err(|e| backend(e.to_string()))?,
                    realized: row
                        .try_get("realized")
                        .map_err(|e| backend(e.to_string()))?,
                    forecast: row
                        .try_get("forecast")
                        .map_err(|e| backend(e.to_string()))?,
                })
            })
            .collect()
    }
}

#[async_trait]
impl WeatherRepository for PgIntensityRepository {
    async fn upsert_weather(
        &self,
        forecasts: &[WeatherForecast],
    ) -> Result<usize, RepositoryError> {
        if forecasts.is_empty() {
            return Ok(0);
        }
        let mut written = 0usize;
        // 4 colonnes × 10 000 = 40 000 paramètres < 65 535.
        for chunk in forecasts.chunks(10_000) {
            let mut builder = QueryBuilder::new(
                "INSERT INTO weather_forecast (valid_at, run_at, wind, irradiance) ",
            );
            builder.push_values(chunk.iter(), |mut row, f| {
                row.push_bind(f.valid_at)
                    .push_bind(f.run_at)
                    .push_bind(f.wind)
                    .push_bind(f.irradiance);
            });
            // Même (valid_at, run_at) ré-ingéré : on rafraîchit les valeurs.
            builder.push(
                " ON CONFLICT (valid_at, run_at) DO UPDATE SET \
                 wind = EXCLUDED.wind, irradiance = EXCLUDED.irradiance",
            );
            let result = builder
                .build()
                .execute(&self.pool)
                .await
                .map_err(|e| backend(format!("upsert_weather : {e}")))?;
            written += result.rows_affected() as usize;
        }
        Ok(written)
    }

    async fn weather_range(
        &self,
        valid: TimeRange,
    ) -> Result<Vec<WeatherForecast>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT valid_at, run_at, wind, irradiance FROM weather_forecast \
             WHERE valid_at >= $1 AND valid_at < $2 ORDER BY valid_at ASC, run_at ASC",
        )
        .bind(valid.start())
        .bind(valid.end())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| backend(format!("weather_range : {e}")))?;

        rows.iter()
            .map(|row| {
                Ok(WeatherForecast {
                    valid_at: row
                        .try_get("valid_at")
                        .map_err(|e| backend(e.to_string()))?,
                    run_at: row.try_get("run_at").map_err(|e| backend(e.to_string()))?,
                    wind: row.try_get("wind").map_err(|e| backend(e.to_string()))?,
                    irradiance: row
                        .try_get("irradiance")
                        .map_err(|e| backend(e.to_string()))?,
                })
            })
            .collect()
    }
}

#[async_trait]
impl CrossBorderRepository for PgIntensityRepository {
    async fn upsert_flows(
        &self,
        snapshots: &[CrossBorderSnapshot],
    ) -> Result<usize, RepositoryError> {
        // Aplatit (snapshot → lignes par voisin) en dédupliquant la clé
        // `(at, neighbor)` (garde la dernière occurrence) : un créneau peut être
        // ré-ingéré, et `ON CONFLICT` interdit deux fois la même clé par requête.
        let mut rows: std::collections::BTreeMap<(OffsetDateTime, &str), &CrossBorderFlow> =
            std::collections::BTreeMap::new();
        for snap in snapshots {
            for flow in &snap.flows.flows {
                rows.insert((snap.at, flow.neighbor.slug()), flow);
            }
        }
        if rows.is_empty() {
            return Ok(0);
        }

        let mut written = 0usize;
        let entries: Vec<((OffsetDateTime, &str), &CrossBorderFlow)> = rows.into_iter().collect();
        // 4 colonnes × 10 000 = 40 000 paramètres < 65 535.
        for chunk in entries.chunks(10_000) {
            let mut builder = QueryBuilder::new(
                "INSERT INTO cross_border_flow (at, neighbor, flow_mw, neighbor_intensity) ",
            );
            builder.push_values(chunk.iter(), |mut row, ((at, slug), flow)| {
                row.push_bind(*at)
                    .push_bind(*slug)
                    .push_bind(flow.flow_mw)
                    .push_bind(flow.neighbor_intensity.value());
            });
            builder.push(
                " ON CONFLICT (at, neighbor) DO UPDATE SET \
                 flow_mw = EXCLUDED.flow_mw, neighbor_intensity = EXCLUDED.neighbor_intensity",
            );
            let result = builder
                .build()
                .execute(&self.pool)
                .await
                .map_err(|e| backend(format!("upsert_flows : {e}")))?;
            written += result.rows_affected() as usize;
        }
        Ok(written)
    }

    async fn flows_at(
        &self,
        at: OffsetDateTime,
    ) -> Result<Option<CrossBorderSnapshot>, RepositoryError> {
        // Dernier horodatage disponible ≤ cible.
        let row = sqlx::query("SELECT max(at) AS at FROM cross_border_flow WHERE at <= $1")
            .bind(at)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| backend(format!("flows_at (max) : {e}")))?;
        let Some(snap_at): Option<OffsetDateTime> =
            row.try_get("at").map_err(|e| backend(e.to_string()))?
        else {
            return Ok(None);
        };

        let rows = sqlx::query(
            "SELECT neighbor, flow_mw, neighbor_intensity FROM cross_border_flow \
             WHERE at = $1 ORDER BY neighbor ASC",
        )
        .bind(snap_at)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| backend(format!("flows_at (rows) : {e}")))?;

        let mut flows = Vec::with_capacity(rows.len());
        for row in &rows {
            let slug: String = row
                .try_get("neighbor")
                .map_err(|e| backend(e.to_string()))?;
            let Some(neighbor) = Neighbor::from_slug(&slug) else {
                continue; // voisin inconnu (donnée héritée) → ignoré
            };
            let flow_mw: f64 = row.try_get("flow_mw").map_err(|e| backend(e.to_string()))?;
            let intensity: f64 = row
                .try_get("neighbor_intensity")
                .map_err(|e| backend(e.to_string()))?;
            let Some(neighbor_intensity) = CarbonIntensity::new(intensity) else {
                continue;
            };
            flows.push(CrossBorderFlow {
                neighbor,
                flow_mw,
                neighbor_intensity,
            });
        }
        if flows.is_empty() {
            return Ok(None);
        }
        Ok(Some(CrossBorderSnapshot {
            at: snap_at,
            flows: CrossBorderFlows::new(flows),
        }))
    }

    async fn flows_range(
        &self,
        range: TimeRange,
    ) -> Result<Vec<CrossBorderSnapshot>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT at, neighbor, flow_mw, neighbor_intensity FROM cross_border_flow \
             WHERE at >= $1 AND at < $2 ORDER BY at ASC, neighbor ASC",
        )
        .bind(range.start())
        .bind(range.end())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| backend(format!("flows_range : {e}")))?;

        // Regroupe les lignes consécutives par horodatage (déjà triées) en snapshots.
        let mut snapshots: Vec<CrossBorderSnapshot> = Vec::new();
        for row in &rows {
            let at: OffsetDateTime = row.try_get("at").map_err(|e| backend(e.to_string()))?;
            let slug: String = row
                .try_get("neighbor")
                .map_err(|e| backend(e.to_string()))?;
            let Some(neighbor) = Neighbor::from_slug(&slug) else {
                continue;
            };
            let flow_mw: f64 = row.try_get("flow_mw").map_err(|e| backend(e.to_string()))?;
            let intensity: f64 = row
                .try_get("neighbor_intensity")
                .map_err(|e| backend(e.to_string()))?;
            let Some(neighbor_intensity) = CarbonIntensity::new(intensity) else {
                continue;
            };
            let flow = CrossBorderFlow {
                neighbor,
                flow_mw,
                neighbor_intensity,
            };
            match snapshots.last_mut() {
                Some(last) if last.at == at => last.flows.flows.push(flow),
                _ => snapshots.push(CrossBorderSnapshot {
                    at,
                    flows: CrossBorderFlows::new(vec![flow]),
                }),
            }
        }
        Ok(snapshots)
    }
}

/// Conversion tier ↔ texte stocké.
fn tier_label(tier: ApiTier) -> &'static str {
    match tier {
        ApiTier::Free => "free",
    }
}

fn parse_tier(label: &str) -> Option<ApiTier> {
    match label {
        "free" => Some(ApiTier::Free),
        _ => None,
    }
}

#[async_trait]
impl ApiKeyRepository for PgIntensityRepository {
    async fn resolve(&self, key_hash: &str) -> Result<Option<ApiKeyRecord>, RepositoryError> {
        let row = sqlx::query("SELECT tier, label FROM api_key WHERE key_hash = $1")
            .bind(key_hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| backend(format!("resolve : {e}")))?;
        let Some(row) = row else { return Ok(None) };
        let tier: String = row.try_get("tier").map_err(|e| backend(e.to_string()))?;
        let label: String = row.try_get("label").map_err(|e| backend(e.to_string()))?;
        // Un tier inconnu (donnée héritée) est traité comme clé absente.
        Ok(parse_tier(&tier).map(|tier| ApiKeyRecord { tier, label }))
    }

    async fn insert_key(
        &self,
        key_hash: &str,
        tier: ApiTier,
        label: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO api_key (key_hash, tier, label) VALUES ($1, $2, $3) \
             ON CONFLICT (key_hash) DO UPDATE SET tier = EXCLUDED.tier, label = EXCLUDED.label",
        )
        .bind(key_hash)
        .bind(tier_label(tier))
        .bind(label)
        .execute(&self.pool)
        .await
        .map_err(|e| backend(format!("insert_key : {e}")))?;
        Ok(())
    }
}

/// Reconstruit un `Subscription` depuis une ligne. Ignore les régions/directions
/// inconnues (donnée héritée) en les signalant par `None`.
fn row_to_subscription(
    row: &sqlx::postgres::PgRow,
) -> Result<Option<Subscription>, RepositoryError> {
    let id: String = row.try_get("id").map_err(|e| backend(e.to_string()))?;
    let owner_key_hash: String = row
        .try_get("owner_key_hash")
        .map_err(|e| backend(e.to_string()))?;
    let region_slug: String = row.try_get("region").map_err(|e| backend(e.to_string()))?;
    let direction_code: String = row
        .try_get("direction")
        .map_err(|e| backend(e.to_string()))?;
    let threshold: f64 = row
        .try_get("threshold")
        .map_err(|e| backend(e.to_string()))?;
    let callback_url: String = row
        .try_get("callback_url")
        .map_err(|e| backend(e.to_string()))?;
    let secret: String = row.try_get("secret").map_err(|e| backend(e.to_string()))?;

    let (Some(region), Some(direction)) = (
        Region::from_slug(&region_slug),
        ThresholdDirection::from_code(&direction_code),
    ) else {
        return Ok(None);
    };
    Ok(Some(Subscription {
        id,
        owner_key_hash,
        region,
        threshold,
        direction,
        callback_url,
        secret,
    }))
}

#[async_trait]
impl SubscriptionRepository for PgIntensityRepository {
    async fn create(&self, s: &Subscription) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO webhook_subscription \
             (id, owner_key_hash, region, threshold, direction, callback_url, secret) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&s.id)
        .bind(&s.owner_key_hash)
        .bind(s.region.slug())
        .bind(s.threshold)
        .bind(s.direction.code())
        .bind(&s.callback_url)
        .bind(&s.secret)
        .execute(&self.pool)
        .await
        .map_err(|e| backend(format!("create subscription : {e}")))?;
        Ok(())
    }

    async fn list_for_owner(
        &self,
        owner_key_hash: &str,
    ) -> Result<Vec<Subscription>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, owner_key_hash, region, threshold, direction, callback_url, secret \
             FROM webhook_subscription WHERE owner_key_hash = $1 ORDER BY created_at ASC",
        )
        .bind(owner_key_hash)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| backend(format!("list_for_owner : {e}")))?;
        Ok(rows
            .iter()
            .filter_map(|r| row_to_subscription(r).transpose())
            .collect::<Result<Vec<_>, _>>()?)
    }

    async fn delete(&self, id: &str, owner_key_hash: &str) -> Result<bool, RepositoryError> {
        // Filtre sur le propriétaire : on ne supprime jamais la ligne d'autrui,
        // et on ne révèle pas son existence (même résultat qu'un id inconnu).
        let result =
            sqlx::query("DELETE FROM webhook_subscription WHERE id = $1 AND owner_key_hash = $2")
                .bind(id)
                .bind(owner_key_hash)
                .execute(&self.pool)
                .await
                .map_err(|e| backend(format!("delete subscription : {e}")))?;
        Ok(result.rows_affected() > 0)
    }

    async fn active(&self) -> Result<Vec<Subscription>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, owner_key_hash, region, threshold, direction, callback_url, secret \
             FROM webhook_subscription",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| backend(format!("active subscriptions : {e}")))?;
        Ok(rows
            .iter()
            .filter_map(|r| row_to_subscription(r).transpose())
            .collect::<Result<Vec<_>, _>>()?)
    }
}
