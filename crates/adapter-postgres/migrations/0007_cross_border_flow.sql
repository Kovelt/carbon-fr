-- Contexte d'import transfrontalier (ADR-0010 §6) : flux net signé par frontière
-- + intensité carbone du voisin, au pas quart d'heure. Entrée du calcul
-- `acv-ademe@2` consumption-based, fait à la lecture (pas de ligne `@2` stockée
-- dans `measurement`).
--
-- Une ligne par (horodatage, voisin) : `flows_at` reconstitue un snapshot en
-- lisant tous les voisins du dernier horodatage ≤ cible.
CREATE TABLE IF NOT EXISTS cross_border_flow (
    at                  TIMESTAMPTZ      NOT NULL,
    neighbor            TEXT             NOT NULL,
    -- Flux net signé (MW), positif = import vers la France.
    flow_mw             DOUBLE PRECISION NOT NULL,
    -- Intensité carbone (cycle de vie) du voisin (gCO₂eq/kWh).
    neighbor_intensity  DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (at, neighbor)
);
