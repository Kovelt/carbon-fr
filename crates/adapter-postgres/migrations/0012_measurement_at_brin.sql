-- Index BRIN sur `measurement (at)` (audit perf 2026-08) : le rafraîchissement
-- incrémental des rollups (`refresh_rollups`, fenêtre glissante de 7 j) filtre
-- par `at >= $1` **seul** — prédicat que ni la PK `(region, at, …)` ni l'index
-- `(region, methodology_id, at DESC)` de la migration 0001 ne peuvent servir.
-- Chaque cycle du poller faisait donc 2 parcours séquentiels complets de la
-- table (~600 k lignes, en croissance), contredisant le « coût O(7 j) au lieu
-- de O(table entière) » visé par la migration 0010.
--
-- BRIN plutôt que btree : la table est écrite en ordre chronologique (backfill
-- par tranches + poller sur le récent), la corrélation physique de `at` est
-- donc forte — l'index tient en quelques pages et borne le scan aux blocs des
-- 7 derniers jours. Les upserts de révision (tr → consolidated) créent quelques
-- versions de lignes hors ordre : dégradation marginale, acceptée.

CREATE INDEX IF NOT EXISTS measurement_at_brin
    ON measurement USING brin (at);
