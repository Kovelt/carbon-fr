-- Rollups (vues matérialisées) horaires et journalières (ADR-0004).
--
-- Servent les séries agrégées (/v1/intensity/stats?interval=…). Le résumé sur
-- un intervalle arbitraire, lui, est calculé à la volée sur `measurement`
-- (exact). Rafraîchissement non incrémental par le poller / le backfill ; à ce
-- volume (~1 Go), c'est sans conséquence (ADR-0004).
--
-- date_trunc(..., 'UTC') : seaux alignés sur l'UTC, indépendants du fuseau de
-- session (déterminisme). L'index unique sur (region, methodology_id, bucket)
-- est requis par REFRESH MATERIALIZED VIEW CONCURRENTLY.

CREATE MATERIALIZED VIEW measurement_rollup_hourly AS
SELECT
    region,
    methodology_id,
    date_trunc('hour', at, 'UTC') AS bucket,
    avg(intensity)               AS avg_intensity,
    min(intensity)               AS min_intensity,
    max(intensity)               AS max_intensity,
    count(*)                     AS n
FROM measurement
GROUP BY region, methodology_id, date_trunc('hour', at, 'UTC');

CREATE UNIQUE INDEX measurement_rollup_hourly_key
    ON measurement_rollup_hourly (region, methodology_id, bucket);

CREATE MATERIALIZED VIEW measurement_rollup_daily AS
SELECT
    region,
    methodology_id,
    date_trunc('day', at, 'UTC') AS bucket,
    avg(intensity)               AS avg_intensity,
    min(intensity)               AS min_intensity,
    max(intensity)               AS max_intensity,
    count(*)                     AS n
FROM measurement
GROUP BY region, methodology_id, date_trunc('day', at, 'UTC');

CREATE UNIQUE INDEX measurement_rollup_daily_key
    ON measurement_rollup_daily (region, methodology_id, bucket);
