-- Prix spot day-ahead du marché de gros (ADR-0023) : composante **énergie** de
-- la décomposition du prix TRV, au pas horaire, zone de marché française.
-- Source ENTSO-E (documentType A44), ingérée par le poller comme le contexte
-- d'import. Servi à la lecture, joint au mix au plus proche (`/v1/price`).
--
-- Une ligne par horodatage (`at`). Pas de millésime : le day-ahead est publié
-- une fois et n'est pas révisé (contrairement aux mesures RTE). Le prix **peut
-- être négatif** (surproduction renouvelable) : aucune contrainte de signe.
CREATE TABLE IF NOT EXISTS spot_price (
    at             TIMESTAMPTZ      NOT NULL,
    -- Prix spot day-ahead (€/MWh), zone FR. Peut être négatif.
    price_eur_mwh  DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (at)
);
