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
use carbonfr_core::domain::{Measurement, Region, TimeRange};
use carbonfr_core::ports::{IntensityRepository, RepositoryError};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, QueryBuilder};

use mapping::{backend, dedup_by_key, mix_field, row_to_measurement, vintage_rank};

/// Liste des colonnes, partagée par les requêtes de lecture et d'écriture.
const COLUMNS: &str = "region, at, methodology_id, methodology_version, intensity, vintage_rank, \
     mix_nucleaire, mix_gaz, mix_charbon, mix_fioul, mix_hydraulique, mix_eolien, \
     mix_solaire, mix_bioenergies, mix_pompage, mix_echanges";

/// Taille de paquet pour l'INSERT multi-lignes : 16 colonnes × 1000 = 16 000
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
     mix_pompage = EXCLUDED.mix_pompage, mix_echanges = EXCLUDED.mix_echanges \
     WHERE EXCLUDED.vintage_rank >= measurement.vintage_rank";

/// Implémentation PostgreSQL du port [`IntensityRepository`].
#[derive(Clone)]
pub struct PgIntensityRepository {
    pool: PgPool,
}

impl PgIntensityRepository {
    /// Ouvre un pool de connexions vers `database_url`.
    pub async fn connect(database_url: &str) -> Result<Self, RepositoryError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|e| backend(format!("connexion à PostgreSQL : {e}")))?;
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
                    .push_bind(mix_field(&m.mix, |x| x.echanges));
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
}
