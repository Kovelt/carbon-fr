-- Prévision météo nationale (ADR-0012) : entrée du futur modèle ML.
--
-- Clé **(valid_at, run_at)** : une même échéance (`valid_at`) est prévue
-- plusieurs fois, à des instants de production (`run_at`) différents. Conserver
-- l'historique des `run_at` est ce qui permet l'**anti-fuite** (ADR-0012 §6) :
-- à l'entraînement, n'utiliser que la prévision disponible **avant** l'instant
-- de la prédiction simulée. National uniquement (agrégat).

CREATE TABLE IF NOT EXISTS weather_forecast (
    valid_at   TIMESTAMPTZ      NOT NULL,
    run_at     TIMESTAMPTZ      NOT NULL,
    wind       DOUBLE PRECISION NOT NULL,
    irradiance DOUBLE PRECISION NOT NULL,

    CONSTRAINT weather_forecast_pkey PRIMARY KEY (valid_at, run_at)
);

-- Sert les lectures par échéance, et la sélection « dernier run avant T ».
CREATE INDEX IF NOT EXISTS weather_forecast_valid_run_idx
    ON weather_forecast (valid_at, run_at DESC);
