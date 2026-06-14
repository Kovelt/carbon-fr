-- Mesures d'intensité carbone.
--
-- ADR-0004 : PostgreSQL natif, sans extension (pas de TimescaleDB).
-- ADR-0006 : clé d'unicité (region, at, methodology) + upsert conditionnel au
--            millésime — on n'écrase l'existant que par une qualité >=.
-- ADR-0005 : la méthodologie (id + version) fait partie de la clé.
--
-- Le partitionnement déclaratif mensuel + index BRIN (ADR-0004) est prévu pour
-- la phase 2 (backfill historique), quand le volume le justifie. Au socle, une
-- table simple avec un index btree suffit (volume national récent ~96 lignes/j).

CREATE TABLE IF NOT EXISTS measurement (
    region               TEXT             NOT NULL,
    at                   TIMESTAMPTZ      NOT NULL,
    methodology_id       TEXT             NOT NULL,
    methodology_version  INTEGER          NOT NULL,
    intensity            DOUBLE PRECISION NOT NULL,
    -- Millésime encodé en rang de qualité (0 = tr, 1 = consolidated,
    -- 2 = definitive) : rend l'upsert conditionnel trivial (comparaison >=).
    vintage_rank         SMALLINT         NOT NULL,
    -- Mix de production en MW par filière. Optionnel : toutes les colonnes
    -- NULL ensemble = mix absent (écriture atomique tout-ou-rien).
    mix_nucleaire        DOUBLE PRECISION,
    mix_gaz              DOUBLE PRECISION,
    mix_charbon          DOUBLE PRECISION,
    mix_fioul            DOUBLE PRECISION,
    mix_hydraulique      DOUBLE PRECISION,
    mix_eolien           DOUBLE PRECISION,
    mix_solaire          DOUBLE PRECISION,
    mix_bioenergies      DOUBLE PRECISION,
    mix_pompage          DOUBLE PRECISION,
    mix_echanges         DOUBLE PRECISION,

    CONSTRAINT measurement_pkey
        PRIMARY KEY (region, at, methodology_id, methodology_version),
    CONSTRAINT measurement_vintage_rank_valid
        CHECK (vintage_rank BETWEEN 0 AND 2),
    CONSTRAINT measurement_intensity_nonneg
        CHECK (intensity >= 0)
);

-- Sert « dernière mesure » (region + methodology, ORDER BY at DESC LIMIT 1) et
-- les plages (region + methodology, at dans [start, end)).
CREATE INDEX IF NOT EXISTS measurement_region_methodology_at_idx
    ON measurement (region, methodology_id, at DESC);
