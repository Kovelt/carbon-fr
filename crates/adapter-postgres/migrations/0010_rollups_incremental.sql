-- Rollups **incrémentaux** : on remplace les vues matérialisées (rafraîchies en
-- entier à chaque cycle — coût O(table) croissant avec l'historique, cf. audit)
-- par de vraies **tables** upsertées par seau touché (poller : fenêtre récente ;
-- backfill : reconstruction complète). Mêmes colonnes → la lecture `rollup()` est
-- inchangée. date_trunc(..., 'UTC') : seaux alignés UTC (déterminisme).

DROP MATERIALIZED VIEW IF EXISTS measurement_rollup_hourly;
DROP MATERIALIZED VIEW IF EXISTS measurement_rollup_daily;

CREATE TABLE IF NOT EXISTS measurement_rollup_hourly (
    region          TEXT             NOT NULL,
    methodology_id  TEXT             NOT NULL,
    bucket          TIMESTAMPTZ      NOT NULL,
    avg_intensity   DOUBLE PRECISION NOT NULL,
    min_intensity   DOUBLE PRECISION NOT NULL,
    max_intensity   DOUBLE PRECISION NOT NULL,
    n               BIGINT           NOT NULL,
    PRIMARY KEY (region, methodology_id, bucket)
);

CREATE TABLE IF NOT EXISTS measurement_rollup_daily (
    region          TEXT             NOT NULL,
    methodology_id  TEXT             NOT NULL,
    bucket          TIMESTAMPTZ      NOT NULL,
    avg_intensity   DOUBLE PRECISION NOT NULL,
    min_intensity   DOUBLE PRECISION NOT NULL,
    max_intensity   DOUBLE PRECISION NOT NULL,
    n               BIGINT           NOT NULL,
    PRIMARY KEY (region, methodology_id, bucket)
);

-- Amorce depuis l'historique déjà présent (one-shot à la migration).
INSERT INTO measurement_rollup_hourly (region, methodology_id, bucket, avg_intensity, min_intensity, max_intensity, n)
SELECT region, methodology_id, date_trunc('hour', at, 'UTC'),
       avg(intensity), min(intensity), max(intensity), count(*)
FROM measurement
GROUP BY region, methodology_id, date_trunc('hour', at, 'UTC')
ON CONFLICT (region, methodology_id, bucket) DO NOTHING;

INSERT INTO measurement_rollup_daily (region, methodology_id, bucket, avg_intensity, min_intensity, max_intensity, n)
SELECT region, methodology_id, date_trunc('day', at, 'UTC'),
       avg(intensity), min(intensity), max(intensity), count(*)
FROM measurement
GROUP BY region, methodology_id, date_trunc('day', at, 'UTC')
ON CONFLICT (region, methodology_id, bucket) DO NOTHING;
