-- Thermique fossile agrégé du mix régional (ADR-0008).
--
-- Le mix régional d'éCO2mix ne détaille pas gaz/charbon/fioul : il fournit un
-- seul champ « thermique ». Colonne optionnelle (NULL au national, où le détail
-- par filière est stocké dans mix_gaz/mix_charbon/mix_fioul).
ALTER TABLE measurement
    ADD COLUMN IF NOT EXISTS mix_thermique DOUBLE PRECISION;
