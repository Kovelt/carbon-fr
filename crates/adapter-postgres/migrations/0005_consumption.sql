-- Charge électrique : consommation réalisée + prévue (ADR-0011 §4).
--
-- Store **dédié**, distinct de `measurement` : la charge n'est pas du carbone,
-- et la prévision (créneaux futurs, RTE J-1/J) n'a pas d'intensité — elle ne
-- pourrait donc pas vivre dans `measurement` (intensité NOT NULL). Réalisée et
-- prévue arrivent séparément (la prévision d'abord, la réalisée ensuite) :
-- l'upsert les fusionne sans qu'un `NULL` n'écrase une valeur déjà connue.

CREATE TABLE IF NOT EXISTS consumption (
    region   TEXT             NOT NULL,
    at       TIMESTAMPTZ      NOT NULL,
    realized DOUBLE PRECISION,
    forecast DOUBLE PRECISION,

    CONSTRAINT consumption_pkey PRIMARY KEY (region, at)
);

-- Sert « dernières charges » et les plages (region, at dans [start, end)).
CREATE INDEX IF NOT EXISTS consumption_region_at_idx ON consumption (region, at DESC);
